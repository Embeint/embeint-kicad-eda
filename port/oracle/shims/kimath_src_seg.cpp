// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned SEG (libs/kimath/src/geometry/seg.cpp).
//
// The adapter decodes 32-bit integer points and IEEE bit strings and encodes every observable
// value exactly; it never computes a reply itself. Before each call, a guard walks the pinned
// evaluation and refuses the request when it would reach undefined behaviour (signed overflow,
// negation of the minimum, an out-of-range floating-to-integer cast). The guards check only
// the arithmetic: where the pinned control flow depends on a value, they take it from the
// genuine function, called only after its own guard has passed.
#include "registry.h"
#include <geometry/seg.h>
#include <math/util.h>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace
{
using oracle::JSON;
constexpr const char* SEG_UNIT = "libs.kimath.src.geometry.seg.application.ui.p001";


[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined seg operation" );
}


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


bool readBool( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_boolean() )
        throw std::invalid_argument( std::string( "expected bool: " ) + aName );
    return aInput.at( aName ).get<bool>();
}


int readInt( const JSON& aInput )
{
    if( !aInput.is_number_integer() || ( aInput.is_number_unsigned() && aInput.get<uint64_t>() > INT_MAX )
        || aInput.get<int64_t>() < INT_MIN || aInput.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return aInput.get<int>();
}


int readInt( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( std::string( "expected integer: " ) + aName );
    return readInt( aInput.at( aName ) );
}


// Doubles travel as 16 lower-case hex digits of their IEEE bits.
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


VECTOR2I readPoint( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "expected point" );
    return readPoint( aInput.at( aName ) );
}


SEG readSeg( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() || aInput.at( aName ).size() != 2 )
        throw std::invalid_argument( std::string( "expected segment: " ) + aName );
    return SEG( readPoint( aInput.at( aName )[0] ), readPoint( aInput.at( aName )[1] ) );
}


JSON point( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


// ---------------------------------------------------------------------------------------------
// Guards. Each mirrors the integer arithmetic of one pinned body in evaluation order.

int64_t L( __int128 aValue )
{
    if( aValue < INT64_MIN || aValue > INT64_MAX )
        undefined();
    return static_cast<int64_t>( aValue );
}


int I( int64_t aValue )
{
    if( aValue < INT_MIN || aValue > INT_MAX )
        undefined();
    return static_cast<int>( aValue );
}


// VECTOR2I - VECTOR2I and VECTOR2I + VECTOR2I: int components.
VECTOR2I subI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) - aB.x ), I( int64_t( aA.y ) - aB.y ) );
}


VECTOR2I addI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) + aB.x ), I( int64_t( aA.y ) + aB.y ) );
}


// VECTOR2<int64_t>::Dot / Cross: products, then the sum or difference.
int64_t dotL( const VECTOR2L& aA, const VECTOR2L& aB )
{
    const int64_t x = L( __int128( aA.x ) * aB.x );
    const int64_t y = L( __int128( aA.y ) * aB.y );
    return L( __int128( x ) + y );
}


int64_t crossL( const VECTOR2L& aA, const VECTOR2L& aB )
{
    const int64_t x = L( __int128( aA.x ) * aB.y );
    const int64_t y = L( __int128( aA.y ) * aB.x );
    return L( __int128( x ) - y );
}


// VECTOR2I::Dot: the int64 products always fit; only their sum can overflow.
int64_t dotI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return L( __int128( int64_t( aA.x ) * aB.x ) + int64_t( aA.y ) * aB.y );
}


VECTOR2L wideSub( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2L( int64_t( aA.x ) - aB.x, int64_t( aA.y ) - aB.y );
}


void guardSquaredDistance( const SEG& aSeg, const VECTOR2I& aP )
{
    const VECTOR2L ab = wideSub( aSeg.B, aSeg.A );
    const VECTOR2L ap = wideSub( aP, aSeg.A );
    const int64_t  e = dotL( ap, ab );

    if( e <= 0 )
        return;

    const int64_t f = ab.SquaredEuclideanNorm();

    if( e >= f )
    {
        const VECTOR2L bp = wideSub( aP, aSeg.B );
        dotL( bp, bp );
    }
    // The remaining path is wide or floating arithmetic and a nonquiet KiROUND.
}


