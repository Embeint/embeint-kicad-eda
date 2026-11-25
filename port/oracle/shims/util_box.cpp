// SPDX-License-Identifier: GPL-3.0-or-later
// This adapter calls the unmodified pinned headers; it never computes reference replies.
#include "registry.h"
#include <math/box2.h>
#include <math/box2_minmax.h>
#include <bit>
#include <iomanip>
#include <locale>
#include <sstream>
#include <climits>
namespace
{
using oracle::JSON;
constexpr const char* UTIL_BOX_UNIT = "libs.kimath.include.math.application.ui.p001";
std::string           text( const JSON& v, const char* k )
{
    if( !v.contains( k ) || !v.at( k ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + k );
    return v.at( k ).get<std::string>();
}
void keys( const JSON& v, std::vector<std::string> fields )
{
    if( !v.is_object() )
        throw std::invalid_argument( "expected object" );
    if( v.size() != fields.size() )
        throw std::invalid_argument( "unexpected or missing field" );
    for( const auto& f : fields )
        if( !v.contains( f ) )
            throw std::invalid_argument( "unexpected or missing field" );
}
template <class T>
T read( const JSON& v )
{
    if constexpr( std::is_floating_point_v<T> )
    {
        if( !v.is_string() )
            throw std::invalid_argument( "expected hex" );
        auto s = v.get<std::string>();
        if( s.size() != sizeof( T ) * 2 || s.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
            throw std::invalid_argument( "invalid hex" );
        if constexpr( sizeof( T ) == 8 )
            return std::bit_cast<double>( std::stoull( s, nullptr, 16 ) );
        else
            return std::bit_cast<float>( uint32_t( std::stoul( s, nullptr, 16 ) ) );
    }
    else
    {
        if( !v.is_number_integer() )
            throw std::invalid_argument( "integer out of range" );
        if( v.is_number_unsigned() )
        {
            auto n = v.get<uint64_t>();
            if( n > uint64_t( std::numeric_limits<T>::max() ) )
                throw std::invalid_argument( "integer out of range" );
            return T( n );
        }
        auto n = v.get<int64_t>();
        if constexpr( std::is_unsigned_v<T> )
        {
            if( n < 0 || uint64_t( n ) > std::numeric_limits<T>::max() )
                throw std::invalid_argument( "integer out of range" );
        }
        else if( n < std::numeric_limits<T>::lowest() || n > std::numeric_limits<T>::max() )
            throw std::invalid_argument( "integer out of range" );
        return T( n );
    }
}
template <class T>
JSON out( T v )
{
    if constexpr( std::is_floating_point_v<T> )
    {
        std::ostringstream s;
        s.imbue( std::locale::classic() );
        s << std::hex << std::setfill( '0' ) << std::setw( sizeof( T ) * 2 );
        if constexpr( sizeof( T ) == 8 )
            s << std::bit_cast<uint64_t>( v );
        else
            s << std::bit_cast<uint32_t>( v );
        return s.str();
    }
    else
        return v;
}
template <class T>
VECTOR2<T> point( const JSON& v )
{
    if( !v.is_array() || v.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return { read<T>( v[0] ), read<T>( v[1] ) };
}
template <class T>
JSON vec( const VECTOR2<T>& p )
{
    return JSON{ out( p.x ), out( p.y ) };
}
void undefined()
{
    throw std::invalid_argument( "undefined box/util operation" );
}
template <class T>
T checked( __int128 n )
{
    if( n < __int128( std::numeric_limits<T>::lowest() ) || n > __int128( std::numeric_limits<T>::max() ) )
        undefined();
    return T( n );
}
template <class T>
T add( T a, T b )
{
    if constexpr( std::is_integral_v<T> )
        return checked<T>( __int128( a ) + b );
    else
        return a + b;
}
template <class T>
T sub( T a, T b )
{
    if constexpr( std::is_integral_v<T> )
        return checked<T>( __int128( a ) - b );
    else
        return a - b;
}
template <class T>
T mul( T a, T b )
{
    if constexpr( std::is_integral_v<T> )
        return checked<T>( __int128( a ) * b );
    else
        return a * b;
}
template <class T>
void castInt( T v )
{
    if constexpr( std::is_floating_point_v<T> )
    {
        if( !std::isfinite( v ) || std::trunc( v ) < INT_MIN || std::trunc( v ) > INT_MAX )
            undefined();
    }
}
// Domain-only checks follow the source intermediates, never replace the result.
template <class T>
void normalizeDomain( const BOX2<VECTOR2<T>>& b )
{
    using E = typename VECTOR2<T>::extended_type;
    auto p = b.GetOrigin();
    auto s = b.GetSize();
    if( s.y < 0 )
    {
        auto sy = sub( E( 0 ), s.y );
        sub( E( p.y ), sy );
    }
    if( s.x < 0 )
    {
        auto sx = sub( E( 0 ), s.x );
        sub( E( p.x ), sx );
    }
}
template <class T>
void endsDomain( const BOX2<VECTOR2<T>>& b )
{
    using E = typename VECTOR2<T>::extended_type;
    add( E( b.GetX() ), b.GetWidth() );
    add( E( b.GetY() ), b.GetHeight() );
}
template <class T>
JSON state( const BOX2<VECTOR2<T>>& b )
{
    return JSON{ { "pos", vec( b.GetOrigin() ) }, { "size", vec( b.GetSize() ) }, { "valid", b.IsValid() } };
}
template <class T>
JSON mutState( BOX2<VECTOR2<T>>& a, BOX2<VECTOR2<T>>& returned )
{
    JSON result = state( a );
    result["same_ref"] = &a == &returned;
    return result;
}

template <class T>
BOX2<VECTOR2<T>> boxRead( const JSON& v )
{
    using B = BOX2<VECTOR2<T>>;
    using E = typename B::ecoord_type;
    auto mode = text( v, "mode" );
    B    b;
    if( mode == "default" )
    {
        keys( v, { "mode" } );
        return b;
    }
    if( mode == "maximum" )
    {
        keys( v, { "mode" } );
        b.SetMaximum();
        return b;
    }
    if( mode == "at" )
    {
        keys( v, { "mode", "pos" } );
        return B( point<T>( v.at( "pos" ) ) );
    }
    if( mode == "corners" )
    {
        keys( v, { "mode", "pos", "end" } );
        auto p = point<T>( v.at( "pos" ) );
        auto q = point<T>( v.at( "end" ) );
        sub( q.x, p.x );
        sub( q.y, p.y );
        return B::ByCorners( p, q );
    }
    if( mode == "compute" )
    {
        keys( v, { "mode", "points" } );
        std::vector<VECTOR2<T>> points;
        for( auto& p : v.at( "points" ) )
            points.push_back( point<T>( p ) );
        if( !points.empty() )
        {
            auto lo = points[0], hi = lo;
            for( auto& p : points )
            {
                lo.x = std::min( lo.x, p.x );
                lo.y = std::min( lo.y, p.y );
                hi.x = std::max( hi.x, p.x );
                hi.y = std::max( hi.y, p.y );
            }
            sub( hi.x, lo.x );
            sub( hi.y, lo.y );
        }
        b.Compute( points );
        return b;
    }
    keys( v, { "mode", "pos", "size" } );
    auto p = point<T>( v.at( "pos" ) );
    auto s = point<E>( v.at( "size" ) );
    if( mode == "raw" )
    {
        b.SetOrigin( p );
        b.SetSize( s );
        return b;
    }
    if( mode == "center" )
    {
        auto       x = sub( E( p.x ), s.x / 2 );
        auto       y = sub( E( p.y ), s.y / 2 );
        VECTOR2<T> pos( VECTOR2<E>( x, y ) );
        add( E( pos.x ), s.x );
        add( E( pos.y ), s.y );
        b.SetOrigin( pos );
        b.SetSize( s );
        normalizeDomain( b );
        return B::ByCenter( p, s );
    }
    if( mode != "new" )
        throw std::invalid_argument( "unknown box mode" );
    add( E( p.x ), s.x );
    add( E( p.y ), s.y );
    b.SetOrigin( p );
    b.SetSize( s );
    normalizeDomain( b );
    return B( p, s );
}
template <class T>
void inflateDomain( const BOX2<VECTOR2<T>>& a, T dx, T dy )
{
    using E = typename VECTOR2<T>::extended_type;
    auto axis = []( T p, E s, T d )
    {
        E delta = d;
        E two = 2;
        if( s >= 0 )
        {
            if( s < mul( E( -2 ), delta ) )
                add( E( p ), s / two );
            else
            {
                auto lo = std::clamp( sub( E( p ), delta ), E( std::numeric_limits<T>::lowest() ),
                                      E( std::numeric_limits<T>::max() ) );
                auto hi = std::clamp( add( add( E( p ), s ), delta ), E( std::numeric_limits<T>::lowest() ),
                                      E( std::numeric_limits<T>::max() ) );
                sub( hi, lo );
            }
        }
        else if( s > mul( two, delta ) )
            sub( E( p ), s / two );
        else
        {
            auto hi = std::clamp( add( E( p ), delta ), E( std::numeric_limits<T>::lowest() ),
                                  E( std::numeric_limits<T>::max() ) );
            auto lo = std::clamp( sub( add( E( p ), s ), delta ), E( std::numeric_limits<T>::lowest() ),
                                  E( std::numeric_limits<T>::max() ) );
            sub( lo, hi );
        }
    };
    axis( a.GetX(), a.GetWidth(), dx );
    axis( a.GetY(), a.GetHeight(), dy );
}
template <class T>
void containsDomain( const BOX2<VECTOR2<T>>& b, VECTOR2<T> p )
{
    using E = typename VECTOR2<T>::extended_type;
    auto r = VECTOR2<E>( sub( E( p.x ), E( b.GetX() ) ), sub( E( p.y ), E( b.GetY() ) ) );
    auto s = b.GetSize();
    if( s.x < 0 )
    {
        s.x = sub( E( 0 ), s.x );
        add( r.x, s.x );
    }
    if( s.y < 0 )
    {
        s.y = sub( E( 0 ), s.y );
        add( r.y, s.y );
    }
}
template <class T>
void distanceDomain( const BOX2<VECTOR2<T>>& a, VECTOR2<T> p )
{
    using E = typename VECTOR2<T>::extended_type;
    endsDomain( a );
    E x = a.GetX(), y = a.GetY();
    E x1 = std::min( x, add( x, a.GetWidth() ) ), x2 = std::max( x, add( x, a.GetWidth() ) );
    E y1 = std::min( y, add( y, a.GetHeight() ) ), y2 = std::max( y, add( y, a.GetHeight() ) );
    E dx = std::max( p.x < x1 ? sub( x1, E( p.x ) ) : sub( E( p.x ), x2 ), E( 0 ) );
    E dy = std::max( p.y < y1 ? sub( y1, E( p.y ) ) : sub( E( p.y ), y2 ), E( 0 ) );
    add( mul( dx, dx ), mul( dy, dy ) );
}
template <class T>
JSON boxes( const JSON& v )
{
    using B = BOX2<VECTOR2<T>>;
    using E = typename B::ecoord_type;
    B                        a = boxRead<T>( v.at( "a" ) );
    auto                     k = text( v, "kind" );
    std::vector<std::string> fields = { "op", "type", "kind", "a" };
    auto                     extra = [&]( std::initializer_list<const char*> fs )
    {
        for( auto f : fs )
            fields.emplace_back( f );
    };
    if( k == "merge" || k == "contains_box" || k == "intersects" || k == "intersect" || k == "distance_box"
        || k == "squared_distance_box" || k == "compare" )
        extra( { "b" } );
    else if( k == "rotated" )
        extra( { "b", "angle" } );
    else if( k == "segment" )
        extra( { "p", "q" } );
    else if( k == "circle" )
        extra( { "p", "radius" } );
    else if( k == "circle_edge" )
        extra( { "p", "radius", "width" } );
    else if( k == "inflate" || k == "inflated" || k == "set_size" )
        extra( { "s" } );
    else if( k == "inflate_uniform" || k == "inflated_uniform" || k == "set_x" || k == "set_y" || k == "set_width"
             || k == "set_height" )
        extra( { "n" } );
    else if( k == "rotated_bbox" )
        extra( { "p", "angle" } );
    else if( k == "compute" )
        extra( { "points" } );
    else if( k == "contains" || k == "contains_xy" || k == "merge_point" || k == "nearest" || k == "farthest"
             || k == "distance" || k == "squared_distance" || k == "offset" || k == "offset_xy" || k == "move"
             || k == "with_offset" || k == "set_end" || k == "set_end_xy" || k == "set_origin" || k == "set_origin_xy" )
        extra( { "p" } );
    keys( v, fields );
    if( k == "state" )
        return state( a );
    if( k == "maximum" )
    {
        a.SetMaximum();
        return state( a );
    }
    if( k == "normalize" )
    {
        normalizeDomain( a );
        return mutState( a, a.Normalize() );
    }
    if( k == "getters" )
    {
        endsDomain( a );
        add( E( a.GetX() ), a.GetWidth() / 2 );
        add( E( a.GetY() ), a.GetHeight() / 2 );
        castInt( a.GetWidth() > a.GetHeight() ? a.GetWidth() : a.GetHeight() );
        return JSON{ { "origin", vec( a.GetOrigin() ) }, { "position", vec( a.GetPosition() ) },
                     { "end", vec( a.GetEnd() ) },       { "center", vec( a.GetCenter() ) },
                     { "centre", vec( a.Centre() ) },    { "left", out( a.GetLeft() ) },
                     { "top", out( a.GetTop() ) },       { "right", out( a.GetRight() ) },
                     { "bottom", out( a.GetBottom() ) }, { "x", out( a.GetX() ) },
                     { "y", out( a.GetY() ) },           { "width", out( a.GetWidth() ) },
                     { "height", out( a.GetHeight() ) }, { "size_max", a.GetSizeMax() } };
    }
    if( k == "contains" || k == "contains_xy" )
    {
        auto p = point<T>( v.at( "p" ) );
        containsDomain( a, p );
        return k == "contains" ? a.Contains( p ) : a.Contains( p.x, p.y );
    }
    if( k == "contains_box" )
    {
        auto b = boxRead<T>( v.at( "b" ) );
        containsDomain( a, b.GetOrigin() );
        if( !a.Contains( b.GetOrigin() ) )
            return a.Contains( b );
        endsDomain( b );
        containsDomain( a, b.GetEnd() );
        return a.Contains( b );
    }
    if( k == "intersects" || k == "intersect" || k == "merge" || k == "compare" )
    {
        auto b = boxRead<T>( v.at( "b" ) );
        if( k == "merge" && !a.IsValid() )
        {
            return mutState( a, a.Merge( b ) );
        }
        normalizeDomain( a );
        normalizeDomain( b );
        auto na = a, nb = b;
        na.Normalize();
        nb.Normalize();
        if( k == "compare" )
            return JSON{ a == b, a != b };
        endsDomain( na );
        endsDomain( nb );
        if( k == "intersects" )
            return a.Intersects( b );
        if( k == "intersect" )
            return state( a.Intersect( b ) );
        return mutState( a, a.Merge( b ) );
    }
    if( k == "merge_point" )
    {
        auto p = point<T>( v.at( "p" ) );
        if( a.IsValid() )
        {
            normalizeDomain( a );
            auto n = a;
            n.Normalize();
            endsDomain( n );
        }
        return mutState( a, a.Merge( p ) );
    }
    if( k == "segment" )
    {
        auto p = point<T>( v.at( "p" ) );
        auto q = point<T>( v.at( "q" ) );
        containsDomain( a, p );
        if( a.Contains( p ) )
            return a.Intersects( p, q );
        containsDomain( a, q );
        if( a.Contains( q ) )
            return a.Intersects( p, q );
        endsDomain( a );
        return a.Intersects( p, q );
    }
    if( k == "rotated" )
        return a.Intersects( boxRead<T>( v.at( "b" ) ), EDA_ANGLE( read<double>( v.at( "angle" ) ), DEGREES_T ) );
    if( k == "rotated_bbox" )
        return state( a.GetBoundingBoxRotated( point<int>( v.at( "p" ) ),
                                               EDA_ANGLE( read<double>( v.at( "angle" ) ), DEGREES_T ) ) );
    if( k == "nearest" || k == "farthest" || k == "circle" || k == "circle_edge" )
    {
        auto p = point<T>( v.at( "p" ) );
        if( ( k == "circle" || k == "circle_edge" ) && !a.IsValid() )
            return false;
        normalizeDomain( a );
        auto n = a;
        n.Normalize();
        endsDomain( n );
        if( k != "farthest" && ( n.GetRight() < n.GetLeft() || n.GetBottom() < n.GetTop() ) )
            undefined();
        if( k == "nearest" )
            return vec( a.NearestPoint( p ) );
        if( k == "farthest" )
            return vec( a.FarthestPointTo( p ) );
        int r = read<int>( v.at( "radius" ) );
        if( k == "circle" )
            return a.IntersectsCircle( p, r );
        int w = read<int>( v.at( "width" ) );
        add( add( r, w / 2 ), 1 );
        return a.IntersectsCircleEdge( p, r, w );
    }
    if( k == "distance" || k == "squared_distance" )
    {
        auto p = point<T>( v.at( "p" ) );
        distanceDomain( a, p );
        return k == "distance" ? out( a.Distance( p ) ) : out( a.SquaredDistance( p ) );
    }
    if( k == "distance_box" || k == "squared_distance_box" )
    {
        auto b = boxRead<T>( v.at( "b" ) );
        E    s = 0;
        auto axis = [&]( T p, E z, T bp, E bz )
        {
            if( add( E( bp ), bz ) < p )
            {
                E d = sub( add( E( bp ), bz ), E( p ) );
                s = add( s, mul( d, d ) );
            }
            else if( E( bp ) > add( E( p ), z ) )
            {
                E d = sub( sub( E( bp ), z ), E( p ) );
                s = add( s, mul( d, d ) );
            }
        };
        axis( a.GetX(), a.GetWidth(), b.GetX(), b.GetWidth() );
        axis( a.GetY(), a.GetHeight(), b.GetY(), b.GetHeight() );
        return k == "distance_box" ? out( a.Distance( b ) ) : out( a.SquaredDistance( b ) );
    }
    if( k == "area" )
    {
        mul( a.GetWidth(), a.GetHeight() );
        return out( a.GetArea() );
    }
    if( k == "diagonal" )
        return out( a.Diagonal() );
    if( k == "squared_diagonal" )
        return out( a.SquaredDiagonal() );
    if( k == "safe" )
    {
        endsDomain( a );
        return IsBOX2Safe( a );
    }
    if( k == "format" )
        return a.Format();
    if( k == "inflate" || k == "inflated" )
    {
        auto p = point<T>( v.at( "s" ) );
        inflateDomain( a, p.x, p.y );
        if( k == "inflated" )
            return state( a.GetInflated( p.x, p.y ) );
        return mutState( a, a.Inflate( p.x, p.y ) );
    }
    if( k == "inflate_uniform" || k == "inflated_uniform" )
    {
        auto d = read<T>( v.at( "n" ) );
        inflateDomain( a, d, d );
        if( k == "inflated_uniform" )
            return state( a.GetInflated( d ) );
        return mutState( a, a.Inflate( d ) );
    }
    if( k == "offset" || k == "offset_xy" || k == "move" || k == "with_offset" )
    {
        auto p = point<T>( v.at( "p" ) );
        add( a.GetX(), p.x );
        add( a.GetY(), p.y );
        if( k == "offset" )
            a.Offset( p );
        else if( k == "offset_xy" )
            a.Offset( p.x, p.y );
        else if( k == "move" )
            a.Move( p );
        else
            return state( a.GetWithOffset( p ) );
        return state( a );
    }
    if( k == "set_origin" || k == "set_origin_xy" )
    {
        auto p = point<T>( v.at( "p" ) );
        if( k == "set_origin" )
            a.SetOrigin( p );
        else
            a.SetOrigin( p.x, p.y );
        return state( a );
    }
    if( k == "set_end" || k == "set_end_xy" )
    {
        auto p = point<T>( v.at( "p" ) );
        sub( E( p.x ), E( a.GetX() ) );
        sub( E( p.y ), E( a.GetY() ) );
        if( k == "set_end" )
            a.SetEnd( p );
        else
            a.SetEnd( p.x, p.y );
        return state( a );
    }
    if( k == "set_size" )
    {
        a.SetSize( point<E>( v.at( "s" ) ) );
        return state( a );
    }
    if( k == "set_x" )
    {
        a.SetX( read<T>( v.at( "n" ) ) );
        return state( a );
    }
    if( k == "set_y" )
    {
        a.SetY( read<T>( v.at( "n" ) ) );
        return state( a );
    }
    if( k == "set_width" )
    {
        a.SetWidth( read<E>( v.at( "n" ) ) );
        return state( a );
    }
    if( k == "set_height" )
    {
        a.SetHeight( read<E>( v.at( "n" ) ) );
        return state( a );
    }
    if( k == "compute" )
    {
        std::vector<VECTOR2<T>> ps;
        for( const auto& p : v.at( "points" ) )
            ps.push_back( point<T>( p ) );
        if( !ps.empty() )
        {
            auto lo = ps[0], hi = lo;
            for( auto p : ps )
            {
                lo.x = std::min( lo.x, p.x );
                lo.y = std::min( lo.y, p.y );
                hi.x = std::max( hi.x, p.x );
                hi.y = std::max( hi.y, p.y );
            }
            sub( hi.x, lo.x );
            sub( hi.y, lo.y );
        }
        a.Compute( ps );
        return state( a );
    }
    throw std::invalid_argument( "unknown box operation" );
}
template <class S>
JSON rounded( const JSON& v )
{
    S x = read<S>( v.at( "a" ) );
    if( !v.at( "quiet" ).is_boolean() )
        throw std::invalid_argument( "expected bool" );
    bool q = v.at( "quiet" ).get<bool>();
    auto t = text( v, "target" );
#define TARGET( name, type )                                                                                           \
    if( t == name )                                                                                                    \
        return out( KiROUND<S, type>( x, q ) );
    TARGET( "i8", int8_t )
    TARGET( "u8", uint8_t )
    TARGET( "i16", int16_t )
    TARGET( "u16", uint16_t )
    TARGET( "i32", int )
    TARGET( "u32", unsigned )
    TARGET( "i64", int64_t )
    TARGET( "ll", long long )
#undef TARGET
    throw std::invalid_argument( "unknown target" );
}
// Validate only the execution domain; the reply still comes from the pinned function.
template <class T>
void checkCtDomain( T x, T lo, T hi )
{
    using P = decltype( T( 0 ) + T( 0 ) );
    for( int depth = 0; depth < 128; ++depth )
    {
        if( lo == hi )
            return;
        P sum;
        if constexpr( std::is_unsigned_v<P> )
            sum = P( P( lo ) + P( hi ) + P( 1 ) );
        else
            sum = add( add( P( lo ), P( hi ) ), P( 1 ) );
        T mid = T( sum / P( 2 ) );
        if( mid == 0 )
            undefined();
        if constexpr( std::is_signed_v<P> )
            if( P( x ) == std::numeric_limits<P>::lowest() && P( mid ) == -1 )
                undefined();
        if( P( x ) / P( mid ) < P( mid ) )
        {
            if constexpr( std::is_unsigned_v<P> )
                hi = T( P( mid ) - P( 1 ) );
            else
                hi = T( sub( P( mid ), P( 1 ) ) );
        }
        else
            lo = mid;
    }
    undefined();
}
template <class T>
JSON scalar( const JSON& v )
{
    auto op = text( v, "op" );
    T    a = read<T>( v.at( "a" ) );
    if( op == "round" )
        return rounded<T>( v );
    if( op == "sign" )
        return sign( a );
    if( op == "clamp" )
    {
        T lo = read<T>( v.at( "lo" ) ), hi = read<T>( v.at( "hi" ) );
        if( hi < lo )
            undefined();
        return out( std::clamp( a, lo, hi ) );
    }
    if constexpr( std::is_floating_point_v<T> )
    {
        if( op == "equals" )
        {
            auto b = read<T>( v.at( "b" ) );
            return v.at( "epsilon" ).is_null() ? equals( a, b ) : equals( a, b, read<T>( v.at( "epsilon" ) ) );
        }
    }
    else
    {
        if( op == "isqrt" )
            return out( isqrt( a ) );
        if( op == "ct_sqrt" )
        {
            checkCtDomain( a, T( 0 ), T( a / 2 + 1 ) );
            return out( ct_sqrt( a ) );
        }
        if( op == "ct_sqrt_helper" )
        {
            auto lo = read<T>( v.at( "lo" ) ), hi = read<T>( v.at( "hi" ) );
            checkCtDomain( a, lo, hi );
            return out( ct_sqrt_helper( a, lo, hi ) );
        }
    }
    throw std::invalid_argument( "unknown operation" );
}
JSON run( const JSON& v )
{
    auto op = text( v, "op" );
    if( op == "box" )
    {
        auto t = text( v, "type" );
        if( t == "i32" )
            return boxes<int>( v );
        if( t == "i64" )
            return boxes<int64_t>( v );
        if( t == "f64" )
            return boxes<double>( v );
        throw std::invalid_argument( "unknown box type" );
    }
    if( op == "minmax" )
    {
        keys( v, { "op", "kind", "a", "b", "p" } );
        const auto& arr = v.at( "a" );
        if( !arr.is_array() || arr.size() != 4 )
            throw std::invalid_argument( "expected edges" );
        BOX2I_MINMAX a( read<int>( arr[0] ), read<int>( arr[1] ), read<int>( arr[2] ), read<int>( arr[3] ) );
        auto         k = text( v, "kind" );
        auto         p = point<int>( v.at( "p" ) );
        if( k == "default" )
            a = BOX2I_MINMAX();
        else if( k == "center" )
            return vec( a.GetCenter() );
        else if( k == "diameter" )
            return out( a.GetDiameter() );
        else if( k == "box" )
            return state( BOX2I( a ) );
        else if( k == "merge" )
            a.Merge( p );
        else if( k == "point" )
            a = BOX2I_MINMAX( p );
        else if( k == "xy" )
            a = BOX2I_MINMAX( p.x, p.y );
        else if( k == "corners" )
            a = BOX2I_MINMAX( p, point<int>( v.at( "b" ) ) );
        else if( k == "segment" )
            a = BOX2I_MINMAX( SEG( p, point<int>( v.at( "b" ) ) ) );
        else if( k == "from_box" )
            a = BOX2I_MINMAX( boxRead<int>( v.at( "b" ) ) );
        else if( k == "intersects" )
            return a.Intersects( BOX2I_MINMAX( boxRead<int>( v.at( "b" ) ) ) );
        else if( k == "arc" )
            a = BOX2I_MINMAX( SHAPE_ARC( p, point<int>( v.at( "b" ) ), VECTOR2I( 0, 0 ), 0 ) );
        else
            throw std::invalid_argument( "unknown minmax operation" );
        return JSON{ a.m_Left, a.m_Top, a.m_Right, a.m_Bottom };
    }
    if( op == "safe_parts" )
    {
        keys( v, { "op", "type", "pos", "size" } );
        auto t = text( v, "type" );
        if( t == "f64" )
        {
            auto p = point<double>( v.at( "pos" ) ), s = point<double>( v.at( "size" ) );
            if( std::isnan( p.x ) || std::isnan( p.y ) || std::isnan( p.x + s.x ) || std::isnan( p.y + s.y ) )
                undefined();
            return state( BOX2ISafe( p, s ) );
        }
        auto p = point<int>( v.at( "pos" ) );
        if( t == "i32" )
            return state( BOX2ISafe( p, point<int>( v.at( "size" ) ) ) );
        if( t == "i64" )
        {
            auto s = point<int64_t>( v.at( "size" ) );
            add( int64_t( p.x ), s.x );
            add( int64_t( p.y ), s.y );
            return state( BOX2ISafe( p, s ) );
        }
        throw std::invalid_argument( "unknown type" );
    }
    if( op == "safe_box" || op == "round_box" )
    {
        keys( v, { "op", "a" } );
        auto b = boxRead<double>( v.at( "a" ) );
        if( op == "safe_box" )
        {
            if( std::isnan( b.GetLeft() ) || std::isnan( b.GetTop() ) || std::isnan( b.GetRight() )
                || std::isnan( b.GetBottom() ) )
                undefined();
            return state( BOX2ISafe( b ) );
        }
        return state( KiROUND( b ) );
    }
    if( op == "round_default" )
    {
        keys( v, { "op", "type", "a" } );
        auto t = text( v, "type" );
        if( t == "f64" )
            return out( KiROUND( read<double>( v.at( "a" ) ) ) );
        if( t == "f32" )
            return out( KiROUND( read<float>( v.at( "a" ) ) ) );
        if( t == "i32" )
            return out( KiROUND( read<int>( v.at( "a" ) ) ) );
        if( t == "i64" )
            return out( KiROUND( read<int64_t>( v.at( "a" ) ) ) );
        if( t == "ll" )
            return out( KiROUND( read<long long>( v.at( "a" ) ) ) );
        throw std::invalid_argument( "unknown type" );
    }
    std::vector<std::string> fields = { "op", "type", "a" };
    if( op == "round" )
    {
        fields.push_back( "quiet" );
        fields.push_back( "target" );
    }
    else if( op == "equals" )
    {
        fields.push_back( "b" );
        fields.push_back( "epsilon" );
    }
    else if( op == "rescale" )
    {
        fields.push_back( "b" );
        fields.push_back( "d" );
    }
    else if( op == "clamp" || op == "ct_sqrt_helper" )
    {
        fields.push_back( "lo" );
        fields.push_back( "hi" );
    }
    else if( op == "hypot" )
        fields.push_back( "b" );
    keys( v, fields );
    if( op == "checked_cast" )
        return out( KiCheckedCast<long long, int>( read<int64_t>( v.at( "a" ) ) ) );
    if( op == "abs" )
        return out( UnsignedAbs( read<int64_t>( v.at( "a" ) ) ) );
    if( op == "hypot" )
        return out( RoundedHypot( read<int64_t>( v.at( "a" ) ), read<int64_t>( v.at( "b" ) ) ) );
    auto t = text( v, "type" );
    if( op == "rescale" )
    {
#define SCALE( name, T )                                                                                               \
    if( t == name )                                                                                                    \
    {                                                                                                                  \
        T a = read<T>( v.at( "a" ) ), b = read<T>( v.at( "b" ) ), d = read<T>( v.at( "d" ) );                          \
        if constexpr( std::is_integral_v<T> )                                                                          \
        {                                                                                                              \
            if( d == 0 )                                                                                               \
                undefined();                                                                                           \
        }                                                                                                              \
        return out( rescale( a, b, d ) );                                                                              \
    }
        SCALE( "i32", int )
        SCALE( "i64", int64_t )
        SCALE( "f64", double )
        SCALE( "f32", float )
#undef SCALE
        throw std::invalid_argument( "unknown type" );
    }
#define TYPE( name, T )                                                                                                \
    if( t == name )                                                                                                    \
        return scalar<T>( v );
    TYPE( "f64", double )
    TYPE( "f32", float )
    TYPE( "i8", int8_t )
    TYPE( "u8", uint8_t )
    TYPE( "i16", int16_t )
    TYPE( "u16", uint16_t )
    TYPE( "i32", int )
    TYPE( "u32", unsigned )
    TYPE( "i64", int64_t )
    TYPE( "ll", long long ) TYPE( "u64", uint64_t )
#undef TYPE
            throw std::invalid_argument( "unknown type" );
}
} // namespace
namespace oracle
{
void RegisterUtilBox( REGISTRY& registry )
{
    registry.Add( UTIL_BOX_UNIT, "math/{util,box2,box2_minmax}.h", run );
}
} // namespace oracle
