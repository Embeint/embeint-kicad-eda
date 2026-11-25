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
constexpr const char* GEOMETRY_UNIT = "libs.kimath.include.geometry.application.ui.p001";
void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


double readNumber( const JSON& aInput, const char* aName )
{
    const std::string text = readText( aInput, aName );
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( std::string( "invalid hex: " ) + aName );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


int readInteger( const JSON& aInput, const char* aName )
{
    return oracle::Integer( aInput, aName, INT_MIN, INT_MAX );
}


const JSON& readArray( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    return aInput.at( aName );
}


VECTOR2I readPoint( const JSON& aInput )
{
    if( !aInput.is_array() || aInput.size() != 2 )
        throw std::invalid_argument( "expected point" );
    int values[2];
    for( int i = 0; i < 2; ++i )
    {
        if( !aInput[i].is_number_integer() || ( aInput[i].is_number_unsigned() && aInput[i].get<uint64_t>() > INT_MAX )
            || aInput[i].get<int64_t>() < INT_MIN || aInput[i].get<int64_t>() > INT_MAX )
            throw std::invalid_argument( "point out of range" );
        values[i] = aInput[i].get<int>();
    }
    return VECTOR2I( values[0], values[1] );
}


void checkInt( int64_t aValue )
{
    if( aValue < INT_MIN || aValue > INT_MAX )
        throw std::invalid_argument( "intermediate overflow" );
}


// Every edge straddling the query belongs to its stripe. These are exactly the
// subtractions executed by Contains, independent of whether it ends inside.
void checkRay( const SHAPE_POLY_SET& aPoly, const VECTOR2I& aPoint )
{
    auto ring = [&]( const SHAPE_LINE_CHAIN& chain )
    {
        if( chain.PointCount() < 3 )
            return;
        for( int j = 0; j < chain.PointCount(); ++j )
        {
            const VECTOR2I& a = chain.CPoint( j );
            const VECTOR2I& b = chain.CPoint( ( j + 1 ) % chain.PointCount() );
            if( ( a.y >= aPoint.y ) == ( b.y >= aPoint.y ) )
                continue;
            checkInt( int64_t( b.x ) - a.x );
            checkInt( int64_t( b.y ) - a.y );
            checkInt( int64_t( aPoint.y ) - a.y );
            checkInt( int64_t( aPoint.x ) - a.x );
        }
    };
    for( int o = 0; o < aPoly.OutlineCount(); ++o )
    {
        ring( aPoly.COutline( o ) );
        for( int h = 0; h < aPoly.HoleCount( o ); ++h )
            ring( aPoly.CHole( o, h ) );
    }
}


// SEG's 64-bit dot/norm intermediates are checked individually, in evaluation
// order. The cross-product branch is widened by the pinned source itself.
int64_t checkLong( __int128 aValue )
{
    if( aValue < INT64_MIN || aValue > INT64_MAX )
        throw std::invalid_argument( "intermediate overflow" );
    return static_cast<int64_t>( aValue );
}


int64_t checkDot( const VECTOR2L& aFirst, const VECTOR2L& aSecond )
{
    const int64_t x = checkLong( __int128( aFirst.x ) * aSecond.x );
    const int64_t y = checkLong( __int128( aFirst.y ) * aSecond.y );
    return checkLong( __int128( x ) + y );
}


void checkDistance( const VECTOR2I& aStart, const VECTOR2I& aEnd, const VECTOR2I& aPoint )
{
    const VECTOR2L ab( int64_t( aEnd.x ) - aStart.x, int64_t( aEnd.y ) - aStart.y );
    const VECTOR2L ap( int64_t( aPoint.x ) - aStart.x, int64_t( aPoint.y ) - aStart.y );
    const int64_t  e = checkDot( ap, ab );
    if( e <= 0 )
    {
        checkDot( ap, ap );
        return;
    }
    const int64_t f = checkDot( ab, ab );
    if( e >= f )
    {
        const VECTOR2L bp( int64_t( aPoint.x ) - aEnd.x, int64_t( aPoint.y ) - aEnd.y );
        checkDot( bp, bp );
    }
}


void checkAccuracy( const SHAPE_POLY_SET& aPoly, const VECTOR2I& aPoint, int aAccuracy )
{
    if( aAccuracy <= 1 )
        return;
    struct EDGE
    {
        VECTOR2I A, B;
        bool     Hole;
    };
    std::vector<EDGE> edges;
    int64_t           yMin = INT_MAX, yMax = INT_MIN;
    auto              ring = [&]( const SHAPE_LINE_CHAIN& chain, bool hole )
    {
        if( chain.PointCount() < 3 )
            return;
        for( int j = 0; j < chain.PointCount(); ++j )
        {
            const VECTOR2I& a = chain.CPoint( j );
            const VECTOR2I& b = chain.CPoint( ( j + 1 ) % chain.PointCount() );
            if( a.y == b.y )
                continue;
            edges.push_back( { a, b, hole } );
            yMin = std::min( yMin, int64_t( std::min( a.y, b.y ) ) );
            yMax = std::max( yMax, int64_t( std::max( a.y, b.y ) ) );
        }
    };
    for( int o = 0; o < aPoly.OutlineCount(); ++o )
    {
        ring( aPoly.COutline( o ), false );
        for( int h = 0; h < aPoly.HoleCount( o ); ++h )
            ring( aPoly.CHole( o, h ), true );
    }
    if( edges.empty() )
        return;
    const int     count = std::clamp( int( std::sqrt( double( edges.size() ) ) ), 1, 65536 );
    const int64_t height = ( yMax - yMin + count - 1 ) / count;
    auto          stripe = [&]( int64_t y )
    {
        return std::clamp( int( ( y - yMin ) / height ), 0, count - 1 );
    };
    // Reconstruct only the execution domain of the distance calls. A later edge
    // must not be pre-rejected after the actual reference would already return.
    for( int s = stripe( int64_t( aPoint.y ) - aAccuracy ); s <= stripe( int64_t( aPoint.y ) + aAccuracy ); ++s )
    {
        for( const EDGE& e : edges )
        {
            if( e.Hole || s < stripe( std::min( e.A.y, e.B.y ) ) || s > stripe( std::max( e.A.y, e.B.y ) ) )
                continue;
            checkDistance( e.A, e.B, aPoint );
            if( SEG( e.A, e.B ).SquaredDistance( aPoint ) <= SEG::Square( aAccuracy ) )
                return;
        }
    }
}


GRID_GEOMETRY readGrid( const JSON& aInput )
{
    checkKeys( aInput, { "priority", "kind", "x", "y" } );
    const auto& p = aInput.at( "priority" );
    if( !p.is_number_integer() || ( !p.is_number_unsigned() && p.get<int64_t>() < 0 ) || p.get<uint64_t>() > UINT_MAX )
        throw std::invalid_argument( "priority out of range" );
    GRID_GEOMETRY grid;
    grid.priority = p.get<unsigned>();
    std::string kind = readText( aInput, "kind" );
    if( kind == "cartesian" )
        grid.kind = GRID_GEOMETRY::KIND::CARTESIAN;
    else if( kind == "polar" )
        grid.kind = GRID_GEOMETRY::KIND::POLAR;
    else
        throw std::invalid_argument( "unknown grid kind" );
    grid.extent = VECTOR2D( readNumber( aInput, "x" ), readNumber( aInput, "y" ) );
    return grid;
}


template <typename T>
JSON ellipse( const std::string& aEndpoint )
{
    ELLIPSE<T> e;
    if( aEndpoint == "default" )
        return JSON{ { "center", { e.Center.x, e.Center.y } },
                     { "radii", { e.MajorRadius, e.MinorRadius } },
                     { "angles",
                       { bits( e.Rotation.AsDegrees() ), bits( e.StartAngle.AsDegrees() ),
                         bits( e.EndAngle.AsDegrees() ) } } };
    VECTOR2<T> aPoint;
    if( aEndpoint == "start" )
        aPoint = e.GetArcStartPoint();
    else if( aEndpoint == "end" )
        aPoint = e.GetArcEndPoint();
    else
        throw std::invalid_argument( "unknown endpoint" );
    if constexpr( std::is_same_v<T, double> )
        return JSON{ bits( aPoint.x ), bits( aPoint.y ) };
    else
        return JSON{ aPoint.x, aPoint.y };
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );
    if( op == "default" )
    {
        checkKeys( aInput, { "op" } );
        JSON constants = JSON::array();
        for( EDA_ANGLE a : { ANGLE_HORIZONTAL, ANGLE_VERTICAL, FULL_CIRCLE, ANGLE_0, ANGLE_45, ANGLE_90, ANGLE_135,
                             ANGLE_180, ANGLE_270, ANGLE_360 } )
            constants.push_back( bits( a.AsDegrees() ) );
        return JSON{ { "angle", bits( EDA_ANGLE().AsDegrees() ) },
                     { "orientation", bits( EDA_ORIENTATION().GetAngle().AsDegrees() ) },
                     { "constants", constants },
                     { "conversion", bits( EDA_ANGLE::DEGREES_TO_RADIANS ) } };
    }
    if( op == "construct" )
    {
        checkKeys( aInput, { "op", "a", "unit" } );
        auto        unit = readText( aInput, "unit" );
        EDA_ANGLE_T type;
        if( unit == "degrees" )
            type = DEGREES_T;
        else if( unit == "radians" )
            type = RADIANS_T;
        else if( unit == "tenths" )
            type = TENTHS_OF_A_DEGREE_T;
        else
            throw std::invalid_argument( "unknown angle unit" );
        return bits( EDA_ANGLE( readNumber( aInput, "a" ), type ).AsDegrees() );
    }
    if( op == "vector" )
    {
        checkKeys( aInput, { "op", "x", "y" } );
        return bits( EDA_ANGLE( VECTOR2D( readNumber( aInput, "x" ), readNumber( aInput, "y" ) ) ).AsDegrees() );
    }
    if( op == "normalize" )
    {
        checkKeys( aInput, { "op", "a", "mode" } );
        EDA_ANGLE a( readNumber( aInput, "a" ), DEGREES_T ), result;
        auto      mode = readText( aInput, "mode" );
        if( mode == "positive" )
            result = a.Normalize();
        else if( mode == "negative" )
            result = a.NormalizeNegative();
        else if( mode == "90" )
            result = a.Normalize90();
        else if( mode == "180" )
            result = a.Normalize180();
        else if( mode == "720" )
            result = a.Normalize720();
        else if( mode == "copy" )
            result = a.Normalized();
        else
            throw std::invalid_argument( "unknown normalization" );
        return JSON{ { "state", bits( a.AsDegrees() ) }, { "returned", bits( result.AsDegrees() ) } };
    }
    if( op == "unary" )
    {
        checkKeys( aInput, { "op", "a", "kind" } );
        double    x = readNumber( aInput, "a" );
        EDA_ANGLE a( x, DEGREES_T );
        auto      kind = readText( aInput, "kind" );
        if( kind == "sin" )
            return bits( a.Sin() );
        if( kind == "cos" )
            return bits( a.Cos() );
        if( kind == "tan" )
            return bits( a.Tan() );
        if( kind == "acos" )
            return bits( EDA_ANGLE::Arccos( x ).AsDegrees() );
        if( kind == "asin" )
            return bits( EDA_ANGLE::Arcsin( x ).AsDegrees() );
        if( kind == "atan" )
            return bits( EDA_ANGLE::Arctan( x ).AsDegrees() );
        if( kind == "radians" )
            return bits( a.AsRadians() );
        if( kind == "negate" )
            return bits( ( -a ).AsDegrees() );
        if( kind == "invert" )
            return bits( a.Invert().AsDegrees() );
        if( kind == "abs" )
            return bits( std::abs( a ).AsDegrees() );
        if( kind == "predicates" )
            return JSON{ a.IsZero(), a.IsHorizontal(), a.IsVertical() };
        if( kind == "tenths" )
            return a.AsTenthsOfADegree();
        throw std::invalid_argument( "unknown unary operation" );
    }
    if( op == "binary" )
    {
        checkKeys( aInput, { "op", "a", "b", "kind" } );
        EDA_ANGLE a( readNumber( aInput, "a" ), DEGREES_T ), b( readNumber( aInput, "b" ), DEGREES_T );
        auto      kind = readText( aInput, "kind" );
        if( kind == "add" )
            return bits( ( a + b ).AsDegrees() );
        if( kind == "sub" )
            return bits( ( a - b ).AsDegrees() );
        if( kind == "mul" )
            return bits( ( a * b.AsDegrees() ).AsDegrees() );
        if( kind == "div" )
            return bits( ( a / b.AsDegrees() ).AsDegrees() );
        if( kind == "ratio" )
            return bits( a / b );
        if( kind == "add_assign" )
        {
            EDA_ANGLE* returned = &( a += b );
            return JSON{ { "value", bits( a.AsDegrees() ) }, { "same", returned == &a } };
        }
        if( kind == "sub_assign" )
        {
            EDA_ANGLE* returned = &( a -= b );
            return JSON{ { "value", bits( a.AsDegrees() ) }, { "same", returned == &a } };
        }
        if( kind == "reflect" )
            return bits( a.Reflect( b ).AsDegrees() );
        if( kind == "atan2" )
            return bits( EDA_ANGLE::Arctan2( a.AsDegrees(), b.AsDegrees() ).AsDegrees() );
        if( kind == "compare" )
            return JSON{ a == b, a != b, a < b, a > b, a <= b, a >= b };
        if( kind == "parallel" )
            return a.IsParallelTo( b );
        if( kind == "snapped" )
            return bits( a.Snapped( b ).AsDegrees() );
        throw std::invalid_argument( "unknown binary operation" );
    }
    if( op == "round" )
    {
        checkKeys( aInput, { "op", "a", "digits" } );
        return bits( EDA_ANGLE( readNumber( aInput, "a" ), DEGREES_T ).Round( readInteger( aInput, "digits" ) ).AsDegrees() );
    }
    if( op == "orientation" )
    {
        checkKeys( aInput, { "op", "a", "b", "axis" } );
        EDA_ANGLE       a( readNumber( aInput, "a" ), DEGREES_T ), b( readNumber( aInput, "b" ), DEGREES_T );
        EDA_ORIENTATION o( a ), p( b ), x( o ), y( o ), assigned;
        x += b;
        y -= b;
        assigned = a;
        JSON values = JSON::array();
        for( EDA_ANGLE angle :
             { o.GetAngle(), ( -o ).GetAngle(), ( o + b ).GetAngle(), ( o - b ).GetAngle(), ( a + p ).GetAngle(),
               ( a - p ).GetAngle(), o - p, x.GetAngle(), y.GetAngle(), assigned.GetAngle(),
               o.Reflect( EDA_ANGLE( readNumber( aInput, "axis" ), DEGREES_T ) ).GetAngle() } )
            values.push_back( bits( angle.AsDegrees() ) );
        return JSON{ { "values", values }, { "equal", o == p } };
    }
    if( op == "stream" )
    {
        checkKeys( aInput, { "op", "a", "orientation", "precision", "width", "fill", "format", "showpos", "uppercase" } );
        int precision = readInteger( aInput, "precision" ), width = readInteger( aInput, "width" );
        if( precision > 512 || width > 512 )
            throw std::invalid_argument( "resource limit" );
        std::ostringstream s;
        s.imbue( std::locale::classic() );
        s.precision( precision );
        s.width( width );
        auto fill = readText( aInput, "fill" );
        if( fill.empty() )
            throw std::invalid_argument( "invalid fill" );
        s.fill( fill[0] );
        auto format = readText( aInput, "format" );
        if( format == "fixed" )
            s << std::fixed;
        if( format == "scientific" )
            s << std::scientific;
        if( format == "hex" )
            s << std::hexfloat;
        if( aInput.at( "showpos" ).get<bool>() )
            s << std::showpos;
        if( aInput.at( "uppercase" ).get<bool>() )
            s << std::uppercase;
        EDA_ANGLE a( readNumber( aInput, "a" ), DEGREES_T );
        if( aInput.at( "orientation" ).get<bool>() )
            s << EDA_ORIENTATION( a );
        else
            s << a;
        return JSON{ { "text", s.str() }, { "width", s.width() } };
    }
    if( op == "ellipse" )
    {
        checkKeys( aInput, { "op", "type", "endpoint" } );
        if( readText( aInput, "type" ) == "i32" )
            return ellipse<int>( readText( aInput, "endpoint" ) );
        if( readText( aInput, "type" ) == "f64" )
            return ellipse<double>( readText( aInput, "endpoint" ) );
        throw std::invalid_argument( "unknown ellipse type" );
    }
    if( op == "grid" )
    {
        checkKeys( aInput, { "op", "a", "b" } );
        return readGrid( aInput.at( "a" ) ).TakesPrecedenceOver( readGrid( aInput.at( "b" ) ) );
    }
    if( op == "poly" )
    {
        checkKeys( aInput, { "op", "builds" } );
        POLY_YSTRIPES_INDEX index;
        JSON                results = JSON::array();
        for( const auto& build : readArray( aInput, "builds" ) )
        {
            checkKeys( build, { "polygons", "point", "accuracy" } );
            SHAPE_POLY_SET aPoly;
            int64_t        total = 0;
            for( const auto& p : readArray( build, "polygons" ) )
            {
                checkKeys( p, { "outline", "holes" } );
                int  o = aPoly.NewOutline();
                auto append = [&]( const JSON& points, int hole )
                {
                    if( !points.is_array() )
                        throw std::invalid_argument( "expected hole" );
                    total += points.size();
                    checkInt( total );
                    // Do not remove repeated vertices: PointCount is part of Build.
                    for( const auto& aPoint : points )
                    {
                        const auto pt = readPoint( aPoint );
                        aPoly.Append( pt.x, pt.y, o, hole, true );
                    }
                };
                append( readArray( p, "outline" ), -1 );
                for( const auto& h : readArray( p, "holes" ) )
                    append( h, aPoly.NewHole( o ) );
            }
            index.Build( aPoly );
            const auto aPoint = readPoint( build.at( "point" ) );
            checkRay( aPoly, aPoint );
            if( build.at( "accuracy" ).is_null() )
                results.push_back( index.Contains( aPoint ) );
            else
            {
                const int aAccuracy = readInteger( build, "accuracy" );
                if( aAccuracy > 1 && !index.Contains( aPoint, 0 ) )
                    checkAccuracy( aPoly, aPoint, aAccuracy );
                results.push_back( index.Contains( aPoint, aAccuracy ) );
            }
        }
        return results;
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace
namespace oracle
{
void RegisterGeometryHeaders( REGISTRY& aRegistry )
{
    aRegistry.Add( GEOMETRY_UNIT, "geometry/{eda_angle,ellipse,grid_geometry,poly_ystripes_index}.h", run );
}
} // namespace oracle