void guardNearestPoint( const SEG& aSeg, const VECTOR2I& aP )
{
    const VECTOR2L d = wideSub( aSeg.B, aSeg.A );
    const int64_t  lSquared = L( __int128( L( __int128( d.x ) * d.x ) ) + L( __int128( d.y ) * d.y ) );

    if( lSquared == 0 )
        return;

    const int64_t t = dotL( d, wideSub( aP, aSeg.A ) );

    if( t < 0 || t > lSquared )
        return;

    const int64_t xp = rescale( t, d.x, lSquared );
    const int64_t yp = rescale( t, d.y, lSquared );
    L( __int128( aSeg.A.x ) + xp );
    L( __int128( aSeg.A.y ) + yp );
}


void guardCollinearOverlap( const SEG& aThis, const SEG& aSeg, bool aUseX, bool aIgnoreEndpoints,
                            bool aWantPoint )
{
    const int s1s = aUseX ? aThis.A.x : aThis.A.y;
    const int s1e = aUseX ? aThis.B.x : aThis.B.y;
    const int s2s = aUseX ? aSeg.A.x : aSeg.A.y;
    const int s2e = aUseX ? aSeg.B.x : aSeg.B.y;
    const int c1s = aUseX ? aThis.A.y : aThis.A.x;
    const int c1e = aUseX ? aThis.B.y : aThis.B.x;
    const int s1min = std::min( s1s, s1e );
    const int s1max = std::max( s1s, s1e );
    const int s2min = std::min( s2s, s2e );
    const int s2max = std::max( s2s, s2e );

    if( !( s1max >= s2min && s2max >= s1min ) )
        return;

    const int start = std::max( s1min, s2min );
    const int end = std::min( s1max, s2max );

    if( aIgnoreEndpoints && start == end && ( start == s1min || start == s1max )
        && ( start == s2min || start == s2max ) )
    {
        return;
    }

    if( !aWantPoint )
        return;

    const int proj = I( int64_t( start ) + end ) / 2;

    if( s1e != s1s )
    {
        const int n = I( int64_t( proj ) - s1s );
        const int v = I( int64_t( c1e ) - c1s );
        const int d = I( int64_t( s1e ) - s1s );
        I( int64_t( c1s ) + rescale( n, v, d ) );
    }
}


void guardIntersects( const SEG& aThis, const SEG& aSeg, bool aIgnoreEndpoints, bool aLines,
                      bool aWantPoint )
{
    if( !aLines )
    {
        if( std::max( aThis.A.x, aThis.B.x ) < std::min( aSeg.A.x, aSeg.B.x )
            || std::max( aSeg.A.x, aSeg.B.x ) < std::min( aThis.A.x, aThis.B.x )
            || std::max( aThis.A.y, aThis.B.y ) < std::min( aSeg.A.y, aSeg.B.y )
            || std::max( aSeg.A.y, aSeg.B.y ) < std::min( aThis.A.y, aThis.B.y ) )
        {
            return;
        }
    }

    const VECTOR2L dir1 = wideSub( aThis.B, aThis.A );
    const VECTOR2L dir2 = wideSub( aSeg.B, aSeg.A );
    const VECTOR2L offset = wideSub( aSeg.A, aThis.A );
    const int64_t  determinant = crossL( dir2, dir1 );

    if( determinant == 0 )
    {
        if( crossL( dir1, offset ) != 0 )
            return;

        if( aLines )
        {
            if( aWantPoint && aSeg.A != aSeg.B && aThis.A != aThis.B )
                addI( aThis.A, aSeg.A );
            return;
        }

        guardCollinearOverlap( aThis, aSeg, std::abs( dir1.x ) >= std::abs( dir1.y ), aIgnoreEndpoints,
                               aWantPoint );
        return;
    }

    const int64_t param2 = crossL( dir2, offset );
    const int64_t param1 = crossL( dir1, offset );

    if( !aLines )
    {
        if( determinant > 0 )
        {
            if( param1 < 0 || param1 > determinant || param2 < 0 || param2 > determinant )
                return;
        }
        else if( param1 > 0 || param1 < determinant || param2 > 0 || param2 < determinant )
        {
            return;
        }

        if( aIgnoreEndpoints && ( param1 == 0 || param1 == determinant )
            && ( param2 == 0 || param2 == determinant ) )
        {
            return;
        }
    }

    if( aWantPoint )
    {
        const int64_t sx = rescale( param1, dir2.x, determinant );
        const int64_t sy = rescale( param1, dir2.y, determinant );
        L( __int128( aSeg.A.x ) + sx );
        L( __int128( aSeg.A.y ) + sy );
    }
}


