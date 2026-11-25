// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001:
// the pinned, unmodified clipper.core.h and clipper.engine.h, compiled with KiCad's public
// USINGZ definition and linked against the pinned clipper2 archive. This file validates
// requests, refuses inputs on which the reference expression would have undefined behaviour
// (the guards repeat the reference's integer and conversion steps in its evaluation order but
// never produce a reply), calls the real templates and classes, and serialises the results.
// Doubles travel as their 16 hex digit bit patterns; pointers never reach the output.
#include "registry.h"
#include <clipper2/clipper.core.h>
#include <clipper2/clipper.engine.h>
#include <algorithm>
#include <bit>
#include <cmath>
#include <cstdint>
#include <functional>
#include <initializer_list>
#include <iomanip>
#include <limits>
#include <locale>
#include <memory>
#include <optional>
#include <sstream>
#include <string>
#include <type_traits>
#include <vector>

#ifndef USINGZ
#error "KiCad compiles Clipper2 with the public USINGZ definition"
#endif
#if defined( CLIPPER2_HI_PRECISION ) || defined( CLIPPER2_MAX_DECIMAL_PRECISION )
#error "KiCad does not define the Clipper2 precision overrides"
#endif
#if !( ( defined( __cpp_exceptions ) && __cpp_exceptions ) || ( defined( __EXCEPTIONS ) && __EXCEPTIONS ) )
#error "KiCad compiles Clipper2 with exceptions"
#endif
#if !( ( defined( __clang__ ) || defined( __GNUC__ ) ) && UINTPTR_MAX >= UINT64_MAX )
#error "the oracle covers the __int128_t branches"
#endif
static_assert( sizeof( int64_t ) == 8 && sizeof( size_t ) == 8, "64-bit oracle contract" );

namespace
{
using oracle::JSON;
namespace c2 = Clipper2Lib;

constexpr const char* CLIPPER2_UNIT = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p001";
constexpr int64_t     STREAM_MIN = -64;
constexpr int64_t     STREAM_MAX = 512;
constexpr size_t      MAX_TREE_OPS = 1024;

[[noreturn]] void Invalid( const std::string& aWhat )
{
    throw std::invalid_argument( "invalid input: " + aWhat );
}


// Exact key set (including "op").
void Keys( const JSON& aValue, std::initializer_list<const char*> aFields )
{
    if( !aValue.is_object() || aValue.size() != aFields.size() )
        Invalid( "fields" );

    for( const char* field : aFields )
    {
        if( !aValue.contains( field ) )
            Invalid( "fields" );
    }
}


const JSON& At( const JSON& aValue, const char* aName )
{
    if( !aValue.is_object() || !aValue.contains( aName ) )
        Invalid( aName );

    return aValue.at( aName );
}


std::string Text( const JSON& aValue, const char* aName )
{
    const JSON& value = At( aValue, aName );

    if( !value.is_string() )
        Invalid( aName );

    return value.get<std::string>();
}


bool Bool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        Invalid( "boolean" );

    return aValue.get<bool>();
}


int64_t I64( const JSON& aValue )
{
    if( !aValue.is_number_integer()
        || ( aValue.is_number_unsigned()
             && aValue.get<uint64_t>() > static_cast<uint64_t>( INT64_MAX ) ) )
    {
        Invalid( "int64" );
    }

    return aValue.get<int64_t>();
}


uint64_t U64( const JSON& aValue )
{
    if( !aValue.is_number_integer() || ( !aValue.is_number_unsigned() && aValue.get<int64_t>() < 0 ) )
        Invalid( "uint64" );

    return aValue.get<uint64_t>();
}


int I32( const JSON& aValue )
{
    const int64_t value = I64( aValue );

    if( value < INT32_MIN || value > INT32_MAX )
        Invalid( "int" );

    return static_cast<int>( value );
}


double F64( const JSON& aValue )
{
    if( !aValue.is_string() )
        Invalid( "double" );

    const std::string& text = aValue.get_ref<const std::string&>();

    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        Invalid( "double" );

    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON Bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


template <typename T>
T Value( const JSON& aValue )
{
    if constexpr( std::is_integral_v<T> )
        return I64( aValue );
    else
        return F64( aValue );
}


template <typename T>
JSON Out( T aValue )
{
    if constexpr( std::is_integral_v<T> )
        return aValue;
    else
        return Bits( aValue );
}


template <typename T>
c2::Point<T> ReadPoint( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        Invalid( "point" );

    c2::Point<T> point;
    point.x = Value<T>( aValue[0] );
    point.y = Value<T>( aValue[1] );
    point.z = I64( aValue[2] );
    return point;
}


template <typename T>
JSON WritePoint( const c2::Point<T>& aPoint )
{
    return JSON::array( { Out( aPoint.x ), Out( aPoint.y ), aPoint.z } );
}


template <typename T>
c2::Path<T> ReadPath( const JSON& aValue )
{
    if( !aValue.is_array() )
        Invalid( "path" );

    c2::Path<T> path;

    for( const JSON& point : aValue )
        path.push_back( ReadPoint<T>( point ) );

    return path;
}


template <typename T>
JSON WritePath( const c2::Path<T>& aPath )
{
    JSON result = JSON::array();

    for( const auto& point : aPath )
        result.push_back( WritePoint( point ) );

    return result;
}


template <typename T>
c2::Paths<T> ReadPaths( const JSON& aValue )
{
    if( !aValue.is_array() )
        Invalid( "paths" );

    c2::Paths<T> paths;

    for( const JSON& path : aValue )
        paths.push_back( ReadPath<T>( path ) );

    return paths;
}


template <typename T>
JSON WritePaths( const c2::Paths<T>& aPaths )
{
    JSON result = JSON::array();

    for( const auto& path : aPaths )
        result.push_back( WritePath( path ) );

    return result;
}


template <typename T>
c2::Rect<T> ReadRect( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 4 )
        Invalid( "rect" );

    return c2::Rect<T>( Value<T>( aValue[0] ), Value<T>( aValue[1] ), Value<T>( aValue[2] ),
                        Value<T>( aValue[3] ) );
}


template <typename T>
JSON WriteRect( const c2::Rect<T>& aRect )
{
    return JSON::array( { Out( aRect.left ), Out( aRect.top ), Out( aRect.right ), Out( aRect.bottom ) } );
}


JSON Thrown( const std::exception& aError )
{
    return JSON{ { "thrown", aError.what() } };
}


const char* PipName( c2::PointInPolygonResult aResult )
{
    switch( aResult )
    {
    case c2::PointInPolygonResult::IsOn: return "IsOn";
    case c2::PointInPolygonResult::IsInside: return "IsInside";
    case c2::PointInPolygonResult::IsOutside: return "IsOutside";
    }

    throw std::logic_error( "point in polygon result" );
}


// --- undefined-behaviour guards ------------------------------------------------------------
// Each guard throws the shared refusal when the reference expression would be undefined.

struct GUARD
{
    std::string op;

    [[noreturn]] void Undefined() const
    {
        throw std::invalid_argument( "undefined behaviour: " + op );
    }

    template <typename T>
    T Add( T a, T b ) const
    {
        if constexpr( std::is_integral_v<T> )
        {
            T r;

            if( __builtin_add_overflow( a, b, &r ) )
                Undefined();

            return r;
        }
        else
        {
            return a + b;
        }
    }

    template <typename T>
    T Sub( T a, T b ) const
    {
        if constexpr( std::is_integral_v<T> )
        {
            T r;

            if( __builtin_sub_overflow( a, b, &r ) )
                Undefined();

            return r;
        }
        else
        {
            return a - b;
        }
    }

    template <typename T>
    T Mul( T a, T b ) const
    {
        if constexpr( std::is_integral_v<T> )
        {
            T r;

            if( __builtin_mul_overflow( a, b, &r ) )
                Undefined();

            return r;
        }
        else
        {
            return a * b;
        }
    }

    template <typename T>
    T Neg( T a ) const
    {
        if constexpr( std::is_integral_v<T> )
        {
            if( a == std::numeric_limits<T>::min() )
                Undefined();
        }

        return -a;
    }

    // static_cast<int64_t>( double ): the truncated value must be representable.
    int64_t ToI64( double v ) const
    {
        if( !( v >= -0x1p63 && v < 0x1p63 ) )
            Undefined();

        return static_cast<int64_t>( v );
    }

    // static_cast<__int128_t>( double ).
    __int128_t ToI128( double v ) const
    {
        if( !( v >= -0x1p127 && v < 0x1p127 ) )
            Undefined();

        return static_cast<__int128_t>( v );
    }

    // static_cast<T>( double ) for a T target.
    template <typename T>
    void Cast( double v ) const
    {
        if constexpr( std::is_integral_v<T> )
            (void) ToI64( v );
    }

    // Point<T>::Init conversion of one T2 coordinate.
    template <typename T, typename T2>
    void Init( T2 v ) const
    {
        if constexpr( std::is_integral_v<T> && !std::is_integral_v<T2> )
            (void) ToI64( std::round( v ) );
    }

    template <typename T, typename T2>
    void InitPoint( const c2::Point<T2>& p ) const
    {
        Init<T>( p.x );
        Init<T>( p.y );
    }

    template <typename T>
    int CrossProductSign( const c2::Point<T>& pt1, const c2::Point<T>& pt2, const c2::Point<T>& pt3 ) const
    {
        const T a = Sub( pt2.x, pt1.x );
        const T b = Sub( pt3.y, pt2.y );
        const T c = Sub( pt2.y, pt1.y );
        const T d = Sub( pt3.x, pt2.x );
        auto wide = [&]( T v ) -> __int128_t
        {
            if constexpr( std::is_integral_v<T> )
                return v;
            else
                return ToI128( v );
        };
        __int128_t ab, cd;

        if( __builtin_mul_overflow( wide( a ), wide( b ), &ab )
            || __builtin_mul_overflow( wide( c ), wide( d ), &cd ) )
        {
            Undefined();
        }

        return ab > cd ? 1 : ( ab < cd ? -1 : 0 );
    }

