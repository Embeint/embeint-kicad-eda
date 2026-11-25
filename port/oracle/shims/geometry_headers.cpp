// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned APIs; arithmetic below guards undefined inputs, never computes replies.
#include "registry.h"
#include <geometry/eda_angle.h>
#include <geometry/ellipse.h>
#include <geometry/grid_geometry.h>
#include <geometry/poly_ystripes_index.h>
#include <bit>
#include <climits>
#include <iomanip>
#include <locale>
#include <sstream>

namespace
{
using oracle::JSON;
constexpr const char* UNIT = "libs.kimath.include.geometry.application.ui.p001";
void Keys( const JSON& v, std::initializer_list<const char*> fields )
{
    if( !v.is_object() ) throw std::invalid_argument( "expected object" );
    if( v.size() != fields.size() ) throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : fields )
        if( !v.contains( field ) ) throw std::invalid_argument( "unexpected or missing field" );
}
std::string Text( const JSON& v, const char* name )
{
    if( !v.contains( name ) || !v.at( name ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + name );
    return v.at( name ).get<std::string>();
}
double Number( const JSON& v, const char* name )
{
    const std::string text = Text( v, name );
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( std::string( "invalid hex: " ) + name );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}
JSON Bits( double value )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( value );
    return s.str();
}
int Int( const JSON& v, const char* name ) { return oracle::Integer( v, name, INT_MIN, INT_MAX ); }
const JSON& Array( const JSON& v, const char* name )
{
    if( !v.contains( name ) || !v.at( name ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + name );
    return v.at( name );
}
VECTOR2I Point( const JSON& v )
{
    if( !v.is_array() || v.size() != 2 ) throw std::invalid_argument( "expected point" );
    int values[2];
    for( int i = 0; i < 2; ++i )
    {
        if( !v[i].is_number_integer() || ( v[i].is_number_unsigned() && v[i].get<uint64_t>() > INT_MAX )
            || v[i].get<int64_t>() < INT_MIN || v[i].get<int64_t>() > INT_MAX )
            throw std::invalid_argument( "point out of range" );
        values[i] = v[i].get<int>();
    }
    return VECTOR2I( values[0], values[1] );
}
void CheckInt( int64_t value )
{
    if( value < INT_MIN || value > INT_MAX ) throw std::invalid_argument( "intermediate overflow" );
}
// Every edge straddling the query belongs to its stripe. These are exactly the
// subtractions executed by Contains, independent of whether it ends inside.
void CheckRay( const SHAPE_POLY_SET& poly, const VECTOR2I& point )
{
    auto ring = [&]( const SHAPE_LINE_CHAIN& chain )
    {
        if( chain.PointCount() < 3 ) return;
        for( int j = 0; j < chain.PointCount(); ++j )
        {
            const auto& a = chain.CPoint( j );
            const auto& b = chain.CPoint( ( j + 1 ) % chain.PointCount() );
            if( ( a.y >= point.y ) == ( b.y >= point.y ) ) continue;
            CheckInt( int64_t( b.x ) - a.x ); CheckInt( int64_t( b.y ) - a.y );
            CheckInt( int64_t( point.y ) - a.y ); CheckInt( int64_t( point.x ) - a.x );
        }
    };
    for( int o = 0; o < poly.OutlineCount(); ++o )
    {
        ring( poly.COutline( o ) );
        for( int h = 0; h < poly.HoleCount( o ); ++h ) ring( poly.CHole( o, h ) );
    }
}
GRID_GEOMETRY Grid( const JSON& v )
{
    Keys( v, { "priority", "kind", "x", "y" } );
    const auto& p = v.at( "priority" );
    if( !p.is_number_integer() || ( !p.is_number_unsigned() && p.get<int64_t>() < 0 )
        || p.get<uint64_t>() > UINT_MAX ) throw std::invalid_argument( "priority out of range" );
    GRID_GEOMETRY grid;
    grid.priority = p.get<unsigned>();
    std::string kind = Text( v, "kind" );
    if( kind == "cartesian" ) grid.kind = GRID_GEOMETRY::KIND::CARTESIAN;
    else if( kind == "polar" ) grid.kind = GRID_GEOMETRY::KIND::POLAR;
    else throw std::invalid_argument( "unknown grid kind" );
    grid.extent = VECTOR2D( Number( v, "x" ), Number( v, "y" ) );
    return grid;
}
template<typename T> JSON Ellipse( const std::string& endpoint )
{
    ELLIPSE<T> e;
    if( endpoint == "default" ) return JSON{ { "center", { e.Center.x, e.Center.y } },
        { "radii", { e.MajorRadius, e.MinorRadius } },
        { "angles", { Bits( e.Rotation.AsDegrees() ), Bits( e.StartAngle.AsDegrees() ), Bits( e.EndAngle.AsDegrees() ) } } };
    VECTOR2<T> point;
    if( endpoint == "start" ) point = e.GetArcStartPoint();
    else if( endpoint == "end" ) point = e.GetArcEndPoint();
    else throw std::invalid_argument( "unknown endpoint" );
    if constexpr( std::is_same_v<T, double> ) return JSON{ Bits( point.x ), Bits( point.y ) };
    else return JSON{ point.x, point.y };
}
JSON Run( const JSON& v )
{
    const std::string op = Text( v, "op" );
    if( op == "default" )
    {
        Keys( v, { "op" } );
        JSON constants = JSON::array();
        for( EDA_ANGLE a : { ANGLE_HORIZONTAL, ANGLE_VERTICAL, FULL_CIRCLE, ANGLE_0, ANGLE_45,
                             ANGLE_90, ANGLE_135, ANGLE_180, ANGLE_270, ANGLE_360 } )
            constants.push_back( Bits( a.AsDegrees() ) );
        return JSON{ { "angle", Bits( EDA_ANGLE().AsDegrees() ) },
                     { "orientation", Bits( EDA_ORIENTATION().GetAngle().AsDegrees() ) },
                     { "constants", constants }, { "conversion", Bits( EDA_ANGLE::DEGREES_TO_RADIANS ) } };
    }
    if( op == "construct" )
    {
        Keys( v, { "op", "a", "unit" } );
        auto unit = Text( v, "unit" );
        EDA_ANGLE_T type;
        if( unit == "degrees" ) type = DEGREES_T;
        else if( unit == "radians" ) type = RADIANS_T;
        else if( unit == "tenths" ) type = TENTHS_OF_A_DEGREE_T;
        else throw std::invalid_argument( "unknown angle unit" );
        return Bits( EDA_ANGLE( Number( v, "a" ), type ).AsDegrees() );
    }
    if( op == "vector" )
    {
        Keys( v, { "op", "x", "y" } );
        return Bits( EDA_ANGLE( VECTOR2D( Number( v, "x" ), Number( v, "y" ) ) ).AsDegrees() );
    }
    if( op == "normalize" )
    {
        Keys( v, { "op", "a", "mode" } );
        EDA_ANGLE a( Number( v, "a" ), DEGREES_T ), result;
        auto mode = Text( v, "mode" );
        if( mode == "positive" ) result = a.Normalize();
        else if( mode == "negative" ) result = a.NormalizeNegative();
        else if( mode == "90" ) result = a.Normalize90();
        else if( mode == "180" ) result = a.Normalize180();
        else if( mode == "720" ) result = a.Normalize720();
        else if( mode == "copy" ) result = a.Normalized();
        else throw std::invalid_argument( "unknown normalization" );
        return JSON{ { "state", Bits( a.AsDegrees() ) }, { "returned", Bits( result.AsDegrees() ) } };
    }
    if( op == "unary" )
    {
        Keys( v, { "op", "a", "kind" } );
        double x = Number( v, "a" ); EDA_ANGLE a( x, DEGREES_T ); auto kind = Text( v, "kind" );
        if( kind == "sin" ) return Bits( a.Sin() );
        if( kind == "cos" ) return Bits( a.Cos() );
        if( kind == "tan" ) return Bits( a.Tan() );
        if( kind == "acos" ) return Bits( EDA_ANGLE::Arccos( x ).AsDegrees() );
        if( kind == "asin" ) return Bits( EDA_ANGLE::Arcsin( x ).AsDegrees() );
        if( kind == "atan" ) return Bits( EDA_ANGLE::Arctan( x ).AsDegrees() );
        if( kind == "radians" ) return Bits( a.AsRadians() );
        if( kind == "negate" ) return Bits( ( -a ).AsDegrees() );
        if( kind == "invert" ) return Bits( a.Invert().AsDegrees() );
        if( kind == "abs" ) return Bits( std::abs( a ).AsDegrees() );
        if( kind == "predicates" ) return JSON{ a.IsZero(), a.IsHorizontal(), a.IsVertical() };
        if( kind == "tenths" ) return a.AsTenthsOfADegree();
        throw std::invalid_argument( "unknown unary operation" );
    }
    if( op == "binary" )
    {
        Keys( v, { "op", "a", "b", "kind" } );
        EDA_ANGLE a( Number( v, "a" ), DEGREES_T ), b( Number( v, "b" ), DEGREES_T );
        auto kind = Text( v, "kind" );
        if( kind == "add" ) return Bits( ( a + b ).AsDegrees() );
        if( kind == "sub" ) return Bits( ( a - b ).AsDegrees() );
        if( kind == "mul" ) return Bits( ( a * b.AsDegrees() ).AsDegrees() );
        if( kind == "div" ) return Bits( ( a / b.AsDegrees() ).AsDegrees() );
        if( kind == "ratio" ) return Bits( a / b );
        if( kind == "add_assign" ) { a += b; return Bits( a.AsDegrees() ); }
        if( kind == "sub_assign" ) { a -= b; return Bits( a.AsDegrees() ); }
        if( kind == "reflect" ) return Bits( a.Reflect( b ).AsDegrees() );
        if( kind == "atan2" ) return Bits( EDA_ANGLE::Arctan2( a.AsDegrees(), b.AsDegrees() ).AsDegrees() );
        if( kind == "compare" ) return JSON{ a == b, a != b, a < b, a > b, a <= b, a >= b };
        if( kind == "parallel" ) return a.IsParallelTo( b );
        if( kind == "snapped" ) return Bits( a.Snapped( b ).AsDegrees() );
        throw std::invalid_argument( "unknown binary operation" );
    }
    if( op == "round" )
    {
        Keys( v, { "op", "a", "digits" } );
        return Bits( EDA_ANGLE( Number( v, "a" ), DEGREES_T ).Round( Int( v, "digits" ) ).AsDegrees() );
    }
    if( op == "orientation" )
    {
        Keys( v, { "op", "a", "b", "axis" } );
        EDA_ANGLE a( Number( v, "a" ), DEGREES_T ), b( Number( v, "b" ), DEGREES_T );
        EDA_ORIENTATION o( a ), p( b ), x( o ), y( o ), assigned;
        x += b; y -= b; assigned = a;
        JSON values = JSON::array();
        for( EDA_ANGLE angle : { o.GetAngle(), ( -o ).GetAngle(), ( o + b ).GetAngle(),
            ( o - b ).GetAngle(), ( a + p ).GetAngle(), ( a - p ).GetAngle(), o - p,
            x.GetAngle(), y.GetAngle(), assigned.GetAngle(), o.Reflect( EDA_ANGLE( Number( v, "axis" ), DEGREES_T ) ).GetAngle() } )
            values.push_back( Bits( angle.AsDegrees() ) );
        return JSON{ { "values", values }, { "equal", o == p } };
    }
    if( op == "stream" )
    {
        Keys( v, { "op", "a", "orientation", "precision", "width", "fill", "format", "showpos", "uppercase" } );
        int precision = Int( v, "precision" ), width = Int( v, "width" );
        if( precision > 512 || width > 512 ) throw std::invalid_argument( "resource limit" );
        std::ostringstream s; s.imbue( std::locale::classic() );
        s.precision( precision ); s.width( width );
        auto fill = Text( v, "fill" ); if( fill.empty() ) throw std::invalid_argument( "invalid fill" );
        s.fill( fill[0] ); auto format = Text( v, "format" );
        if( format == "fixed" ) s << std::fixed;
        if( format == "scientific" ) s << std::scientific;
        if( format == "hex" ) s << std::hexfloat;
        if( v.at( "showpos" ).get<bool>() ) s << std::showpos;
        if( v.at( "uppercase" ).get<bool>() ) s << std::uppercase;
        EDA_ANGLE a( Number( v, "a" ), DEGREES_T );
        if( v.at( "orientation" ).get<bool>() ) s << EDA_ORIENTATION( a ); else s << a;
        return JSON{ { "text", s.str() }, { "width", s.width() } };
    }
    if( op == "ellipse" )
    {
        Keys( v, { "op", "type", "endpoint" } );
        if( Text( v, "type" ) == "i32" ) return Ellipse<int>( Text( v, "endpoint" ) );
        if( Text( v, "type" ) == "f64" ) return Ellipse<double>( Text( v, "endpoint" ) );
        throw std::invalid_argument( "unknown ellipse type" );
    }
    if( op == "grid" )
    {
        Keys( v, { "op", "a", "b" } );
        return Grid( v.at( "a" ) ).TakesPrecedenceOver( Grid( v.at( "b" ) ) );
    }
    if( op == "poly" )
    {
        Keys( v, { "op", "builds" } );
        POLY_YSTRIPES_INDEX index;
        JSON results = JSON::array();
        for( const auto& build : Array( v, "builds" ) )
        {
            Keys( build, { "polygons", "point", "accuracy" } );
            SHAPE_POLY_SET poly;
            int64_t total = 0;
            for( const auto& p : Array( build, "polygons" ) )
            {
                Keys( p, { "outline", "holes" } );
                int o = poly.NewOutline();
                auto append = [&]( const JSON& points, int hole )
                {
                    if( !points.is_array() ) throw std::invalid_argument( "expected hole" );
                    total += points.size(); CheckInt( total );
                    // Do not remove repeated vertices: PointCount is part of Build.
                    for( const auto& point : points )
                    {
                        const auto pt = Point( point );
                        poly.Append( pt.x, pt.y, o, hole, true );
                    }
                };
                append( Array( p, "outline" ), -1 );
                for( const auto& h : Array( p, "holes" ) ) append( h, poly.NewHole( o ) );
            }
            index.Build( poly );
            const auto point = Point( build.at( "point" ) );
            CheckRay( poly, point );
            if( build.at( "accuracy" ).is_null() ) results.push_back( index.Contains( point ) );
            else results.push_back( index.Contains( point, Int( build, "accuracy" ) ) );
        }
        return results;
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace
namespace oracle
{
void RegisterGeometryHeaders( REGISTRY& registry )
{
    registry.Add( UNIT, "geometry/{eda_angle,ellipse,grid_geometry,poly_ystripes_index}.h", Run );
}
}