void guardSquaredDistanceSeg( const SEG& aThis, const SEG& aSeg )
{
    if( aThis.A == aThis.B )
        return guardSquaredDistance( aSeg, aThis.A );
    if( aSeg.A == aSeg.B )
        return guardSquaredDistance( aThis, aSeg.A );

    guardIntersects( aThis, aSeg, false, false, false );

    if( aThis.Intersects( aSeg ) )
        return;

    const std::pair<const SEG*, VECTOR2I> queries[4] = {
        { &aSeg, aThis.A }, { &aSeg, aThis.B }, { &aThis, aSeg.A }, { &aThis, aSeg.B }
    };

    for( const auto& [seg, p] : queries )
    {
        guardNearestPoint( *seg, p );
        subI( seg->NearestPoint( p ), p );
    }
}


void guardNearestPointSeg( const SEG& aThis, const SEG& aSeg )
{
    guardIntersects( aThis, aSeg, false, false, true );

    if( aThis.Intersect( aSeg ) )
        return;

    guardNearestPoint( aSeg, aThis.A );
    guardNearestPoint( aSeg, aThis.B );
    guardNearestPoint( aThis, aSeg.A );
    guardNearestPoint( aThis, aSeg.B );
    subI( aSeg.NearestPoint( aThis.A ), aThis.A );
    subI( aSeg.NearestPoint( aThis.B ), aThis.B );
    subI( aThis.NearestPoint( aSeg.A ), aSeg.A );
    subI( aThis.NearestPoint( aSeg.B ), aSeg.B );
}


void guardIntersectsLine( const SEG& aSeg, double aSlope, double aOffset )
{
    const VECTOR2L segDir = wideSub( aSeg.B, aSeg.A );

    if( segDir.x == 0 )
        return;

    // static_cast<ecoord>( aSlope * 1000 ) is defined only for a representable truncation.
    const double scaled = aSlope * 1000;

    if( std::isnan( scaled ) || !( std::trunc( scaled ) >= -9223372036854775808.0 )
        || !( std::trunc( scaled ) < 9223372036854775808.0 ) )
    {
        undefined();
    }

    const VECTOR2L lineDir( 1000, static_cast<int64_t>( scaled ) );

    if( crossL( segDir, lineDir ) == 0 )
    {
        const double expected = aSlope * aSeg.A.x + aOffset;

        if( std::abs( aSeg.A.y - expected ) < 0.5 )
            addI( aSeg.A, aSeg.B );
    }
}


void guardPerpendicular( const SEG& aSeg, const VECTOR2I& aP )
{
    const VECTOR2I slope = subI( aSeg.B, aSeg.A );

    if( slope.y == INT_MIN )
        undefined();

    addI( VECTOR2I( -slope.y, slope.x ), aP );
}


void guardParallel( const SEG& aSeg, const VECTOR2I& aP )
{
    addI( subI( aSeg.B, aSeg.A ), aP );
}


void guardCollide( const SEG& aThis, const SEG& aSeg, int aClearance )
{
    if( aClearance < 0 )
        return;
    if( aThis.A == aThis.B )
        return guardSquaredDistance( aSeg, aThis.A );
    if( aSeg.A == aSeg.B )
        return guardSquaredDistance( aThis, aSeg.A );

    guardIntersects( aThis, aSeg, false, false, false );

    if( aThis.Intersects( aSeg ) )
        return;

    const std::pair<const SEG*, VECTOR2I> queries[4] = {
        { &aThis, aSeg.A }, { &aThis, aSeg.B }, { &aSeg, aThis.A }, { &aSeg, aThis.B }
    };

    for( const auto& [seg, p] : queries )
    {
        guardSquaredDistance( *seg, p );

        if( seg->SquaredDistance( p ) == 0 )
            return;
    }
}