    template <typename T>
    void Area( const c2::Path<T>& path ) const
    {
        const size_t cnt = path.size();

        if( cnt < 3 )
            return;

        auto term = [&]( const c2::Point<T>& p, const c2::Point<T>& q )
        {
            (void) Add( p.y, q.y );
            (void) Sub( p.x, q.x );
        };
        size_t it2 = cnt - 1, stop = it2, it1 = 0;

        if( !( cnt & 1 ) )
            ++stop;

        for( ; it1 != stop; it1 += 2 )
        {
            term( path[it2], path[it1] );
            it2 = it1 + 1;
            term( path[it1], path[it2] );
        }

        if( cnt & 1 )
            term( path[it2], path[it1] );
    }

    template <typename T>
    bool NearEqual( const c2::Point<T>& p1, const c2::Point<T>& p2, double max ) const
    {
        return c2::Sqr( Sub( p1.x, p2.x ) ) + c2::Sqr( Sub( p1.y, p2.y ) ) < max;
    }

    // The StripNearEqual loop, evaluating its NearEqual calls in order.
    template <typename T>
    void StripNearEqual( const c2::Path<T>& path, double max, bool closed ) const
    {
        if( path.empty() )
            return;

        c2::Path<T> result{ path.front() };
        c2::Point<T> first = path.front(), last = first;

        for( size_t i = 1; i < path.size(); ++i )
        {
            if( !NearEqual( path[i], last, max ) )
            {
                last = path[i];
                result.push_back( last );
            }
        }

        if( !closed )
            return;

        while( result.size() > 1 && NearEqual( result.back(), first, max ) )
            result.pop_back();
    }

    // The GetBounds<T, T2> loop with its mixed comparisons and conversions.
    template <typename T, typename T2>
    void Bounds( const c2::Path<T2>& path, T& xmin, T& ymin, T& xmax, T& ymax ) const
    {
        for( const auto& p : path )
        {
            // Only a floating source converts (by truncation) into an integral bound.
            auto take = [&]( T2 v, T& bound )
            {
                if constexpr( std::is_integral_v<T> && !std::is_integral_v<T2> )
                    (void) ToI64( v );

                bound = static_cast<T>( v );
            };

            if( p.x < xmin ) take( p.x, xmin );
            if( p.x > xmax ) take( p.x, xmax );
            if( p.y < ymin ) take( p.y, ymin );
            if( p.y > ymax ) take( p.y, ymax );
        }
    }

    // ScalePath conversions (after its zero-scale check passed).
    template <typename T1, typename T2>
    void ScalePath( const c2::Path<T2>& path, double sx, double sy ) const
    {
        for( const auto& pt : path )
        {
            Init<T1>( pt.x * sx );
            Init<T1>( pt.y * sy );
        }
    }

    // ScalePaths: range rejection, then the first zero-scale ScalePath throws.
    template <typename T1, typename T2>
    void ScalePaths( const c2::Paths<T2>& paths, double sx, double sy ) const
    {
        if constexpr( std::is_integral_v<T1> )
        {
            c2::RectD r = c2::GetBounds<double, T2>( paths );

            if( ( r.left * sx ) < c2::min_coord || ( r.right * sx ) > c2::max_coord
                || ( r.top * sy ) < c2::min_coord || ( r.bottom * sy ) > c2::max_coord )
            {
                return;
            }
        }

        if( sx == 0 || sy == 0 )
            return;

        for( const auto& path : paths )
            ScalePath<T1>( path, sx, sy );
    }

    // The PointInPolygon walk, guarding each CrossProductSign it reaches.
    template <typename T>
    void PointInPolygon( const c2::Point<T>& pt, const c2::Path<T>& polygon ) const
    {
        if( polygon.size() < 3 )
            return;

        int    val = 0;
        size_t n = polygon.size(), cbegin = 0, first = 0, cend = n, curr, prev;

        while( first != cend && polygon[first].y == pt.y )
            ++first;

        if( first == cend )
            return;

        bool is_above = polygon[first].y < pt.y, starting_above = is_above;
        curr = first + 1;

        while( true )
        {
            if( curr == cend )
            {
                if( cend == first || first == cbegin )
                    break;

                cend = first;
                curr = cbegin;
            }

            if( is_above )
            {
                while( curr != cend && polygon[curr].y < pt.y )
                    ++curr;

                if( curr == cend )
                    continue;
            }
            else
            {
                while( curr != cend && polygon[curr].y > pt.y )
                    ++curr;

                if( curr == cend )
                    continue;
            }

            prev = curr == cbegin ? n - 1 : curr - 1;

            if( polygon[curr].y == pt.y )
            {
                if( polygon[curr].x == pt.x
                    || ( polygon[curr].y == polygon[prev].y
                         && ( ( pt.x < polygon[prev].x ) != ( pt.x < polygon[curr].x ) ) ) )
                {
                    return;
                }

                ++curr;

                if( curr == first )
                    break;

                continue;
            }

            if( pt.x < polygon[curr].x && pt.x < polygon[prev].x )
            {
            }
            else if( pt.x > polygon[prev].x && pt.x > polygon[curr].x )
            {
                val = 1 - val;
            }
            else
            {
                int d = CrossProductSign( polygon[prev], polygon[curr], pt );

                if( d == 0 )
                    return;

                if( ( d < 0 ) == is_above )
                    val = 1 - val;
            }

            is_above = !is_above;
            ++curr;
        }

        if( is_above != starting_above )
        {
            cend = n;

            if( curr == cend )
                curr = cbegin;

            prev = curr == cbegin ? cend - 1 : curr - 1;
            (void) CrossProductSign( polygon[prev], polygon[curr], pt );
        }
    }

    // StripeIndex::BuildFromPath, Build, YToStripe and PointInPolygon (clipper.engine.cpp):
    // the pinned y range, stripe height and stripe arithmetic, the decline test, and every
    // CrossProductSign the query reaches, in the pinned order. Nothing is returned.
    void StripeIndex( const c2::Path64& path, const c2::Point64* query ) const
    {
        std::vector<std::pair<c2::Point64, c2::Point64>> edges;

        if( path.size() >= 3 )
        {
            for( size_t i = 0, j = path.size() - 1; i < path.size(); j = i++ )
            {
                if( path[j].x != path[i].x || path[j].y != path[i].y )
                    edges.emplace_back( path[j], path[i] );
            }
        }

        // Build's empty branch sets no range; PointInPolygon then returns IsOutside.
        if( edges.empty() )
            return;

        int64_t yMin = edges[0].first.y, yMax = edges[0].first.y;

        for( const auto& [a, b] : edges )
        {
            yMin = std::min( yMin, std::min( a.y, b.y ) );
            yMax = std::max( yMax, std::max( a.y, b.y ) );
        }

        int count = static_cast<int>( std::sqrt( static_cast<double>( edges.size() ) ) );

        if( count < 1 )
            count = 1;

        if( count > 65536 )
            count = 65536;

        // y_range = y_max - y_min_; stripe_height_ = (y_range + stripe_count_ - 1) / stripe_count_.
        const int64_t range = Sub( yMax, yMin );
        const int64_t height = range <= 0 ? 1 : Sub( Add( range, int64_t( count ) ), int64_t( 1 ) ) / count;
        auto          stripe = [&]( int64_t y ) -> int64_t
        {
            const int64_t s = Sub( y, yMin ) / height;
            return s < 0 ? 0 : ( s >= count ? count - 1 : s );
        };
        int64_t total = 0;

        for( const auto& [a, b] : edges )
        {
            total = Add( total, Add( Sub( stripe( std::max( a.y, b.y ) ), stripe( std::min( a.y, b.y ) ) ),
                                     int64_t( 1 ) ) );
        }

        // A declined ring keeps no edges: PointInPolygon returns IsOutside without arithmetic.
        if( total > Mul( static_cast<int64_t>( edges.size() ), int64_t( 8 ) ) || !query )
            return;

        const int64_t s = stripe( query->y );

        // The stripe lists its edges in ascending index order.
        for( const auto& [a, b] : edges )
        {
            if( s < stripe( std::min( a.y, b.y ) ) || s > stripe( std::max( a.y, b.y ) ) )
                continue;

            const int cross = CrossProductSign( a, b, *query );

            if( cross == 0 && query->x >= std::min( a.x, b.x ) && query->x <= std::max( a.x, b.x )
                && query->y >= std::min( a.y, b.y ) && query->y <= std::max( a.y, b.y ) )
            {
                return;
            }
        }
    }

    // Clipper64 runs the pinned engine (clipper.engine.cpp, owned by the engine source units)
    // directly on caller coordinates: its int64_t edge differences (GetDx, TopX, ...) are
    // undefined for coordinates whose differences overflow. The adapter executes it only within
    // Clipper2's declared coordinate range, MIN_COORD..MAX_COORD, which is the range ClipperD's
    // ScalePaths range check admits. This is a harness domain limit, not a claim that every
    // coordinate outside it is undefined; the engine units own the precise guards.
    void EngineDomain( const c2::Paths64& paths ) const
    {
        for( const auto& path : paths )
        {
            for( const auto& pt : path )
            {
                if( pt.x < c2::MIN_COORD || pt.x > c2::MAX_COORD || pt.y < c2::MIN_COORD
                    || pt.y > c2::MAX_COORD )
                {
                    throw std::invalid_argument( "outside the guarded engine domain: " + op );
                }
            }
        }
    }
};


// --- stream state ----------------------------------------------------------------------------