void guardReflectOrProject( const SEG& aSeg, const VECTOR2I& aP, bool aReflect )
{
    const VECTOR2I d = subI( aSeg.B, aSeg.A );
    const int64_t  lSquared = dotI( d, d );

    if( !aReflect && lSquared == 0 )
        return;

    const int64_t t = dotI( d, subI( aP, aSeg.A ) );

    if( lSquared == 0 )
        return; // ReflectPoint: c = aP, and 2 * c - aP fits.

    const int64_t cx = L( __int128( aSeg.A.x ) + rescale( t, int64_t( d.x ), lSquared ) );
    const int64_t cy = L( __int128( aSeg.A.y ) + rescale( t, int64_t( d.y ), lSquared ) );

    if( aReflect )
    {
        L( __int128( L( __int128( 2 ) * cx ) ) - aP.x );
        L( __int128( L( __int128( 2 ) * cy ) ) - aP.y );
    }
}


// p, q, r, l of LineDistance and mutualDistanceSquared; returns them for the caller's guard.
void guardCanonical( const SEG& aSeg, int64_t& aP, int64_t& aQ, int64_t& aR, int64_t& aL )
{
    aP = int64_t( aSeg.A.y ) - aSeg.B.y;
    aQ = int64_t( aSeg.B.x ) - aSeg.A.x;
    const int64_t left = L( __int128( -aP ) * aSeg.A.x );
    const int64_t right = L( __int128( aQ ) * aSeg.A.y );
    aR = L( __int128( left ) - right );
    aL = L( __int128( L( __int128( aP ) * aP ) ) + L( __int128( aQ ) * aQ ) );
}


int64_t guardDet( int64_t aP, int64_t aQ, int64_t aR, const VECTOR2I& aPoint )
{
    const int64_t x = L( __int128( aP ) * aPoint.x );
    const int64_t y = L( __int128( aQ ) * aPoint.y );
    return L( __int128( L( __int128( x ) + y ) ) + aR );
}


void guardLineDistance( const SEG& aSeg, const VECTOR2I& aP )
{
    int64_t p, q, r, l;
    guardCanonical( aSeg, p, q, r, l );
    guardDet( p, q, r, aP );
    // rescale, isqrt and sgn * dist (|dist| <= 3037000499) are defined.
}


// mutualDistanceSquared; returns false when it does, and the two signed distances otherwise.
bool guardMutual( const SEG& aThis, const SEG& aSeg, int64_t& aD1, int64_t& aD2 )
{
    SEG a( aThis );
    SEG b( aSeg );
    const VECTOR2I la = subI( a.A, a.B );
    const VECTOR2I lb = subI( b.A, b.B );

    if( la.SquaredEuclideanNorm() < lb.SquaredEuclideanNorm() )
        std::swap( a, b );

    int64_t p, q, r, l;
    guardCanonical( a, p, q, r, l );

    if( l == 0 )
        return false;

    const int64_t det1 = guardDet( p, q, r, b.A );
    const int64_t det2 = guardDet( p, q, r, b.B );
    const int64_t dsq1 = rescale( det1, det1, l );
    const int64_t dsq2 = rescale( det2, det2, l );

    // sgn( det ) * dsq is undefined only for -1 * INT64_MIN.
    if( ( det1 < 0 && dsq1 == INT64_MIN ) || ( det2 < 0 && dsq2 == INT64_MIN ) )
        undefined();

    aD1 = int64_t( ( det1 > 0 ) - ( det1 < 0 ) ) * dsq1;
    aD2 = int64_t( ( det2 > 0 ) - ( det2 < 0 ) ) * dsq2;
    return true;
}


void guardApproxCollinear( const SEG& aThis, const SEG& aSeg, int aThreshold )
{
    int64_t d1, d2;

    if( !guardMutual( aThis, aSeg, d1, d2 ) )
        return;

    const int64_t t = int64_t( aThreshold ) * aThreshold;

    if( d1 == INT64_MIN )
        undefined();
    if( std::abs( d1 ) <= t && d2 == INT64_MIN )
        undefined();
}