void ApplyState( std::ostringstream& aStream, const JSON& aState )
{
    static const std::vector<std::pair<const char*, std::ios_base::fmtflags>> names = {
        { "boolalpha", std::ios_base::boolalpha }, { "dec", std::ios_base::dec },
        { "fixed", std::ios_base::fixed },         { "hex", std::ios_base::hex },
        { "internal", std::ios_base::internal },   { "left", std::ios_base::left },
        { "oct", std::ios_base::oct },             { "right", std::ios_base::right },
        { "scientific", std::ios_base::scientific }, { "showbase", std::ios_base::showbase },
        { "showpoint", std::ios_base::showpoint }, { "showpos", std::ios_base::showpos },
        { "skipws", std::ios_base::skipws },       { "unitbuf", std::ios_base::unitbuf },
        { "uppercase", std::ios_base::uppercase },
    };
    Keys( aState, { "flags", "precision", "width", "fill" } );
    const JSON& list = aState.at( "flags" );

    if( !list.is_array() )
        Invalid( "state" );

    std::ios_base::fmtflags  flags{};
    std::vector<std::string> seen;

    for( const JSON& item : list )
    {
        if( !item.is_string() )
            Invalid( "state" );

        const std::string& name = item.get_ref<const std::string&>();
        const auto found = std::find_if( names.begin(), names.end(),
                                         [&]( const auto& aEntry ) { return name == aEntry.first; } );

        if( found == names.end() || std::find( seen.begin(), seen.end(), name ) != seen.end() )
            Invalid( "state" );

        seen.push_back( name );
        flags |= found->second;
    }

    auto bounded = [&]( const char* aName )
    {
        const int64_t value = I64( aState.at( aName ) );

        if( value < STREAM_MIN || value > STREAM_MAX )
            Invalid( "state" );

        return value;
    };
    const int64_t precision = bounded( "precision" );
    const int64_t width = bounded( "width" );
    const JSON&   fill = aState.at( "fill" );

    if( !fill.is_string() || fill.get_ref<const std::string&>().size() != 1
        || fill.get_ref<const std::string&>()[0] < 0x20 || fill.get_ref<const std::string&>()[0] > 0x7e )
    {
        Invalid( "state" );
    }

    aStream.flags( flags );
    aStream.precision( static_cast<std::streamsize>( precision ) );
    aStream.width( static_cast<std::streamsize>( width ) );
    aStream.fill( fill.get_ref<const std::string&>()[0] );
}


// --- typed core operations -----------------------------------------------------------------

template <typename T>
JSON RunTyped( const std::string& aOp, const JSON& aIn, const GUARD& g )
{
    using P = c2::Point<T>;

    if( aOp == "point_default" )
    {
        Keys( aIn, { "op", "type" } );
        return WritePoint( P() );
    }

    if( aOp == "point_scale" )
    {
        Keys( aIn, { "op", "type", "point", "scale" } );
        const P      p = ReadPoint<T>( aIn.at( "point" ) );
        const double scale = F64( aIn.at( "scale" ) );
        g.Init<T>( p.x * scale );
        g.Init<T>( p.y * scale );
        return WritePoint( p * scale );
    }

    if( aOp == "point_unary" )
    {
        Keys( aIn, { "op", "type", "point", "kind", "z" } );
        P                 p = ReadPoint<T>( aIn.at( "point" ) );
        const std::string kind = Text( aIn, "kind" );
        const int64_t     z = I64( aIn.at( "z" ) );

        if( kind == "negated" )
        {
            g.Neg( p.x );
            g.Neg( p.y );
            return WritePoint( -p );
        }

        if( kind == "negate" )
        {
            g.Neg( p.x );
            g.Neg( p.y );
            p.Negate();
            return WritePoint( p );
        }

        if( kind == "set_z" )
        {
            p.SetZ( z );
            return WritePoint( p );
        }

        Invalid( "kind" );
    }

    if( aOp == "point_binary" )
    {
        Keys( aIn, { "op", "type", "a", "b", "kind" } );
        const P           a = ReadPoint<T>( aIn.at( "a" ) );
        const P           b = ReadPoint<T>( aIn.at( "b" ) );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "plus" )
        {
            g.Add( a.x, b.x );
            g.Add( a.y, b.y );
            return WritePoint( a + b );
        }

        if( kind == "minus" )
        {
            g.Sub( a.x, b.x );
            g.Sub( a.y, b.y );
            return WritePoint( a - b );
        }

        if( kind == "eq" )
            return a == b;

        if( kind == "ne" )
            return a != b;

        Invalid( "kind" );
    }

    if( aOp == "midpoint" )
    {
        Keys( aIn, { "op", "type", "a", "b" } );
        const P a = ReadPoint<T>( aIn.at( "a" ) );
        const P b = ReadPoint<T>( aIn.at( "b" ) );
        g.Add( a.x, b.x );
        g.Add( a.y, b.y );
        return WritePoint( c2::MidPoint( a, b ) );
    }

    if( aOp == "rect_construct" )
    {
        Keys( aIn, { "op", "type", "kind", "rect" } );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "values" )
            return WriteRect( ReadRect<T>( aIn.at( "rect" ) ) );

        if( !aIn.at( "rect" ).is_null() )
            Invalid( "rect" );

        if( kind == "default" )
            return WriteRect( c2::Rect<T>() );

        if( kind == "valid" )
            return WriteRect( c2::Rect<T>( true ) );

        if( kind == "invalid" )
            return WriteRect( c2::Rect<T>( false ) );

        if( kind == "invalid_rect" )
            return WriteRect( c2::Rect<T>::InvalidRect() );

        if( kind == "invalid_constant" )
        {
            if constexpr( std::is_integral_v<T> )
                return WriteRect( c2::InvalidRect64 );
            else
                return WriteRect( c2::InvalidRectD );
        }

        Invalid( "kind" );
    }

    if( aOp == "rect_query" )
    {
        Keys( aIn, { "op", "type", "rect", "kind" } );
        const c2::Rect<T> r = ReadRect<T>( aIn.at( "rect" ) );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "is_valid" )
            return r.IsValid();

        if( kind == "is_empty" )
            return r.IsEmpty();

        if( kind == "width" )
        {
            g.Sub( r.right, r.left );
            return Out( r.Width() );
        }

        if( kind == "height" )
        {
            g.Sub( r.bottom, r.top );
            return Out( r.Height() );
        }

        if( kind == "mid_point" )
        {
            g.Add( r.left, r.right );
            g.Add( r.top, r.bottom );
            return WritePoint( r.MidPoint() );
        }

        if( kind == "as_path" )
            return WritePath( r.AsPath() );

        Invalid( "kind" );
    }

    if( aOp == "rect_set" )
    {
        Keys( aIn, { "op", "type", "rect", "kind", "value" } );
        c2::Rect<T>       r = ReadRect<T>( aIn.at( "rect" ) );
        const std::string kind = Text( aIn, "kind" );
        const T           value = Value<T>( aIn.at( "value" ) );

        if( kind == "width" )
        {
            g.Add( r.left, value );
            r.Width( value );
        }
        else if( kind == "height" )
        {
            g.Add( r.top, value );
            r.Height( value );
        }
        else
        {
            Invalid( "kind" );
        }

        return WriteRect( r );
    }

    if( aOp == "rect_contains" )
    {
        Keys( aIn, { "op", "type", "rect", "kind", "other" } );
        const c2::Rect<T> r = ReadRect<T>( aIn.at( "rect" ) );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "point" )
            return r.Contains( ReadPoint<T>( aIn.at( "other" ) ) );

        if( kind == "rect" )
            return r.Contains( ReadRect<T>( aIn.at( "other" ) ) );

        Invalid( "kind" );
    }

    if( aOp == "rect_binary" )
    {
        Keys( aIn, { "op", "type", "a", "b", "kind" } );
        c2::Rect<T>       a = ReadRect<T>( aIn.at( "a" ) );
        const c2::Rect<T> b = ReadRect<T>( aIn.at( "b" ) );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "intersects" )
            return a.Intersects( b );

        if( kind == "eq" )
            return a == b;

        if( kind == "plus" )
            return WriteRect( a + b );

        if( kind == "add_assign" )
        {
            c2::Rect<T>& self = ( a += b );
            return JSON{ { "rect", WriteRect( a ) }, { "returns_self", &self == &a } };
        }

        Invalid( "kind" );
    }

    if( aOp == "rect_scale" )
    {
        Keys( aIn, { "op", "type", "rect", "scale" } );
        c2::Rect<T>  r = ReadRect<T>( aIn.at( "rect" ) );
        const double scale = F64( aIn.at( "scale" ) );

        for( T v : { r.left, r.top, r.right, r.bottom } )
            g.Cast<T>( v * scale );

        r.Scale( scale );
        return WriteRect( r );
    }

    if( aOp == "sqr" )
    {
        Keys( aIn, { "op", "type", "value" } );
        return Bits( c2::Sqr( Value<T>( aIn.at( "value" ) ) ) );
    }

    if( aOp == "near_equal" )
    {
        Keys( aIn, { "op", "type", "a", "b", "max" } );
        const P      a = ReadPoint<T>( aIn.at( "a" ) );
        const P      b = ReadPoint<T>( aIn.at( "b" ) );
        const double max = F64( aIn.at( "max" ) );
        g.NearEqual( a, b, max );
        return c2::NearEqual( a, b, max );
    }

    if( aOp == "strip_near_equal" )
    {
        Keys( aIn, { "op", "type", "shape", "value", "max", "closed" } );
        const std::string shape = Text( aIn, "shape" );
        const double      max = F64( aIn.at( "max" ) );
        const bool        closed = Bool( aIn.at( "closed" ) );

        if( shape == "path" )
        {
            const c2::Path<T> path = ReadPath<T>( aIn.at( "value" ) );
            g.StripNearEqual( path, max, closed );
            return WritePath( c2::StripNearEqual( path, max, closed ) );
        }

        if( shape == "paths" )
        {
            const c2::Paths<T> paths = ReadPaths<T>( aIn.at( "value" ) );

            for( const auto& path : paths )
                g.StripNearEqual( path, max, closed );

            return WritePaths( c2::StripNearEqual( paths, max, closed ) );
        }

        Invalid( "shape" );
    }

    if( aOp == "strip_duplicates" )
    {
        Keys( aIn, { "op", "type", "shape", "value", "closed" } );
        const std::string shape = Text( aIn, "shape" );
        const bool        closed = Bool( aIn.at( "closed" ) );

        if( shape == "path" )
        {
            c2::Path<T> path = ReadPath<T>( aIn.at( "value" ) );
            c2::StripDuplicates( path, closed );
            return WritePath( path );
        }

        if( shape == "paths" )
        {
            c2::Paths<T> paths = ReadPaths<T>( aIn.at( "value" ) );
            c2::StripDuplicates( paths, closed );
            return WritePaths( paths );
        }

        Invalid( "shape" );
    }

    if( aOp == "cross_product_sign" || aOp == "is_collinear" || aOp == "cross3" || aOp == "dot3"
        || aOp == "perpendic_dist" )
    {
        Keys( aIn, { "op", "type", "points" } );
        const c2::Path<T> pts = ReadPath<T>( aIn.at( "points" ) );

        if( pts.size() != 3 )
            Invalid( "points" );

        if( aOp == "cross_product_sign" )
        {
            g.CrossProductSign( pts[0], pts[1], pts[2] );
            return c2::CrossProductSign( pts[0], pts[1], pts[2] );
        }

        if( aOp == "is_collinear" )
        {
            const T a = g.Sub( pts[1].x, pts[0].x );
            const T b = g.Sub( pts[2].y, pts[1].y );
            const T c = g.Sub( pts[1].y, pts[0].y );
            const T d = g.Sub( pts[2].x, pts[1].x );

            // Only double differences convert to ProductsAreEqual's int64_t parameters.
            if constexpr( !std::is_integral_v<T> )
            {
                for( T v : { a, b, c, d } )
                    (void) g.ToI64( v );
            }

            return c2::IsCollinear( pts[0], pts[1], pts[2] );
        }

        if( aOp == "cross3" )
        {
            g.Sub( pts[1].x, pts[0].x );
            g.Sub( pts[2].y, pts[1].y );
            g.Sub( pts[1].y, pts[0].y );
            g.Sub( pts[2].x, pts[1].x );
            return Bits( c2::CrossProduct( pts[0], pts[1], pts[2] ) );
        }

        if( aOp == "dot3" )
        {
            g.Sub( pts[1].x, pts[0].x );
            g.Sub( pts[2].x, pts[1].x );
            g.Sub( pts[1].y, pts[0].y );
            g.Sub( pts[2].y, pts[1].y );
            return Bits( c2::DotProduct( pts[0], pts[1], pts[2] ) );
        }

        g.Sub( pts[0].x, pts[1].x );
        g.Sub( pts[0].y, pts[1].y );
        g.Sub( pts[2].x, pts[1].x );
        g.Sub( pts[2].y, pts[1].y );
        return Bits( c2::PerpendicDistFromLineSqrd( pts[0], pts[1], pts[2] ) );
    }

    if( aOp == "cross2" || aOp == "dot2" || aOp == "distance_sqr" )
    {
        Keys( aIn, { "op", "type", "points" } );
        const c2::Path<T> pts = ReadPath<T>( aIn.at( "points" ) );

        if( pts.size() != 2 )
            Invalid( "points" );

        if( aOp == "cross2" )
        {
            g.Mul( pts[0].y, pts[1].x );
            g.Mul( pts[1].y, pts[0].x );
            return Bits( c2::CrossProduct( pts[0], pts[1] ) );
        }

        if( aOp == "dot2" )
        {
            g.Mul( pts[0].x, pts[1].x );
            g.Mul( pts[0].y, pts[1].y );
            return Bits( c2::DotProduct( pts[0], pts[1] ) );
        }

        g.Sub( pts[0].x, pts[1].x );
        g.Sub( pts[0].y, pts[1].y );
        return Bits( c2::DistanceSqr( pts[0], pts[1] ) );
    }

    if( aOp == "area" || aOp == "is_positive" )
    {
        Keys( aIn, { "op", "type", "shape", "value" } );
        const std::string shape = Text( aIn, "shape" );

        if( shape == "path" )
        {
            const c2::Path<T> path = ReadPath<T>( aIn.at( "value" ) );
            g.Area( path );

            if( aOp == "is_positive" )
                return c2::IsPositive( path );

            return Bits( c2::Area( path ) );
        }

        if( shape == "paths" && aOp == "area" )
        {
            const c2::Paths<T> paths = ReadPaths<T>( aIn.at( "value" ) );

            for( const auto& path : paths )
                g.Area( path );

            return Bits( c2::Area( paths ) );
        }

        Invalid( "shape" );
    }

    if( aOp == "line_intersect" )
    {
        Keys( aIn, { "op", "type", "points", "ip" } );
        const c2::Path<T> pts = ReadPath<T>( aIn.at( "points" ) );

        if( pts.size() != 4 )
            Invalid( "points" );

        P ip = ReadPoint<T>( aIn.at( "ip" ) );
        const double dx1 = static_cast<double>( g.Sub( pts[1].x, pts[0].x ) );
        const double dy1 = static_cast<double>( g.Sub( pts[1].y, pts[0].y ) );
        const double dx2 = static_cast<double>( g.Sub( pts[3].x, pts[2].x ) );
        const double dy2 = static_cast<double>( g.Sub( pts[3].y, pts[2].y ) );
        const double det = dy1 * dx2 - dy2 * dx1;

        if( det != 0.0 )
        {
            const double t = ( static_cast<double>( g.Sub( pts[0].x, pts[2].x ) ) * dy2
                               - static_cast<double>( g.Sub( pts[0].y, pts[2].y ) ) * dx2 )
                             / det;

            if( !( t <= 0.0 ) && !( t >= 1.0 ) )
            {
                g.Cast<T>( pts[0].x + t * dx1 );
                g.Cast<T>( pts[0].y + t * dy1 );
            }
        }

        const bool result = c2::GetLineIntersectPt( pts[0], pts[1], pts[2], pts[3], ip );
        return JSON{ { "result", result }, { "ip", WritePoint( ip ) } };
    }

    if( aOp == "translate_point" )
    {
        Keys( aIn, { "op", "type", "point", "dx", "dy" } );
        const P      p = ReadPoint<T>( aIn.at( "point" ) );
        const double dx = F64( aIn.at( "dx" ) );
        const double dy = F64( aIn.at( "dy" ) );
        g.Init<T>( p.x + dx );
        g.Init<T>( p.y + dy );
        return WritePoint( c2::TranslatePoint( p, dx, dy ) );
    }

    if( aOp == "reflect_point" )
    {
        Keys( aIn, { "op", "type", "point", "pivot" } );
        const P p = ReadPoint<T>( aIn.at( "point" ) );
        const P pivot = ReadPoint<T>( aIn.at( "pivot" ) );
        g.Add( pivot.x, g.Sub( pivot.x, p.x ) );
        g.Add( pivot.y, g.Sub( pivot.y, p.y ) );
        return WritePoint( c2::ReflectPoint( p, pivot ) );
    }

    if( aOp == "get_sign" )
    {
        Keys( aIn, { "op", "type", "value" } );
        return c2::GetSign( Value<T>( aIn.at( "value" ) ) );
    }

    if( aOp == "closest_point" )
    {
        Keys( aIn, { "op", "type", "points" } );
        const c2::Path<T> pts = ReadPath<T>( aIn.at( "points" ) );

        if( pts.size() != 3 )
            Invalid( "points" );

        const P &off = pts[0], &s1 = pts[1], &s2 = pts[2];

        if( !( s1.x == s2.x && s1.y == s2.y ) )
        {
            const double dx = static_cast<double>( g.Sub( s2.x, s1.x ) );
            const double dy = static_cast<double>( g.Sub( s2.y, s1.y ) );
            double       q = ( static_cast<double>( g.Sub( off.x, s1.x ) ) * dx
                         + static_cast<double>( g.Sub( off.y, s1.y ) ) * dy )
                       / ( c2::Sqr( dx ) + c2::Sqr( dy ) );

            if( q < 0 )
                q = 0;
            else if( q > 1 )
                q = 1;

            if constexpr( std::is_integral_v<T> )
            {
                g.Add( s1.x, g.ToI64( nearbyint( q * dx ) ) );
                g.Add( s1.y, g.ToI64( nearbyint( q * dy ) ) );
            }
        }

        return WritePoint( c2::GetClosestPointOnSegment( off, s1, s2 ) );
    }

    if( aOp == "point_in_polygon" )
    {
        Keys( aIn, { "op", "type", "point", "polygon" } );
        const P           p = ReadPoint<T>( aIn.at( "point" ) );
        const c2::Path<T> polygon = ReadPath<T>( aIn.at( "polygon" ) );
        g.PointInPolygon( p, polygon );
        return PipName( c2::PointInPolygon( p, polygon ) );
    }

    if( aOp == "stream" )
    {
        Keys( aIn, { "op", "type", "shape", "value", "state" } );
        const std::string  shape = Text( aIn, "shape" );
        std::ostringstream stream;

        if( !aIn.at( "state" ).is_null() )
            ApplyState( stream, aIn.at( "state" ) );

        if( shape == "point" )
            stream << ReadPoint<T>( aIn.at( "value" ) );
        else if( shape == "rect" )
            stream << ReadRect<T>( aIn.at( "value" ) );
        else if( shape == "path" )
            stream << ReadPath<T>( aIn.at( "value" ) );
        else if( shape == "paths" )
            stream << ReadPaths<T>( aIn.at( "value" ) );
        else
            Invalid( "shape" );

        return JSON{ { "text", stream.str() }, { "width_after", static_cast<int64_t>( stream.width() ) } };
    }

    Invalid( "op" );
}


// --- conversions between coordinate types ------------------------------------------------------