void guardApproxParallel( const SEG& aThis, const SEG& aSeg )
{
    int64_t d1, d2;

    if( !guardMutual( aThis, aSeg, d1, d2 ) )
        return;

    if( L( __int128( d1 ) - d2 ) == INT64_MIN )
        undefined();
}


// ---------------------------------------------------------------------------------------------

JSON segment( const SEG& aSeg )
{
    return JSON{ { "a", point( aSeg.A ) }, { "b", point( aSeg.B ) }, { "index", aSeg.Index() } };
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "squared_distance_point" || op == "distance_point" || op == "nearest_point_point"
        || op == "reflect_point" || op == "line_project" || op == "perpendicular_seg"
        || op == "parallel_seg" )
    {
        checkKeys( aInput, { "op", "seg", "point" } );
        const SEG      seg = readSeg( aInput, "seg" );
        const VECTOR2I p = readPoint( aInput, "point" );

        if( op == "squared_distance_point" || op == "distance_point" )
        {
            guardSquaredDistance( seg, p );
            if( op == "distance_point" )
                return seg.Distance( p );
            return seg.SquaredDistance( p );
        }
        if( op == "nearest_point_point" )
        {
            guardNearestPoint( seg, p );
            return point( seg.NearestPoint( p ) );
        }
        if( op == "reflect_point" || op == "line_project" )
        {
            guardReflectOrProject( seg, p, op == "reflect_point" );
            return point( op == "reflect_point" ? seg.ReflectPoint( p ) : seg.LineProject( p ) );
        }
        if( op == "perpendicular_seg" )
        {
            guardPerpendicular( seg, p );
            return segment( seg.PerpendicularSeg( p ) );
        }
        guardParallel( seg, p );
        return segment( seg.ParallelSeg( p ) );
    }

    if( op == "contains_point" )
    {
        const bool hasThreshold = aInput.contains( "threshold" );
        if( hasThreshold )
            checkKeys( aInput, { "op", "seg", "point", "threshold" } );
        else
            checkKeys( aInput, { "op", "seg", "point" } );
        const SEG      seg = readSeg( aInput, "seg" );
        const VECTOR2I p = readPoint( aInput, "point" );
        const int      threshold = hasThreshold ? readInt( aInput, "threshold" ) : 0;
        guardSquaredDistance( seg, p );
        return hasThreshold ? seg.Contains( p, threshold ) : seg.Contains( p );
    }

    if( op == "line_distance" )
    {
        const bool hasSide = aInput.contains( "determine_side" );
        if( hasSide )
            checkKeys( aInput, { "op", "seg", "point", "determine_side" } );
        else
            checkKeys( aInput, { "op", "seg", "point" } );
        const SEG      seg = readSeg( aInput, "seg" );
        const VECTOR2I p = readPoint( aInput, "point" );
        const bool     side = hasSide ? readBool( aInput, "determine_side" ) : false;
        guardLineDistance( seg, p );
        return hasSide ? seg.LineDistance( p, side ) : seg.LineDistance( p );
    }

    if( op == "squared_distance_seg" || op == "distance_seg" || op == "angle"
        || op == "nearest_point_seg" || op == "nearest_points" || op == "intersects"
        || op == "approx_perpendicular" )
    {
        checkKeys( aInput, { "op", "seg", "other" } );
        const SEG seg = readSeg( aInput, "seg" );
        const SEG other = readSeg( aInput, "other" );

        if( op == "squared_distance_seg" || op == "distance_seg" )
        {
            guardSquaredDistanceSeg( seg, other );
            if( op == "distance_seg" )
                return seg.Distance( other );
            return seg.SquaredDistance( other );
        }
        if( op == "angle" )
        {
            subI( seg.A, seg.B );
            subI( other.A, other.B );
            return bits( seg.Angle( other ).AsDegrees() );
        }
        if( op == "nearest_point_seg" )
        {
            guardNearestPointSeg( seg, other );
            return point( seg.NearestPoint( other ) );
        }
        if( op == "nearest_points" )
        {
            guardNearestPointSeg( seg, other );
            VECTOR2I a, b;
            int64_t  dist = 0;
            const bool returned = seg.NearestPoints( other, a, b, dist );
            return JSON{ { "returned", returned }, { "a", point( a ) }, { "b", point( b ) }, { "dist", dist } };
        }
        if( op == "intersects" )
        {
            guardIntersects( seg, other, false, false, false );
            return seg.Intersects( other );
        }
        guardPerpendicular( seg, seg.A );
        guardApproxParallel( other, seg.PerpendicularSeg( seg.A ) );
        return seg.ApproxPerpendicular( other );
    }

    if( op == "intersect" )
    {
        // The defaults are exercised by omitting trailing arguments, as in C++.
        const bool hasLines = aInput.contains( "lines" );
        const bool hasIgnore = aInput.contains( "ignore_endpoints" );
        if( hasLines )
            checkKeys( aInput, { "op", "seg", "other", "ignore_endpoints", "lines" } );
        else if( hasIgnore )
            checkKeys( aInput, { "op", "seg", "other", "ignore_endpoints" } );
        else
            checkKeys( aInput, { "op", "seg", "other" } );
        const SEG  seg = readSeg( aInput, "seg" );
        const SEG  other = readSeg( aInput, "other" );
        const bool ignore = hasIgnore ? readBool( aInput, "ignore_endpoints" ) : false;
        const bool lines = hasLines ? readBool( aInput, "lines" ) : false;
        guardIntersects( seg, other, ignore, lines, true );
        const OPT_VECTOR2I result = hasLines    ? seg.Intersect( other, ignore, lines )
                                    : hasIgnore ? seg.Intersect( other, ignore )
                                                : seg.Intersect( other );
        return result ? point( *result ) : JSON( nullptr );
    }

    if( op == "intersects_line" )
    {
        checkKeys( aInput, { "op", "seg", "slope", "offset", "initial" } );
        const SEG    seg = readSeg( aInput, "seg" );
        const double slope = readNumber( aInput, "slope" );
        const double offset = readNumber( aInput, "offset" );
        VECTOR2I     intersection = readPoint( aInput, "initial" );
        guardIntersectsLine( seg, slope, offset );
        const bool returned = seg.IntersectsLine( slope, offset, intersection );
        return JSON{ { "returned", returned }, { "intersection", point( intersection ) } };
    }

    if( op == "collide" )
    {
        const bool hasActual = aInput.contains( "actual" );
        if( hasActual )
            checkKeys( aInput, { "op", "seg", "other", "clearance", "actual" } );
        else
            checkKeys( aInput, { "op", "seg", "other", "clearance" } );
        const SEG seg = readSeg( aInput, "seg" );
        const SEG other = readSeg( aInput, "other" );
        const int clearance = readInt( aInput, "clearance" );

        if( hasActual )
        {
            int actual = readInt( aInput, "actual" );
            guardCollide( seg, other, clearance );
            const bool returned = seg.Collide( other, clearance, &actual );
            return JSON{ { "returned", returned }, { "actual", actual } };
        }
        guardCollide( seg, other, clearance );
        return JSON{ { "returned", seg.Collide( other, clearance ) }, { "actual", nullptr } };
    }

    if( op == "approx_collinear" || op == "approx_parallel" )
    {
        const bool hasThreshold = aInput.contains( "threshold" );
        if( hasThreshold )
            checkKeys( aInput, { "op", "seg", "other", "threshold" } );
        else
            checkKeys( aInput, { "op", "seg", "other" } );
        const SEG seg = readSeg( aInput, "seg" );
        const SEG other = readSeg( aInput, "other" );
        const int threshold = hasThreshold ? readInt( aInput, "threshold" ) : 1;

        if( op == "approx_collinear" )
        {
            guardApproxCollinear( seg, other, threshold );
            return hasThreshold ? seg.ApproxCollinear( other, threshold ) : seg.ApproxCollinear( other );
        }
        guardApproxParallel( seg, other );
        return hasThreshold ? seg.ApproxParallel( other, threshold ) : seg.ApproxParallel( other );
    }

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterSeg( REGISTRY& aRegistry )
{
    aRegistry.Add( SEG_UNIT, "geometry/seg.cpp", run );
}
} // namespace oracle