template <typename T1, typename T2>
JSON RunMixed( const std::string& aOp, const JSON& aIn, const GUARD& g )
{
    if( aOp == "point_construct" )
    {
        Keys( aIn, { "op", "to", "from", "x", "y", "z" } );
        const T2 x = Value<T2>( aIn.at( "x" ) );
        const T2 y = Value<T2>( aIn.at( "y" ) );
        g.Init<T1>( x );
        g.Init<T1>( y );

        if( aIn.at( "z" ).is_null() )
            return WritePoint( c2::Point<T1>( x, y ) );

        return WritePoint( c2::Point<T1>( x, y, I64( aIn.at( "z" ) ) ) );
    }

    if( aOp == "point_convert" )
    {
        Keys( aIn, { "op", "to", "from", "point", "z" } );
        const c2::Point<T2> p = ReadPoint<T2>( aIn.at( "point" ) );
        g.InitPoint<T1>( p );

        if( aIn.at( "z" ).is_null() )
            return WritePoint( c2::Point<T1>( p ) );

        return WritePoint( c2::Point<T1>( p, I64( aIn.at( "z" ) ) ) );
    }

    if( aOp == "point_init" )
    {
        Keys( aIn, { "op", "to", "from", "point", "x", "y", "z" } );
        c2::Point<T1> p = ReadPoint<T1>( aIn.at( "point" ) );
        const T2      x = Value<T2>( aIn.at( "x" ) );
        const T2      y = Value<T2>( aIn.at( "y" ) );
        g.Init<T1>( x );
        g.Init<T1>( y );

        if( aIn.at( "z" ).is_null() )
            p.Init( x, y );
        else
            p.Init( x, y, I64( aIn.at( "z" ) ) );

        return WritePoint( p );
    }

    if( aOp == "append_point" )
    {
        Keys( aIn, { "op", "to", "from", "path", "point" } );
        c2::Path<T1>        path = ReadPath<T1>( aIn.at( "path" ) );
        const c2::Point<T2> p = ReadPoint<T2>( aIn.at( "point" ) );
        g.InitPoint<T1>( p );
        c2::Path<T1>& self = ( path << p );
        return JSON{ { "path", WritePath( path ) }, { "returns_self", &self == &path } };
    }

    if( aOp == "scale_rect" )
    {
        Keys( aIn, { "op", "to", "from", "rect", "scale" } );
        const c2::Rect<T2> r = ReadRect<T2>( aIn.at( "rect" ) );
        const double       scale = F64( aIn.at( "scale" ) );

        for( T2 v : { r.left, r.top, r.right, r.bottom } )
        {
            if constexpr( std::is_integral_v<T1> && !std::is_integral_v<T2> )
                g.Cast<T1>( std::round( v * scale ) );
            else
                g.Cast<T1>( v * scale );
        }

        return WriteRect( c2::ScaleRect<T1, T2>( r, scale ) );
    }

    if( aOp == "get_bounds" )
    {
        Keys( aIn, { "op", "to", "from", "shape", "value" } );
        const std::string shape = Text( aIn, "shape" );
        T1 xmin = ( std::numeric_limits<T1>::max )(), ymin = xmin;
        T1 xmax = std::numeric_limits<T1>::lowest(), ymax = xmax;

        if( shape == "path" )
        {
            const c2::Path<T2> path = ReadPath<T2>( aIn.at( "value" ) );
            g.Bounds<T1>( path, xmin, ymin, xmax, ymax );

            if constexpr( std::is_same_v<T1, T2> )
                return WriteRect( c2::GetBounds( path ) );
            else
                return WriteRect( c2::GetBounds<T1, T2>( path ) );
        }

        if( shape == "paths" )
        {
            const c2::Paths<T2> paths = ReadPaths<T2>( aIn.at( "value" ) );

            for( const auto& path : paths )
                g.Bounds<T1>( path, xmin, ymin, xmax, ymax );

            if constexpr( std::is_same_v<T1, T2> )
                return WriteRect( c2::GetBounds( paths ) );
            else
                return WriteRect( c2::GetBounds<T1, T2>( paths ) );
        }

        Invalid( "shape" );
    }

    if( aOp == "scale_path" || aOp == "scale_paths" )
    {
        Keys( aIn, { "op", "to", "from", "value", "scale_x", "scale_y", "error_code" } );
        const double sx = F64( aIn.at( "scale_x" ) );
        const bool   uniform = aIn.at( "scale_y" ).is_null();
        const double sy = uniform ? sx : F64( aIn.at( "scale_y" ) );
        int          error_code = I32( aIn.at( "error_code" ) );

        try
        {
            if( aOp == "scale_path" )
            {
                const c2::Path<T2> path = ReadPath<T2>( aIn.at( "value" ) );

                if( sx != 0 && sy != 0 )
                    g.ScalePath<T1>( path, sx, sy );

                const c2::Path<T1> result = uniform ? c2::ScalePath<T1, T2>( path, sx, error_code )
                                                    : c2::ScalePath<T1, T2>( path, sx, sy, error_code );
                return JSON{ { "value", WritePath( result ) }, { "error_code", error_code } };
            }

            const c2::Paths<T2> paths = ReadPaths<T2>( aIn.at( "value" ) );
            g.ScalePaths<T1>( paths, sx, sy );
            const c2::Paths<T1> result = uniform ? c2::ScalePaths<T1, T2>( paths, sx, error_code )
                                                 : c2::ScalePaths<T1, T2>( paths, sx, sy, error_code );
            return JSON{ { "value", WritePaths( result ) }, { "error_code", error_code } };
        }
        catch( const c2::Clipper2Exception& error )
        {
            return JSON{ { "thrown", error.what() }, { "error_code", error_code } };
        }
    }

    if( aOp == "transform_path" )
    {
        Keys( aIn, { "op", "to", "from", "value" } );
        const c2::Path<T2> path = ReadPath<T2>( aIn.at( "value" ) );

        for( const auto& p : path )
            g.InitPoint<T1>( p );

        return WritePath( c2::TransformPath<T1, T2>( path ) );
    }

    if( aOp == "transform_paths" )
    {
        Keys( aIn, { "op", "to", "from", "value" } );
        const c2::Paths<T2> paths = ReadPaths<T2>( aIn.at( "value" ) );

        for( const auto& path : paths )
        {
            for( const auto& p : path )
                g.InitPoint<T1>( p );
        }

        return WritePaths( c2::TransformPaths<T1, T2>( paths ) );
    }

    Invalid( "op" );
}


template <typename T1>
JSON DispatchFrom( const std::string& aOp, const JSON& aIn, const GUARD& g )
{
    const std::string from = Text( aIn, "from" );

    if( from == "i64" )
        return RunMixed<T1, int64_t>( aOp, aIn, g );

    if( from == "f64" )
        return RunMixed<T1, double>( aOp, aIn, g );

    Invalid( "from" );
}


// --- engine data structures --------------------------------------------------------------------

JSON VertexJSON( const c2::Vertex& v )
{
    return JSON{ { "pt", WritePoint( v.pt ) }, { "next_null", v.next == nullptr },
                 { "prev_null", v.prev == nullptr }, { "flags", static_cast<uint32_t>( v.flags ) } };
}


const char* JoinName( c2::JoinWith aJoin )
{
    switch( aJoin )
    {
    case c2::JoinWith::NoJoin: return "NoJoin";
    case c2::JoinWith::Left: return "Left";
    case c2::JoinWith::Right: return "Right";
    }

    throw std::logic_error( "join" );
}


JSON EngineDefaults()
{
    c2::Vertex v;
    c2::OutRec rec;
    c2::Active e;
    c2::IntersectNode node;
    c2::HorzSegment seg;
    c2::HorzJoin join;
    c2::StripeIndex index;

    JSON outrec{ { "idx", rec.idx }, { "owner_null", rec.owner == nullptr },
                 { "front_edge_null", rec.front_edge == nullptr },
                 { "back_edge_null", rec.back_edge == nullptr }, { "pts_null", rec.pts == nullptr },
                 { "polypath_null", rec.polypath == nullptr }, { "splits_null", rec.splits == nullptr },
                 { "recursive_split_null", rec.recursive_split == nullptr },
                 { "bounds", WriteRect( rec.bounds ) }, { "path", WritePath( rec.path ) },
                 { "is_open", rec.is_open }, { "pip_index_null", rec.pip_index == nullptr } };
    JSON active{ { "bot", WritePoint( e.bot ) }, { "top", WritePoint( e.top ) }, { "curr_x", e.curr_x },
                 { "dx", Bits( e.dx ) }, { "wind_dx", e.wind_dx }, { "wind_cnt", e.wind_cnt },
                 { "wind_cnt2", e.wind_cnt2 }, { "outrec_null", e.outrec == nullptr },
                 { "links_null", e.prev_in_ael == nullptr && e.next_in_ael == nullptr && e.prev_in_sel == nullptr
                                         && e.next_in_sel == nullptr && e.jump == nullptr },
                 { "vertex_top_null", e.vertex_top == nullptr }, { "local_min_null", e.local_min == nullptr },
                 { "is_left_bound", e.is_left_bound }, { "join_with", JoinName( e.join_with ) } };
    return JSON{ { "vertex", VertexJSON( v ) },
                 { "outrec", outrec },
                 { "active", active },
                 { "intersect_node", { { "pt", WritePoint( node.pt ) }, { "edge1_null", node.edge1 == nullptr },
                                       { "edge2_null", node.edge2 == nullptr } } },
                 { "horz_segment", { { "left_op_null", seg.left_op == nullptr }, { "right_op_null", seg.right_op == nullptr },
                                     { "left_to_right", seg.left_to_right } } },
                 { "horz_join", { { "op1_null", join.op1 == nullptr }, { "op2_null", join.op2 == nullptr } } },
                 { "stripe_index", { { "edge_count", index.EdgeCount() }, { "empty", index.Empty() } } } };
}


c2::ClipType ReadClipType( const JSON& aValue )
{
    static const std::vector<std::pair<const char*, c2::ClipType>> names = {
        { "NoClip", c2::ClipType::NoClip }, { "Intersection", c2::ClipType::Intersection },
        { "Union", c2::ClipType::Union }, { "Difference", c2::ClipType::Difference }, { "Xor", c2::ClipType::Xor } };

    for( const auto& [name, value] : names )
    {
        if( aValue.is_string() && aValue.get<std::string>() == name )
            return value;
    }

    Invalid( "clip_type" );
}


c2::FillRule ReadFillRule( const JSON& aValue )
{
    static const std::vector<std::pair<const char*, c2::FillRule>> names = {
        { "EvenOdd", c2::FillRule::EvenOdd }, { "NonZero", c2::FillRule::NonZero },
        { "Positive", c2::FillRule::Positive }, { "Negative", c2::FillRule::Negative } };

    for( const auto& [name, value] : names )
    {
        if( aValue.is_string() && aValue.get<std::string>() == name )
            return value;
    }

    Invalid( "fill_rule" );
}


// Read access to the protected base callback, as a derived class may.
struct PROBE64 : c2::Clipper64
{
    bool HasZ() const { return static_cast<bool>( zCallback_ ); }
};


struct PROBED : c2::ClipperD
{
    explicit PROBED( int aPrecision ) : c2::ClipperD( aPrecision ) {}
    PROBED() : c2::ClipperD() {}
    bool HasZ() const { return static_cast<bool>( zCallback_ ); }
    void CallBase( const c2::Point64& a, const c2::Point64& b, const c2::Point64& c, const c2::Point64& d,
                   c2::Point64& pt )
    {
        zCallback_( a, b, c, d, pt );
    }
};


// A recording ZCallbackD: stores the descaled arguments, writes z and a decoy x.
struct RECORDER
{
    JSON    calls = JSON::array();
    int64_t z = 0;

    c2::ZCallbackD Callback()
    {
        return [this]( const c2::PointD& a, const c2::PointD& b, const c2::PointD& c, const c2::PointD& d,
                       c2::PointD& pt )
        {
            calls.push_back( JSON::array( { WritePoint( a ), WritePoint( b ), WritePoint( c ), WritePoint( d ),
                                            WritePoint( pt ) } ) );
            pt.z = z;
            pt.x = -pt.x;
        };
    }
};


std::unique_ptr<PROBED> MakeD( const JSON& aPrecision )
{
    if( aPrecision.is_null() )
        return std::make_unique<PROBED>();

    return std::make_unique<PROBED>( I32( aPrecision ) );
}


// ClipperD's private scale_, recovered exactly on a separate clipper of the same precision as
// the reciprocal of the descale its ZCB reports (the measured clipper keeps its own callback).
double ScaleOf( const JSON& aPrecision )
{
    std::unique_ptr<PROBED> probe = MakeD( aPrecision );
    RECORDER                recorder;
    probe->SetZCallback( recorder.Callback() );
    c2::Point64 unit( 1, 1 );
    probe->ZCB( unit, unit, unit, unit, unit );
    return 1 / F64( recorder.calls[0][0][0] );
}


void ReadPoints5( const JSON& aValue, c2::Point64 ( &aOut )[5] )
{
    const c2::Path64 pts = ReadPath<int64_t>( aValue );

    if( pts.size() != 5 )
        Invalid( "points" );

    std::copy( pts.begin(), pts.end(), aOut );
}


JSON Tree64( const c2::PolyPath64& aNode )
{
    JSON children = JSON::array();

    for( const auto& child : aNode )
        children.push_back( Tree64( *child ) );

    return JSON{ { "polygon", WritePath( aNode.Polygon() ) }, { "children", children } };
}


JSON TreeD( const c2::PolyPathD& aNode )
{
    JSON children = JSON::array();

    for( const auto& child : aNode )
        children.push_back( TreeD( *child ) );

    return JSON{ { "polygon", WritePath( aNode.Polygon() ) }, { "scale", Bits( aNode.Scale() ) },
                 { "children", children } };
}


// Run a sequence of PolyPath operations; nodes are numbered by creation (root 0).
template <typename TREE>
JSON RunTree( const JSON& aIn, const GUARD& g )
{
    constexpr bool D = std::is_same_v<TREE, c2::PolyPathD>;
    using T = std::conditional_t<D, double, int64_t>;
    const JSON&    ops = At( aIn, "ops" );

    if( !ops.is_array() || ops.size() > MAX_TREE_OPS )
        Invalid( "ops" );

    std::unique_ptr<TREE> root;
    const JSON&           rootPath = At( aIn, "root" );
    const std::string     rootType = Text( aIn, "root_type" );

    if( rootPath.is_null() )
        root = std::make_unique<TREE>( nullptr );
    else if( rootType == "i64" )
        root = std::make_unique<TREE>( nullptr, ReadPath<int64_t>( rootPath ) );
    else if( D && rootType == "f64" )
    {
        if constexpr( D )
            root = std::make_unique<TREE>( nullptr, ReadPath<double>( rootPath ) );
    }
    else
        Invalid( "root_type" );

    std::vector<TREE*> nodes{ root.get() };
    std::vector<int>   parents{ -1 };
    std::vector<bool>  live{ true };
    JSON               events = JSON::array();

    // Freed nodes' addresses can be reused by later children: resolve among live nodes only.
    auto indexOf = [&]( const c2::PolyPath* aNode ) -> JSON
    {
        for( size_t i = 0; i < nodes.size(); ++i )
        {
            if( live[i] && nodes[i] == aNode )
                return i;
        }

        throw std::logic_error( "unknown PolyPath" );
    };

    auto node = [&]( const JSON& aIndex ) -> TREE*
    {
        const uint64_t index = U64( aIndex );

        if( index >= nodes.size() )
            Invalid( "node" );

        if( !live[index] )
            g.Undefined();

        return nodes[index];
    };

    for( const JSON& op : ops )
    {
        const std::string kind = Text( op, "op" );

        if( kind == "add" )
        {
            Keys( op, { "op", "node", "path_type", "path" } );
            TREE*             parent = node( op.at( "node" ) );
            const std::string type = Text( op, "path_type" );
            const int         parentIndex = static_cast<int>( U64( op.at( "node" ) ) );

            try
            {
                TREE* child = nullptr;

                if( type == "i64" )
                    child = parent->AddChild( ReadPath<int64_t>( op.at( "path" ) ) );
                else if constexpr( D )
                {
                    if( type == "f64" )
                        child = parent->AddChild( ReadPath<double>( op.at( "path" ) ) );
                    else
                        Invalid( "path_type" );
                }
                else
                    Invalid( "path_type" );

                nodes.push_back( child );
                parents.push_back( parentIndex );
                live.push_back( true );
                events.push_back( JSON{ { "added", nodes.size() - 1 } } );
            }
            catch( const c2::Clipper2Exception& error )
            {
                events.push_back( Thrown( error ) );
            }
        }
        else if( kind == "clear" )
        {
            Keys( op, { "op", "node" } );
            node( op.at( "node" ) )->Clear();
            const int index = static_cast<int>( U64( op.at( "node" ) ) );

            // Every descendant of the cleared node is destroyed.
            for( size_t i = 1; i < nodes.size(); ++i )
            {
                for( int p = parents[i]; p >= 0; p = parents[p] )
                {
                    if( p == index )
                    {
                        live[i] = false;
                        break;
                    }
                }
            }

            events.push_back( JSON{ { "cleared", index } } );
        }
        else if( kind == "child" )
        {
            Keys( op, { "op", "node", "index", "subscript" } );
            TREE*          parent = node( op.at( "node" ) );
            const uint64_t index = U64( op.at( "index" ) );

            if( index >= parent->Count() )
                g.Undefined();

            TREE* child = Bool( op.at( "subscript" ) ) ? ( *parent )[index] : parent->Child( index );
            events.push_back( JSON{ { "child", indexOf( child ) } } );
        }
        else if( kind == "set_scale" )
        {
            Keys( op, { "op", "node", "scale" } );

            if constexpr( D )
                node( op.at( "node" ) )->SetScale( F64( op.at( "scale" ) ) );
            else
                Invalid( "op" );

            events.push_back( JSON{ { "scaled", U64( op.at( "node" ) ) } } );
        }
        else
        {
            Invalid( "op" );
        }
    }

    JSON report = JSON::array();

    for( size_t i = 0; i < nodes.size(); ++i )
    {
        if( !live[i] )
        {
            report.push_back( nullptr );
            continue;
        }

        const TREE* n = nodes[i];
        g.Area( n->Polygon() );
        JSON children = JSON::array();

        for( const auto& child : *n )
            children.push_back( indexOf( child.get() ) );

        // Area() recurses into every descendant; their polygons are guarded too.
        for( size_t j = 0; j < nodes.size(); ++j )
        {
            for( int p = parents[j]; p >= 0 && live[j]; p = parents[p] )
            {
                if( static_cast<size_t>( p ) == i )
                {
                    g.Area( nodes[j]->Polygon() );
                    break;
                }
            }
        }

        const c2::PolyPath* parent = n->Parent();
        JSON parentIndex = nullptr;

        if( parent )
            parentIndex = indexOf( parent );

        JSON entry{ { "level", n->Level() }, { "is_hole", n->IsHole() }, { "parent", parentIndex },
                    { "count", n->Count() }, { "children", children },
                    { "polygon", WritePath<T>( n->Polygon() ) }, { "area", Bits( n->Area() ) } };

        if constexpr( D )
            entry["scale"] = Bits( n->Scale() );

        report.push_back( entry );
    }

    return JSON{ { "events", events }, { "nodes", report } };
}


JSON RunEngine( const std::string& aOp, const JSON& aIn, const GUARD& g )
{
    if( aOp == "vertex_flags" )
    {
        Keys( aIn, { "op", "a", "b", "kind" } );
        const uint64_t a = U64( aIn.at( "a" ) ), b = U64( aIn.at( "b" ) );

        if( a > UINT32_MAX || b > UINT32_MAX )
            Invalid( "flags" );

        const auto fa = static_cast<c2::VertexFlags>( a ), fb = static_cast<c2::VertexFlags>( b );
        const std::string kind = Text( aIn, "kind" );

        if( kind == "and" )
            return static_cast<uint32_t>( fa & fb );

        if( kind == "or" )
            return static_cast<uint32_t>( fa | fb );

        Invalid( "kind" );
    }

    if( aOp == "engine_defaults" )
    {
        Keys( aIn, { "op" } );
        return EngineDefaults();
    }

    if( aOp == "engine_construct" )
    {
        Keys( aIn, { "op", "kind", "point", "flag" } );
        const std::string kind = Text( aIn, "kind" );
        const bool        flag = Bool( aIn.at( "flag" ) );
        c2::Point64       pt = ReadPoint<int64_t>( aIn.at( "point" ) );
        c2::OutRec        rec;
        c2::Vertex        vertex;
        c2::Active        e1, e2;

        if( kind == "outpt" )
        {
            c2::OutPt op( pt, flag ? &rec : nullptr );
            return JSON{ { "pt", WritePoint( op.pt ) }, { "next_self", op.next == &op },
                         { "prev_self", op.prev == &op }, { "outrec_arg", op.outrec == &rec },
                         { "outrec_null", op.outrec == nullptr }, { "horz_null", op.horz == nullptr } };
        }

        if( kind == "local_minima" )
        {
            // flag selects the path type; a nonzero point z selects an open path.
            c2::LocalMinima lm( &vertex, flag ? c2::PathType::Clip : c2::PathType::Subject, pt.z != 0 );
            return JSON{ { "vertex_arg", lm.vertex == &vertex },
                         { "polytype", lm.polytype == c2::PathType::Clip ? "Clip" : "Subject" },
                         { "is_open", lm.is_open } };
        }

        if( kind == "intersect_node" )
        {
            c2::IntersectNode node( flag ? &e1 : nullptr, flag ? &e2 : nullptr, pt );
            return JSON{ { "pt", WritePoint( node.pt ) }, { "edge1_arg", node.edge1 == &e1 },
                         { "edge2_arg", node.edge2 == &e2 }, { "edge1_null", node.edge1 == nullptr },
                         { "edge2_null", node.edge2 == nullptr } };
        }

        if( kind == "horz_segment" )
        {
            c2::OutPt       op( pt, &rec );
            c2::HorzSegment seg( flag ? &op : nullptr );
            return JSON{ { "left_op_arg", seg.left_op == &op }, { "left_op_null", seg.left_op == nullptr },
                         { "right_op_null", seg.right_op == nullptr }, { "left_to_right", seg.left_to_right } };
        }

        if( kind == "horz_join" )
        {
            c2::OutPt    a( pt, &rec ), b( pt, &rec );
            c2::HorzJoin join( flag ? &a : nullptr, flag ? &b : nullptr );
            return JSON{ { "op1_arg", join.op1 == &a }, { "op2_arg", join.op2 == &b },
                         { "op1_null", join.op1 == nullptr }, { "op2_null", join.op2 == nullptr } };
        }

        Invalid( "kind" );
    }

    if( aOp == "stripe_index" )
    {
        Keys( aIn, { "op", "path", "query" } );
        const c2::Path64 path = ReadPath<int64_t>( aIn.at( "path" ) );
        std::optional<c2::Point64> query;

        if( !aIn.at( "query" ).is_null() )
            query = ReadPoint<int64_t>( aIn.at( "query" ) );

        g.StripeIndex( path, query ? &*query : nullptr );
        c2::StripeIndex index;
        index.BuildFromPath( path );
        JSON result{ { "edge_count", index.EdgeCount() }, { "empty", index.Empty() } };

        if( query )
            result["query"] = PipName( index.PointInPolygon( *query ) );

        return result;
    }

    if( aOp == "clipper_settings" )
    {
        Keys( aIn, { "op", "class", "precision", "preserve_collinear", "reverse_solution",
                     "pip_index_threshold" } );
        const std::string             cls = Text( aIn, "class" );
        std::unique_ptr<PROBE64>      c64;
        std::unique_ptr<PROBED>       cd;
        c2::ClipperBase*              base = nullptr;
        bool                          hasZ = false;

        try
        {
            if( cls == "64" )
            {
                if( !aIn.at( "precision" ).is_null() )
                    Invalid( "precision" );

                c64 = std::make_unique<PROBE64>();
                base = c64.get();
            }
            else if( cls == "D" )
            {
                cd = MakeD( aIn.at( "precision" ) );
                base = cd.get();
            }
            else
            {
                Invalid( "class" );
            }
        }
        catch( const c2::Clipper2Exception& error )
        {
            return Thrown( error );
        }

        JSON before{ { "error_code", base->ErrorCode() }, { "preserve_collinear", base->PreserveCollinear() },
                     { "reverse_solution", base->ReverseSolution() },
                     { "pip_index_threshold", base->PipIndexThreshold() }, { "default_z", base->DefaultZ } };

        if( !aIn.at( "preserve_collinear" ).is_null() )
            base->PreserveCollinear( Bool( aIn.at( "preserve_collinear" ) ) );

        if( !aIn.at( "reverse_solution" ).is_null() )
            base->ReverseSolution( Bool( aIn.at( "reverse_solution" ) ) );

        if( !aIn.at( "pip_index_threshold" ).is_null() )
            base->PipIndexThreshold( U64( aIn.at( "pip_index_threshold" ) ) );

        hasZ = c64 ? c64->HasZ() : cd->HasZ();
        JSON after{ { "error_code", base->ErrorCode() }, { "preserve_collinear", base->PreserveCollinear() },
                    { "reverse_solution", base->ReverseSolution() },
                    { "pip_index_threshold", base->PipIndexThreshold() }, { "default_z", base->DefaultZ } };
        return JSON{ { "before", before }, { "after", after }, { "z_callback", hasZ } };
    }

    if( aOp == "clipper64_z_callback" )
    {
        Keys( aIn, { "op", "set" } );
        PROBE64 clipper;
        const bool before = clipper.HasZ();
        clipper.SetZCallback( Bool( aIn.at( "set" ) )
                                      ? c2::ZCallback64( []( const c2::Point64&, const c2::Point64&,
                                                             const c2::Point64&, const c2::Point64&,
                                                             c2::Point64& ) {} )
                                      : c2::ZCallback64( nullptr ) );
        return JSON{ { "before", before }, { "after", clipper.HasZ() } };
    }

    if( aOp == "clipperd_zcb" || aOp == "clipperd_check_callback" )
    {
        Keys( aIn, { "op", "precision", "callback", "points", "z", "invoke" } );
        c2::Point64 pts[5];
        ReadPoints5( aIn.at( "points" ), pts );
        const bool callback = Bool( aIn.at( "callback" ) );
        const bool invoke = Bool( aIn.at( "invoke" ) );
        RECORDER   recorder;
        recorder.z = I64( aIn.at( "z" ) );
        std::unique_ptr<PROBED> clipper;

        try
        {
            clipper = MakeD( aIn.at( "precision" ) );
        }
        catch( const c2::Clipper2Exception& error )
        {
            return Thrown( error );
        }

        if( callback )
            clipper->SetZCallback( recorder.Callback() );

        JSON result{ { "before", clipper->HasZ() } };

        if( aOp == "clipperd_check_callback" )
        {
            clipper->CheckCallback();
            result["after"] = clipper->HasZ();
        }

        if( invoke )
        {
            try
            {
                if( aOp == "clipperd_zcb" )
                    clipper->ZCB( pts[0], pts[1], pts[2], pts[3], pts[4] );
                else
                    clipper->CallBase( pts[0], pts[1], pts[2], pts[3], pts[4] );

                result["pt"] = WritePoint( pts[4] );
            }
            catch( const std::bad_function_call& )
            {
                result["thrown"] = "std::bad_function_call";
            }
        }

        result["calls"] = recorder.calls;
        return result;
    }

    if( aOp == "clipperd_add" )
    {
        Keys( aIn, { "op", "precision", "kind", "paths" } );
        const c2::PathsD paths = ReadPaths<double>( aIn.at( "paths" ) );
        const std::string kind = Text( aIn, "kind" );
        std::unique_ptr<PROBED> clipper;

        try
        {
            clipper = MakeD( aIn.at( "precision" ) );
        }
        catch( const c2::Clipper2Exception& error )
        {
            return Thrown( error );
        }

        // ScalePaths<int64_t, double>( paths, scale_, error_code_ ) runs before AddPaths.
        const double scale = ScaleOf( aIn.at( "precision" ) );
        g.ScalePaths<int64_t>( paths, scale, scale );

        try
        {
            if( kind == "subject" )
                clipper->AddSubject( paths );
            else if( kind == "open_subject" )
                clipper->AddOpenSubject( paths );
            else if( kind == "clip" )
                clipper->AddClip( paths );
            else
                Invalid( "kind" );
        }
        catch( const c2::Clipper2Exception& error )
        {
            return JSON{ { "thrown", error.what() }, { "error_code", clipper->ErrorCode() } };
        }

        return JSON{ { "added", true }, { "error_code", clipper->ErrorCode() } };
    }

    if( aOp == "clipper64_execute" || aOp == "clipperd_execute" )
    {
        Keys( aIn, { "op", "precision", "subjects", "open_subjects", "clips", "clip_type", "fill_rule",
                     "output" } );
        const c2::ClipType ct = ReadClipType( aIn.at( "clip_type" ) );
        const c2::FillRule fr = ReadFillRule( aIn.at( "fill_rule" ) );
        const std::string  output = Text( aIn, "output" );

        if( output != "paths" && output != "paths_open" && output != "tree" && output != "tree_open" )
            Invalid( "output" );

        if( aOp == "clipper64_execute" )
        {
            if( !aIn.at( "precision" ).is_null() )
                Invalid( "precision" );

            // A null path group skips that Add call. Every group is checked against the
            // guarded engine domain before the engine sees any of them.
            std::optional<c2::Paths64> groups[3];
            const char*                fields[3] = { "subjects", "open_subjects", "clips" };

            for( int i = 0; i < 3; ++i )
            {
                if( !aIn.at( fields[i] ).is_null() )
                {
                    groups[i] = ReadPaths<int64_t>( aIn.at( fields[i] ) );
                    g.EngineDomain( *groups[i] );
                }
            }

            c2::Clipper64 clipper;

            if( groups[0] )
                clipper.AddSubject( *groups[0] );

            if( groups[1] )
                clipper.AddOpenSubject( *groups[1] );

            if( groups[2] )
                clipper.AddClip( *groups[2] );
            c2::Paths64 closed{ { c2::Point64( 7, 7 ) } }, open{ { c2::Point64( 8, 8 ) } };
            c2::PolyTree64 tree;
            bool           ok;

            if( output == "paths" )
                ok = clipper.Execute( ct, fr, closed );
            else if( output == "paths_open" )
                ok = clipper.Execute( ct, fr, closed, open );
            else if( output == "tree" )
                ok = clipper.Execute( ct, fr, tree );
            else
                ok = clipper.Execute( ct, fr, tree, open );

            return JSON{ { "succeeded", ok }, { "closed", WritePaths( closed ) }, { "open", WritePaths( open ) },
                         { "tree", Tree64( tree ) }, { "error_code", clipper.ErrorCode() } };
        }

        std::unique_ptr<PROBED> clipper;

        try
        {
            clipper = MakeD( aIn.at( "precision" ) );
        }
        catch( const c2::Clipper2Exception& error )
        {
            return Thrown( error );
        }

        // Each Add* runs ScalePaths<int64_t, double>( paths, scale_, error_code_ ) before
        // AddPaths: guard it exactly as clipperd_add does, group by group in call order, so a
        // range exception from an earlier group still stops before a later group is scaled.
        const double scale = ScaleOf( aIn.at( "precision" ) );
        const char*  fields[3] = { "subjects", "open_subjects", "clips" };

        for( int i = 0; i < 3; ++i )
        {
            if( aIn.at( fields[i] ).is_null() )
                continue;

            const c2::PathsD paths = ReadPaths<double>( aIn.at( fields[i] ) );
            g.ScalePaths<int64_t>( paths, scale, scale );

            try
            {
                if( i == 0 )
                    clipper->AddSubject( paths );
                else if( i == 1 )
                    clipper->AddOpenSubject( paths );
                else
                    clipper->AddClip( paths );
            }
            catch( const c2::Clipper2Exception& error )
            {
                return Thrown( error );
            }
        }

        c2::PathsD     closed, open;
        c2::PolyTreeD  tree;
        bool           ok;

        if( output == "paths" )
            ok = clipper->Execute( ct, fr, closed );
        else if( output == "paths_open" )
            ok = clipper->Execute( ct, fr, closed, open );
        else if( output == "tree" )
            ok = clipper->Execute( ct, fr, tree );
        else
            ok = clipper->Execute( ct, fr, tree, open );

        return JSON{ { "succeeded", ok }, { "closed", WritePaths( closed ) }, { "open", WritePaths( open ) },
                     { "tree", TreeD( tree ) }, { "error_code", clipper->ErrorCode() } };
    }

    if( aOp == "polytree" )
    {
        Keys( aIn, { "op", "type", "root", "root_type", "ops" } );
        const std::string type = Text( aIn, "type" );

        if( type == "i64" )
            return RunTree<c2::PolyPath64>( aIn, g );

        if( type == "f64" )
            return RunTree<c2::PolyPathD>( aIn, g );

        Invalid( "type" );
    }

    if( aOp == "polypath_base_count" )
    {
        Keys( aIn, { "op" } );
        c2::PolyPath64 tree;
        tree.AddChild( c2::Path64() );
        return JSON{ { "base", static_cast<c2::PolyPath&>( tree ).PolyPath::Count() },
                     { "derived", static_cast<c2::PolyPath&>( tree ).Count() } };
    }

    Invalid( "op" );
}


JSON Run( const JSON& aIn )
{
    const std::string op = Text( aIn, "op" );
    const GUARD       g{ op };

    if( op == "constants" )
    {
        Keys( aIn, { "op" } );
        return JSON{ { "max_coord", c2::MAX_COORD }, { "min_coord", c2::MIN_COORD }, { "invalid", c2::INVALID },
                     { "max_coord_d", Bits( c2::max_coord ) }, { "min_coord_d", Bits( c2::min_coord ) },
                     { "max_dbl", Bits( c2::MAX_DBL ) }, { "pi", Bits( c2::PI ) },
                     { "max_dec_precision", c2::CLIPPER2_MAX_DEC_PRECISION },
                     { "error_codes", JSON::array( { c2::precision_error_i, c2::scale_error_i, c2::non_pair_error_i,
                                                     c2::undefined_error_i, c2::range_error_i } ) },
                     { "messages", JSON::array( { c2::precision_error, c2::range_error, c2::scale_error,
                                                  c2::non_pair_error, c2::undefined_error } ) },
                     { "invalid_point64", WritePoint( c2::InvalidPoint64 ) },
                     { "invalid_pointd", WritePoint( c2::InvalidPointD ) },
                     { "invalid_rect64", WriteRect( c2::InvalidRect64 ) },
                     { "invalid_rectd", WriteRect( c2::InvalidRectD ) } };
    }

    if( op == "do_error" )
    {
        Keys( aIn, { "op", "code" } );

        try
        {
            c2::DoError( I32( aIn.at( "code" ) ) );
        }
        catch( const c2::Clipper2Exception& error )
        {
            return Thrown( error );
        }

        throw std::logic_error( "DoError returned" );
    }

    if( op == "exception" )
    {
        Keys( aIn, { "op", "message" } );
        const std::string        message = Text( aIn, "message" );
        const c2::Clipper2Exception error( message.c_str() );
        return JSON{ { "what", error.what() } };
    }

    if( op == "check_precision" )
    {
        Keys( aIn, { "op", "precision", "error_code" } );
        int precision = I32( aIn.at( "precision" ) );

        try
        {
            if( aIn.at( "error_code" ).is_null() )
            {
                c2::CheckPrecisionRange( precision );
                return JSON{ { "precision", precision } };
            }

            int error_code = I32( aIn.at( "error_code" ) );

            try
            {
                c2::CheckPrecisionRange( precision, error_code );
            }
            catch( const c2::Clipper2Exception& error )
            {
                return JSON{ { "thrown", error.what() }, { "precision", precision }, { "error_code", error_code } };
            }

            return JSON{ { "precision", precision }, { "error_code", error_code } };
        }
        catch( const c2::Clipper2Exception& error )
        {
            return JSON{ { "thrown", error.what() }, { "precision", precision } };
        }
    }

    if( op == "tri_sign" )
    {
        Keys( aIn, { "op", "x" } );
        return c2::TriSign( I64( aIn.at( "x" ) ) );
    }

    if( op == "multiply_u64" )
    {
        Keys( aIn, { "op", "a", "b" } );
        const c2::UInt128Struct r = c2::MultiplyUInt64( U64( aIn.at( "a" ) ), U64( aIn.at( "b" ) ) );
        return JSON{ { "lo", r.lo }, { "hi", r.hi } };
    }

    if( op == "uint128_eq" )
    {
        Keys( aIn, { "op", "a", "b" } );
        const JSON& a = aIn.at( "a" );
        const JSON& b = aIn.at( "b" );

        if( !a.is_array() || a.size() != 2 || !b.is_array() || b.size() != 2 )
            Invalid( "uint128" );

        const c2::UInt128Struct x{ U64( a[0] ), U64( a[1] ) }, y{ U64( b[0] ), U64( b[1] ) };
        return x == y;
    }

    if( op == "products_are_equal" )
    {
        Keys( aIn, { "op", "a", "b", "c", "d" } );
        return c2::ProductsAreEqual( I64( aIn.at( "a" ) ), I64( aIn.at( "b" ) ), I64( aIn.at( "c" ) ),
                                     I64( aIn.at( "d" ) ) );
    }

    if( op == "segments_intersect" )
    {
        Keys( aIn, { "op", "points", "inclusive" } );
        const c2::Path64 pts = ReadPath<int64_t>( aIn.at( "points" ) );

        if( pts.size() != 4 )
            Invalid( "points" );

        const double dy1 = static_cast<double>( g.Sub( pts[1].y, pts[0].y ) );
        const double dx1 = static_cast<double>( g.Sub( pts[1].x, pts[0].x ) );
        const double dy2 = static_cast<double>( g.Sub( pts[3].y, pts[2].y ) );
        const double dx2 = static_cast<double>( g.Sub( pts[3].x, pts[2].x ) );

        if( dy1 * dx2 - dy2 * dx1 != 0 )
        {
            g.Sub( pts[0].x, pts[2].x );
            g.Sub( pts[0].y, pts[2].y );
        }

        if( aIn.at( "inclusive" ).is_null() )
            return c2::SegmentsIntersect( pts[0], pts[1], pts[2], pts[3] );

        return c2::SegmentsIntersect( pts[0], pts[1], pts[2], pts[3], Bool( aIn.at( "inclusive" ) ) );
    }

    if( op == "append_path" )
    {
        Keys( aIn, { "op", "type", "paths", "path" } );
        const std::string type = Text( aIn, "type" );
        auto run = [&]( auto aTag ) -> JSON
        {
            using T = decltype( aTag );
            c2::Paths<T>      paths = ReadPaths<T>( aIn.at( "paths" ) );
            const c2::Path<T> path = ReadPath<T>( aIn.at( "path" ) );
            c2::Paths<T>&     self = ( paths << path );
            return JSON{ { "paths", WritePaths( paths ) }, { "returns_self", &self == &paths } };
        };

        if( type == "i64" )
            return run( int64_t() );

        if( type == "f64" )
            return run( double() );

        Invalid( "type" );
    }

    static const std::vector<std::string> mixed = { "point_construct", "point_convert", "point_init",
                                                    "append_point", "scale_rect", "get_bounds", "scale_path",
                                                    "scale_paths", "transform_path", "transform_paths" };

    if( std::find( mixed.begin(), mixed.end(), op ) != mixed.end() )
    {
        const std::string to = Text( aIn, "to" );

        if( to == "i64" )
            return DispatchFrom<int64_t>( op, aIn, g );

        if( to == "f64" )
            return DispatchFrom<double>( op, aIn, g );

        Invalid( "to" );
    }

    if( aIn.is_object() && aIn.contains( "type" ) && op != "polytree" )
    {
        const std::string type = Text( aIn, "type" );

        if( type == "i64" )
            return RunTyped<int64_t>( op, aIn, g );

        if( type == "f64" )
            return RunTyped<double>( op, aIn, g );

        Invalid( "type" );
    }

    return RunEngine( op, aIn, g );
}
} // namespace


namespace oracle
{
void RegisterClipper2CoreEngine( REGISTRY& aRegistry )
{
    aRegistry.Add( CLIPPER2_UNIT, "thirdparty/clipper2/Clipper2Lib/include/clipper2/clipper.core.h", Run );
}
} // namespace oracle
