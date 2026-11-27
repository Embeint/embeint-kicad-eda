// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned seg.h, shape.h, shape_arc.h, shape_circle.h, shape_compound.h and
// shape_ellipse.h inline bodies (unit libs.kimath.include.geometry.application.ui.p003).
//
// The adapter decodes 32-bit integer points and IEEE bit strings, calls the pinned header
// bodies (and, through them, the genuine kimath library) and encodes every observable value
// exactly; it never computes a reply itself. Shapes are driven by a script: one shape is built
// from a specification, a list of calls is applied to it in order, and every call's reply and
// the final state (read back through the inline getters) are returned.
//
// Before each operation a guard walks the pinned evaluation and refuses the request when it
// would reach undefined behaviour (signed overflow, std::abs of the minimum, an out-of-range
// floating-to-integer conversion). Where control flow or a later operand depends on a computed
// value, the guard takes it from the genuine function, called only after its own guard passed.
//
// Calls into out-of-line bodies this unit does not port and whose undefined domain is not
// replayed here (the three-point SHAPE_ARC constructor, shape_collisions.cpp, the out-of-line
// SHAPE_COMPOUND and SHAPE_ARC members) are accepted only inside a verified domain: every
// coordinate, radius, width and clearance within 2^20 (radii and clearances non-negative where
// noted), compounds holding only circles, and arcs whose mid-point angle has a sine of at least
// 1/8. Anything else is refused as "outside the verified domain" before the call.
#include "registry.h"
#include <geometry/circle.h>
#include <geometry/eda_angle.h>
#include <geometry/seg.h>
#include <geometry/shape.h>
#include <geometry/shape_arc.h>
#include <geometry/shape_circle.h>
#include <geometry/shape_compound.h>
#include <geometry/shape_ellipse.h>
#include <math/box2.h>
#include <math/util.h>
#include <trigo.h>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iomanip>
#include <locale>
#include <memory>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* P003_UNIT = "libs.kimath.include.geometry.application.ui.p003";
constexpr int64_t     DOMAIN = int64_t( 1 ) << 20;

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined p003 operation" );
}


[[noreturn]] void outside()
{
    throw std::invalid_argument( "outside the verified domain" );
}


// ---------------------------------------------------------------------------------------------
// Decoding and encoding.

void checkKeys( const JSON& aInput, std::initializer_list<const char*> aRequired,
                std::initializer_list<const char*> aOptional = {} )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    for( const char* field : aRequired )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
    for( const auto& [key, value] : aInput.items() )
    {
        bool known = false;
        for( const char* field : aRequired )
            known = known || key == field;
        for( const char* field : aOptional )
            known = known || key == field;
        if( !known )
            throw std::invalid_argument( "unexpected or missing field" );
    }
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


int readInt( const JSON& aValue )
{
    if( !aValue.is_number_integer() || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
        || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<int>();
}


int readInt( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    return readInt( aInput.at( aName ) );
}


double readBits( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    const std::string text = aInput.at( aName ).get<std::string>();
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


VECTOR2I readPoint( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2I( readInt( aValue[0] ), readInt( aValue[1] ) );
}


VECTOR2I readPoint( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) )
        throw std::invalid_argument( "unexpected or missing field" );
    return readPoint( aInput.at( aName ) );
}


SEG readSeg( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() || aInput.at( aName ).size() != 2 )
        throw std::invalid_argument( std::string( "expected segment: " ) + aName );
    return SEG( readPoint( aInput.at( aName )[0] ), readPoint( aInput.at( aName )[1] ) );
}


const JSON& readArray( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    return aInput.at( aName );
}


JSON point( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON segment( const SEG& aSeg )
{
    return JSON{ { "a", point( aSeg.A ) }, { "b", point( aSeg.B ) }, { "index", aSeg.Index() } };
}


// ---------------------------------------------------------------------------------------------
// Integer guards: the first undefined operation refuses the request.

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


VECTOR2I subI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) - aB.x ), I( int64_t( aA.y ) - aB.y ) );
}


VECTOR2I addI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) + aB.x ), I( int64_t( aA.y ) + aB.y ) );
}


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


int64_t dotI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return L( __int128( int64_t( aA.x ) * aB.x ) + int64_t( aA.y ) * aB.y );
}


VECTOR2L wideSub( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2L( int64_t( aA.x ) - aB.x, int64_t( aA.y ) - aB.y );
}


// seg.cpp bodies, as the SEG unit's adapter (kimath_src_seg.cpp) guards them -----------------

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


// mutualDistanceSquared; returns false when it does, and the two signed distances otherwise.
bool guardMutual( const SEG& aThis, const SEG& aSeg, int64_t& aD1, int64_t& aD2 )
{
    SEG            a( aThis );
    SEG            b( aSeg );
    const VECTOR2I la = subI( a.A, a.B );
    const VECTOR2I lb = subI( b.A, b.B );

    if( la.SquaredEuclideanNorm() < lb.SquaredEuclideanNorm() )
        std::swap( a, b );

    const int64_t p = int64_t( a.A.y ) - a.B.y;
    const int64_t q = int64_t( a.B.x ) - a.A.x;
    const int64_t r = L( __int128( L( __int128( -p ) * a.A.x ) ) - L( __int128( q ) * a.A.y ) );
    const int64_t l = L( __int128( L( __int128( p ) * p ) ) + L( __int128( q ) * q ) );

    if( l == 0 )
        return false;

    const auto det = [&]( const VECTOR2I& aPoint )
    {
        const int64_t x = L( __int128( p ) * aPoint.x );
        const int64_t y = L( __int128( q ) * aPoint.y );
        return L( __int128( L( __int128( x ) + y ) ) + r );
    };
    const int64_t det1 = det( b.A );
    const int64_t det2 = det( b.B );
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


// circle.cpp bodies, as the CIRCLE unit's adapter (kimath_src_circle.cpp) guards them ---------

void guardResize( const VECTOR2I& aVec, int aLength )
{
    if( aVec.x == 0 && aVec.y == 0 )
        return;
    if( aVec.x == INT_MIN || aVec.y == INT_MIN )
        undefined();
    if( std::abs( aVec.x ) == std::abs( aVec.y ) && aLength == INT_MIN )
        undefined();
}


void guardLineProject( const SEG& aSeg, const VECTOR2I& aP )
{
    const VECTOR2I d = subI( aSeg.B, aSeg.A );
    const int64_t  lSquared = dotI( d, d );

    if( lSquared == 0 )
        return;

    const int64_t t = dotI( d, subI( aP, aSeg.A ) );
    L( __int128( aSeg.A.x ) + rescale( t, int64_t( d.x ), lSquared ) );
    L( __int128( aSeg.A.y ) + rescale( t, int64_t( d.y ), lSquared ) );
}


void guardIntersectLine( const CIRCLE& aCircle, const SEG& aLine )
{
    guardLineProject( aLine, aCircle.Center );
    const VECTOR2I m = aLine.LineProject( aCircle.Center );
    const int64_t  omDist = ( VECTOR2L( m ) - aCircle.Center ).EuclideanNorm();

    if( omDist > ( (int64_t) aCircle.Radius + SHAPE::MIN_PRECISION_IU ) )
        return;
    if( omDist >= ( (int64_t) aCircle.Radius - SHAPE::MIN_PRECISION_IU ) )
        return;

    const int64_t radiusSquared = (int64_t) aCircle.Radius * (int64_t) aCircle.Radius;
    const int     mTo1dist = sqrt( radiusSquared - omDist * omDist );

    const VECTOR2I dir = subI( aLine.B, aLine.A );
    guardResize( dir, mTo1dist );
    const VECTOR2I mTo1vec = dir.Resize( mTo1dist );
    addI( mTo1vec, m );
    addI( -mTo1vec, m );
}


void guardIntersectSeg( const CIRCLE& aCircle, const SEG& aSeg )
{
    guardIntersectLine( aCircle, aSeg );
    for( const VECTOR2I& p : aCircle.IntersectLine( aSeg ) )
        guardSquaredDistance( aSeg, p );
}


// RotatePoint( VECTOR2I&, const VECTOR2I&, const EDA_ANGLE& ) (trigo.h, trigo.cpp): ox, oy, the
// negations of the 90/180/270 degree branches, then (after the genuine rotation of copies) both
// int sums with the centre.
void guardRotatePoint( const VECTOR2I& aPoint, const VECTOR2I& aCentre, const EDA_ANGLE& aAngle )
{
    int ox = I( int64_t( aPoint.x ) - aCentre.x );
    int oy = I( int64_t( aPoint.y ) - aCentre.y );

    EDA_ANGLE angle = aAngle;
    angle.Normalize();

    if( ( angle == ANGLE_90 || angle == ANGLE_180 ) && ox == INT_MIN )
        undefined();
    if( ( angle == ANGLE_180 || angle == ANGLE_270 ) && oy == INT_MIN )
        undefined();

    RotatePoint( &ox, &oy, aAngle );
    I( int64_t( ox ) + aCentre.x );
    I( int64_t( oy ) + aCentre.y );
}


// seg.h bodies -----------------------------------------------------------------------------

void guardCanonical( const SEG& aSeg, int64_t& aA, int64_t& aB, int64_t& aC )
{
    aA = int64_t( aSeg.A.y ) - aSeg.B.y;
    aB = int64_t( aSeg.B.x ) - aSeg.A.x;
    aC = L( __int128( L( __int128( -aA ) * aSeg.A.x ) ) - L( __int128( aB ) * aSeg.A.y ) );
}


void guardCollinear( const SEG& aThis, const SEG& aSeg )
{
    int64_t qa, qb, qc;
    guardCanonical( aThis, qa, qb, qc );

    for( const VECTOR2I& p : { aSeg.A, aSeg.B } )
    {
        const int64_t x = L( __int128( p.x ) * qa );
        const int64_t y = L( __int128( p.y ) * qb );
        if( L( __int128( L( __int128( x ) + y ) ) + qc ) == INT64_MIN )
            undefined();
    }
}


// Overlaps, ContainsSeg: the guarded Collinear, then Contains (SquaredDistance) of each tested
// point, taking each short-circuit decision from the genuine call after its guard.
void guardContainsAll( const SEG& aSeg, std::initializer_list<VECTOR2I> aPoints, bool aAll,
                       int aThreshold, bool& aDecided )
{
    aDecided = false;
    for( const VECTOR2I& p : aPoints )
    {
        guardSquaredDistance( aSeg, p );
        const bool inside = aSeg.Contains( p, aThreshold );
        if( aAll ? !inside : inside )
        {
            aDecided = true;
            return;
        }
    }
}


void guardOverlaps( const SEG& aThis, const SEG& aSeg )
{
    bool decided;

    if( aSeg.A == aSeg.B )
    {
        if( aThis.A == aSeg.A || aThis.B == aSeg.A )
            return;
        guardSquaredDistance( aThis, aSeg.A );
        return;
    }

    guardCollinear( aThis, aSeg );

    if( !aThis.Collinear( aSeg ) )
        return;

    guardContainsAll( aThis, { aSeg.A, aSeg.B }, false, 3, decided );
    if( decided )
        return;
    guardContainsAll( aSeg, { aThis.A, aThis.B }, false, 3, decided );
}


void guardApproxOverlaps( const SEG& aThis, const SEG& aSeg, int aThreshold )
{
    bool decided;

    if( aThis.A == aThis.B )
    {
        if( aSeg.A == aThis.A || aSeg.B == aThis.A )
            return;
        guardSquaredDistance( aSeg, aThis.A );
        return;
    }

    guardApproxCollinear( aSeg, aThis, aThreshold );

    if( !aSeg.ApproxCollinear( aThis, aThreshold ) )
        return;

    guardContainsAll( aSeg, { aThis.A, aThis.B }, false, aThreshold, decided );
    if( decided )
        return;
    guardContainsAll( aThis, { aSeg.A, aSeg.B }, false, aThreshold, decided );
}


void guardContainsSeg( const SEG& aThis, const SEG& aSeg )
{
    bool decided;

    if( aSeg.A == aSeg.B )
    {
        guardSquaredDistance( aThis, aSeg.A );
        return;
    }

    guardCollinear( aThis, aSeg );

    if( !aThis.Collinear( aSeg ) )
        return;

    guardContainsAll( aThis, { aSeg.A, aSeg.B }, true, 3, decided );
}


// ---------------------------------------------------------------------------------------------
// Verified domains of out-of-line calls.

bool within( int64_t aValue )
{
    return aValue >= -DOMAIN && aValue <= DOMAIN;
}


bool within( const VECTOR2I& aPoint )
{
    return within( aPoint.x ) && within( aPoint.y );
}


void circleDomain( const SHAPE& aShape )
{
    if( aShape.Type() != SH_CIRCLE )
        outside();
    const auto& circle = static_cast<const SHAPE_CIRCLE&>( aShape );
    if( !within( circle.GetCenter() ) || circle.GetRadius() < 0 || circle.GetRadius() > DOMAIN )
        outside();
}


void compoundDomain( const SHAPE_COMPOUND& aCompound )
{
    for( const SHAPE* shape : aCompound.Shapes() )
    {
        if( !shape )
            outside();
        circleDomain( *shape );
    }
}


void arcDomain( const VECTOR2I& aStart, const VECTOR2I& aMid, const VECTOR2I& aEnd, int aWidth )
{
    if( !within( aStart ) || !within( aMid ) || !within( aEnd ) || !within( aWidth ) )
        outside();

    const __int128 d1x = int64_t( aMid.x ) - aStart.x, d1y = int64_t( aMid.y ) - aStart.y;
    const __int128 d2x = int64_t( aEnd.x ) - aMid.x, d2y = int64_t( aEnd.y ) - aMid.y;
    const __int128 n1 = d1x * d1x + d1y * d1y;
    const __int128 n2 = d2x * d2x + d2y * d2y;
    const __int128 cross = d1x * d2y - d1y * d2x;

    if( aStart == aEnd || n1 == 0 || n2 == 0 || cross * cross * 64 < n1 * n2 )
        outside();
}


// ---------------------------------------------------------------------------------------------
// Shapes.

std::unique_ptr<SHAPE> buildShape( const JSON& aSpec );


JSON state( const SHAPE& aShape )
{
    switch( aShape.Type() )
    {
    case SH_CIRCLE:
    {
        const auto& c = static_cast<const SHAPE_CIRCLE&>( aShape );
        return JSON{ { "type", "SH_CIRCLE" }, { "center", point( c.GetCenter() ) },
                     { "radius", c.GetRadius() } };
    }
    case SH_ARC:
    {
        const auto& a = static_cast<const SHAPE_ARC&>( aShape );
        return JSON{ { "type", "SH_ARC" }, { "start", point( a.GetP0() ) },
                     { "mid", point( a.GetArcMid() ) }, { "end", point( a.GetP1() ) },
                     { "width", a.GetWidth() } };
    }
    case SH_COMPOUND:
    {
        const auto& c = static_cast<const SHAPE_COMPOUND&>( aShape );
        JSON        shapes = JSON::array();
        for( const SHAPE* s : c.Shapes() )
            shapes.push_back( s ? state( *s ) : JSON( nullptr ) );
        return JSON{ { "type", "SH_COMPOUND" }, { "shapes", shapes } };
    }
    case SH_ELLIPSE:
    {
        const auto& e = static_cast<const SHAPE_ELLIPSE&>( aShape );
        return JSON{ { "type", "SH_ELLIPSE" },
                     { "center", point( e.GetCenter() ) },
                     { "major", e.GetMajorRadius() },
                     { "minor", e.GetMinorRadius() },
                     { "rotation", bits( e.GetRotation().AsDegrees() ) },
                     { "start", bits( e.GetStartAngle().AsDegrees() ) },
                     { "end", bits( e.GetEndAngle().AsDegrees() ) },
                     { "is_arc", e.IsArc() } };
    }
    default: throw std::invalid_argument( "unknown shape" );
    }
}


JSON optionalState( const SHAPE* aShape )
{
    return aShape ? state( *aShape ) : JSON( nullptr );
}


void addShape( SHAPE_COMPOUND& aCompound, const JSON& aItem, bool aIsCall )
{
    if( aIsCall )
        checkKeys( aItem, { "call", "shape", "shared" } );
    else
        checkKeys( aItem, { "shape", "shared" } );

    std::unique_ptr<SHAPE> shape = buildShape( aItem.at( "shape" ) );
    const bool             shared = readBool( aItem, "shared" );

    if( shared )
        aCompound.AddShape( std::shared_ptr<SHAPE>( shape.release() ) );
    else
        aCompound.AddShape( shape.release() );
}


std::unique_ptr<SHAPE> buildShape( const JSON& aSpec )
{
    if( !aSpec.is_object() )
        throw std::invalid_argument( "expected object" );
    const std::string kind = readText( aSpec, "kind" );

    if( kind == "compound" )
    {
        checkKeys( aSpec, { "kind", "add" } );
        auto compound = std::make_unique<SHAPE_COMPOUND>();
        for( const JSON& item : readArray( aSpec, "add" ) )
            addShape( *compound, item, false );
        return compound;
    }

    const std::string ctor = readText( aSpec, "ctor" );

    if( kind == "circle" )
    {
        if( ctor == "default" )
        {
            checkKeys( aSpec, { "kind", "ctor" } );
            return std::make_unique<SHAPE_CIRCLE>();
        }
        checkKeys( aSpec, { "kind", "ctor", "center", "radius" } );
        const VECTOR2I center = readPoint( aSpec, "center" );
        const int      radius = readInt( aSpec, "radius" );
        if( ctor == "center_radius" )
            return std::make_unique<SHAPE_CIRCLE>( center, radius );
        if( ctor == "circle" )
            return std::make_unique<SHAPE_CIRCLE>( CIRCLE( center, radius ) );
        if( ctor == "copy" )
        {
            const SHAPE_CIRCLE source( center, radius );
            return std::make_unique<SHAPE_CIRCLE>( source );
        }
    }
    else if( kind == "arc" )
    {
        if( ctor == "default" )
        {
            checkKeys( aSpec, { "kind", "ctor" } );
            return std::make_unique<SHAPE_ARC>();
        }
        if( ctor == "three_point" )
        {
            checkKeys( aSpec, { "kind", "ctor", "start", "mid", "end", "width" } );
            const VECTOR2I start = readPoint( aSpec, "start" );
            const VECTOR2I mid = readPoint( aSpec, "mid" );
            const VECTOR2I end = readPoint( aSpec, "end" );
            const int      width = readInt( aSpec, "width" );
            arcDomain( start, mid, end, width );
            return std::make_unique<SHAPE_ARC>( start, mid, end, width );
        }
    }
    else if( kind == "ellipse" )
    {
        if( ctor == "default" )
        {
            checkKeys( aSpec, { "kind", "ctor" } );
            return std::make_unique<SHAPE_ELLIPSE>();
        }
        if( ctor == "radii" || ctor == "arc_radii" )
        {
            if( ctor == "radii" )
                checkKeys( aSpec, { "kind", "ctor", "center", "major", "minor", "rotation" } );
            else
                checkKeys( aSpec, { "kind", "ctor", "center", "major", "minor", "rotation", "start",
                                    "end" } );
            const VECTOR2I  center = readPoint( aSpec, "center" );
            const int       major = readInt( aSpec, "major" );
            const int       minor = readInt( aSpec, "minor" );
            const EDA_ANGLE rotation( readBits( aSpec, "rotation" ), DEGREES_T );
            if( ctor == "radii" )
                return std::make_unique<SHAPE_ELLIPSE>( center, major, minor, rotation );
            const EDA_ANGLE start( readBits( aSpec, "start" ), DEGREES_T );
            const EDA_ANGLE end( readBits( aSpec, "end" ), DEGREES_T );
            return std::make_unique<SHAPE_ELLIPSE>( center, major, minor, rotation, start, end );
        }
    }
    else
    {
        throw std::invalid_argument( "unknown shape" );
    }
    throw std::invalid_argument( "unknown constructor" );
}


bool isDefaultArc( const SHAPE_ARC& aArc )
{
    return aArc.GetP0() == VECTOR2I( 0, 0 ) && aArc.GetArcMid() == VECTOR2I( 0, 0 )
           && aArc.GetP1() == VECTOR2I( 0, 0 );
}


JSON boxJson( const BOX2I& aBox )
{
    return JSON{ { "pos", point( aBox.GetPosition() ) },
                 { "size", JSON::array( { aBox.GetSize().x, aBox.GetSize().y } ) } };
}


// The guard of a non-virtual-dispatch-free out-of-line call on a non-circle shape.
void outOfLineDomain( const SHAPE& aShape )
{
    if( aShape.Type() == SH_COMPOUND )
        compoundDomain( static_cast<const SHAPE_COMPOUND&>( aShape ) );
    else
        outside();
}


void guardCircleBBox( const SHAPE_CIRCLE& aCircle, int aClearance )
{
    const int r = I( int64_t( aCircle.GetRadius() ) + aClearance );
    subI( aCircle.GetCenter(), VECTOR2I( r, r ) );
    I( int64_t( r ) * 2 );
}


void guardCircleCollide( const SHAPE_CIRCLE& aShape, const SEG& aSeg, int aClearance, bool aActual,
                         bool aLocation )
{
    const CIRCLE circle = aShape.GetCircle();
    const int    minDist = I( int64_t( aClearance ) + circle.Radius );
    guardNearestPoint( aSeg, circle.Center );
    const VECTOR2I pn = aSeg.NearestPoint( circle.Center );
    const int64_t  distSq = subI( pn, circle.Center ).SquaredEuclideanNorm();

    if( !( distSq == 0 || distSq < SEG::Square( minDist ) ) )
        return;

    if( aLocation )
        guardIntersectSeg( circle, aSeg );

    if( aActual )
    {
        const double root = std::trunc( sqrt( distSq ) );
        if( !( root < 2147483648.0 ) )
            undefined();
        I( int64_t( root ) - circle.Radius );
    }
}


JSON collide( SHAPE& aShape, const JSON& aCall, bool aPoint )
{
    if( aPoint )
        checkKeys( aCall, { "call", "point" }, { "clearance", "actual", "location" } );
    else
        checkKeys( aCall, { "call", "seg" }, { "clearance", "actual", "location" } );

    const SEG seg = aPoint ? SEG( readPoint( aCall, "point" ), readPoint( aCall, "point" ) )
                           : readSeg( aCall, "seg" );
    const bool hasClearance = aCall.contains( "clearance" );
    const bool hasActual = aCall.contains( "actual" );
    const bool hasLocation = aCall.contains( "location" );
    const int  clearance = hasClearance ? readInt( aCall, "clearance" ) : 0;
    int        actual = hasActual ? readInt( aCall, "actual" ) : 0;
    VECTOR2I   location = hasLocation ? readPoint( aCall, "location" ) : VECTOR2I();

    if( aShape.Type() == SH_CIRCLE )
    {
        guardCircleCollide( static_cast<const SHAPE_CIRCLE&>( aShape ), seg, clearance, hasActual,
                            hasLocation );
    }
    else
    {
        outOfLineDomain( aShape );
        if( !within( clearance ) || !within( seg.A ) || !within( seg.B ) )
            outside();
    }

    bool returned;
    if( !hasClearance && !hasActual && !hasLocation )
        returned = aPoint ? aShape.Collide( seg.A ) : aShape.Collide( seg );
    else if( aPoint )
        returned = aShape.Collide( seg.A, clearance, hasActual ? &actual : nullptr,
                                   hasLocation ? &location : nullptr );
    else
        returned = aShape.Collide( seg, clearance, hasActual ? &actual : nullptr,
                                   hasLocation ? &location : nullptr );

    return JSON{ { "returned", returned },
                 { "actual", hasActual ? JSON( actual ) : JSON( nullptr ) },
                 { "location", hasLocation ? point( location ) : JSON( nullptr ) } };
}


JSON applyCall( SHAPE& aShape, const JSON& aCall )
{
    const std::string name = readText( aCall, "call" );
    const SHAPE_TYPE  type = aShape.Type();
    auto*             circle = type == SH_CIRCLE ? static_cast<SHAPE_CIRCLE*>( &aShape ) : nullptr;
    auto*             arc = type == SH_ARC ? static_cast<SHAPE_ARC*>( &aShape ) : nullptr;
    auto* compound = type == SH_COMPOUND ? static_cast<SHAPE_COMPOUND*>( &aShape ) : nullptr;
    auto* ellipse = type == SH_ELLIPSE ? static_cast<SHAPE_ELLIPSE*>( &aShape ) : nullptr;
    const auto plain = [&]() { checkKeys( aCall, { "call" } ); };

    // SHAPE_BASE and SHAPE members, through the base class.
    if( name == "type" )
        return plain(), JSON( static_cast<int>( aShape.Type() ) );
    if( name == "type_name" )
        return plain(), JSON( aShape.TypeName().ToStdString() );
    if( name == "is_null" )
        return plain(), JSON( aShape.IsNull() );
    if( name == "has_indexable_subshapes" )
        return plain(), JSON( aShape.HasIndexableSubshapes() );
    if( name == "indexable_subshape_count" )
        return plain(), JSON( aShape.GetIndexableSubshapeCount() );
    if( name == "indexable_subshapes" )
    {
        checkKeys( aCall, { "call", "prefill" } );
        const int prefill = readInt( aCall, "prefill" );
        if( prefill < 0 || prefill > 8 )
            throw std::invalid_argument( "integer out of range" );
        std::vector<const SHAPE*> subshapes( prefill, nullptr );
        aShape.GetIndexableSubshapes( subshapes );
        JSON out = JSON::array();
        for( const SHAPE* s : subshapes )
            out.push_back( optionalState( s ) );
        return out;
    }
    if( name == "get_start" )
        return plain(), point( aShape.GetStart() );
    if( name == "get_end" )
        return plain(), point( aShape.GetEnd() );
    if( name == "get_width" )
        return plain(), JSON( aShape.GetWidth() );
    if( name == "set_width" )
    {
        checkKeys( aCall, { "call", "width" } );
        aShape.SetWidth( readInt( aCall, "width" ) );
        return nullptr;
    }
    if( name == "is_solid" )
    {
        plain();
        if( compound )
            compoundDomain( *compound );
        else if( !circle && !arc && !ellipse )
            outside();
        return aShape.IsSolid();
    }
    if( name == "clone" )
    {
        plain();
        std::unique_ptr<SHAPE> copy( aShape.Clone() );
        return optionalState( copy.get() );
    }
    if( name == "bbox" || name == "centre" )
    {
        if( name == "bbox" )
            checkKeys( aCall, { "call" }, { "clearance" } );
        else
            plain();
        const bool hasClearance = aCall.contains( "clearance" );
        const int  clearance = hasClearance ? readInt( aCall, "clearance" ) : 0;

        if( circle )
        {
            guardCircleBBox( *circle, clearance );
        }
        else if( arc )
        {
            if( !isDefaultArc( *arc ) || !within( arc->GetWidth() ) || !within( clearance ) )
                outside();
        }
        else
        {
            outOfLineDomain( aShape );
            if( !within( clearance ) )
                outside();
        }

        if( name == "centre" )
            return point( aShape.Centre() );
        return boxJson( hasClearance ? aShape.BBox( clearance ) : aShape.BBox() );
    }
    if( name == "collide_seg" || name == "collide_point" )
        return collide( aShape, aCall, name == "collide_point" );
    if( name == "collide_shape" )
    {
        checkKeys( aCall, { "call", "other" }, { "clearance", "actual", "location" } );
        std::unique_ptr<SHAPE> other = buildShape( aCall.at( "other" ) );
        const bool hasClearance = aCall.contains( "clearance" );
        const bool hasActual = aCall.contains( "actual" );
        const bool hasLocation = aCall.contains( "location" );
        const int  clearance = hasClearance ? readInt( aCall, "clearance" ) : 0;
        int        actual = hasActual ? readInt( aCall, "actual" ) : 0;
        VECTOR2I   location = hasLocation ? readPoint( aCall, "location" ) : VECTOR2I();

        if( circle )
            circleDomain( *circle );
        else
            outOfLineDomain( aShape );
        if( other->Type() != SH_CIRCLE || clearance < 0 || clearance > DOMAIN )
            outside();
        circleDomain( *other );

        const bool returned = aShape.Collide( other.get(), clearance, hasActual ? &actual : nullptr,
                                              hasLocation ? &location : nullptr );
        return JSON{ { "returned", returned },
                     { "actual", hasActual ? JSON( actual ) : JSON( nullptr ) },
                     { "location", hasLocation ? point( location ) : JSON( nullptr ) } };
    }
    if( name == "move" )
    {
        checkKeys( aCall, { "call", "vector" } );
        const VECTOR2I vector = readPoint( aCall, "vector" );
        if( circle )
            addI( circle->GetCenter(), vector );
        else
        {
            outOfLineDomain( aShape );
            if( !within( vector ) )
                outside();
        }
        aShape.Move( vector );
        return nullptr;
    }
    if( name == "rotate" )
    {
        checkKeys( aCall, { "call", "angle" }, { "center" } );
        const EDA_ANGLE angle( readBits( aCall, "angle" ), DEGREES_T );
        const bool      hasCenter = aCall.contains( "center" );
        const VECTOR2I  center = hasCenter ? readPoint( aCall, "center" ) : VECTOR2I( 0, 0 );
        if( circle )
            guardRotatePoint( circle->GetCenter(), center, angle );
        else
        {
            outOfLineDomain( aShape );
            if( !within( center ) )
                outside();
        }
        if( hasCenter )
            aShape.Rotate( angle, center );
        else
            aShape.Rotate( angle );
        return nullptr;
    }

    if( circle )
    {
        if( name == "get_radius" )
            return plain(), JSON( circle->GetRadius() );
        if( name == "get_center" )
            return plain(), point( circle->GetCenter() );
        if( name == "get_circle" )
        {
            plain();
            const CIRCLE c = circle->GetCircle();
            return JSON{ { "center", point( c.Center ) }, { "radius", c.Radius } };
        }
        if( name == "set_radius" )
        {
            checkKeys( aCall, { "call", "radius" } );
            circle->SetRadius( readInt( aCall, "radius" ) );
            return nullptr;
        }
        if( name == "set_center" )
        {
            checkKeys( aCall, { "call", "center" } );
            circle->SetCenter( readPoint( aCall, "center" ) );
            return nullptr;
        }
        if( name == "assign" )
        {
            checkKeys( aCall, { "call", "from" } );
            std::unique_ptr<SHAPE> from = buildShape( aCall.at( "from" ) );
            if( from->Type() != SH_CIRCLE )
                throw std::invalid_argument( "unknown shape" );
            SHAPE_CIRCLE& result = ( *circle = static_cast<const SHAPE_CIRCLE&>( *from ) );
            return JSON( &result == circle );
        }
    }
    if( arc )
    {
        if( name == "get_p0" )
            return plain(), point( arc->GetP0() );
        if( name == "get_p1" )
            return plain(), point( arc->GetP1() );
        if( name == "get_arc_mid" )
            return plain(), point( arc->GetArcMid() );
        if( name == "get_chord" )
            return plain(), segment( arc->GetChord() );
        if( name == "is_ccw" || name == "is_clockwise" )
        {
            plain();
            const VECTOR2L mid = arc->GetArcMid();
            const VECTOR2L v1 = arc->GetP1() - mid;
            const VECTOR2L v2 = arc->GetP0() - mid;
            crossL( v1, v2 );
            return name == "is_ccw" ? arc->IsCCW() : arc->IsClockwise();
        }
        if( name == "equals" )
        {
            checkKeys( aCall, { "call", "other" }, { "other_width" } );
            std::unique_ptr<SHAPE> other = buildShape( aCall.at( "other" ) );
            if( other->Type() != SH_ARC )
                throw std::invalid_argument( "unknown shape" );
            auto& otherArc = static_cast<SHAPE_ARC&>( *other );
            if( aCall.contains( "other_width" ) )
                otherArc.SetWidth( readInt( aCall, "other_width" ) );
            return JSON{ { "this", *arc == otherArc }, { "other", otherArc == *arc } };
        }
    }
    if( compound )
    {
        if( name == "add_shape" )
        {
            addShape( *compound, aCall, true );
            return nullptr;
        }
        if( name == "empty" )
            return plain(), JSON( compound->Empty() );
        if( name == "size" )
            return plain(), JSON( compound->Size() );
        if( name == "unique_subshape" )
            return plain(), optionalState( compound->UniqueSubshape() );
        if( name == "shapes" )
        {
            plain();
            JSON out = JSON::array();
            for( const SHAPE* s : compound->Shapes() )
                out.push_back( optionalState( s ) );
            return out;
        }
    }
    if( ellipse )
    {
        if( name == "get_center" )
            return plain(), point( ellipse->GetCenter() );
        if( name == "get_major_radius" )
            return plain(), JSON( ellipse->GetMajorRadius() );
        if( name == "get_minor_radius" )
            return plain(), JSON( ellipse->GetMinorRadius() );
        if( name == "get_rotation" )
            return plain(), bits( ellipse->GetRotation().AsDegrees() );
        if( name == "get_start_angle" )
            return plain(), bits( ellipse->GetStartAngle().AsDegrees() );
        if( name == "get_end_angle" )
            return plain(), bits( ellipse->GetEndAngle().AsDegrees() );
        if( name == "is_arc" )
            return plain(), JSON( ellipse->IsArc() );
        if( name == "equals" )
        {
            checkKeys( aCall, { "call", "other" } );
            std::unique_ptr<SHAPE> other = buildShape( aCall.at( "other" ) );
            if( other->Type() != SH_ELLIPSE )
                throw std::invalid_argument( "unknown shape" );
            const auto& otherEllipse = static_cast<const SHAPE_ELLIPSE&>( *other );
            return JSON{ { "this", *ellipse == otherEllipse }, { "other", otherEllipse == *ellipse } };
        }
        if( name == "assign" )
        {
            checkKeys( aCall, { "call", "from" } );
            std::unique_ptr<SHAPE> from = buildShape( aCall.at( "from" ) );
            if( from->Type() != SH_ELLIPSE )
                throw std::invalid_argument( "unknown shape" );
            SHAPE_ELLIPSE& result = ( *ellipse = static_cast<const SHAPE_ELLIPSE&>( *from ) );
            return JSON( &result == ellipse );
        }
    }
    throw std::invalid_argument( "unknown call" );
}


// ---------------------------------------------------------------------------------------------

JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "shape" )
    {
        checkKeys( aInput, { "op", "shape", "calls" } );
        std::unique_ptr<SHAPE> shape = buildShape( aInput.at( "shape" ) );
        JSON                   replies = JSON::array();
        for( const JSON& call : readArray( aInput, "calls" ) )
        {
            if( !call.is_object() )
                throw std::invalid_argument( "expected object" );
            replies.push_back( applyCall( *shape, call ) );
        }
        return JSON{ { "calls", replies }, { "state", state( *shape ) } };
    }
    if( op == "constants" )
    {
        checkKeys( aInput, { "op" } );
        return JSON{ { "min_precision_iu", SHAPE::MIN_PRECISION_IU },
                     { "arc_high_def", ARC_HIGH_DEF },
                     { "default_accuracy_for_pcb", SHAPE_ARC::DefaultAccuracyForPCB() } };
    }
    if( op == "shape_type_as_string" )
    {
        checkKeys( aInput, { "op", "value" } );
        const int value = readInt( aInput, "value" );
        if( value < SH_RECT || value > SH_ELLIPSE )
            throw std::invalid_argument( "integer out of range" );
        return SHAPE_TYPE_asString( static_cast<SHAPE_TYPE>( value ) ).ToStdString();
    }
    if( op == "seg_new" )
    {
        const std::string ctor = readText( aInput, "ctor" );
        if( ctor == "default" )
        {
            checkKeys( aInput, { "op", "ctor" } );
            return segment( SEG() );
        }
        if( ctor == "coords" )
        {
            checkKeys( aInput, { "op", "ctor", "coords" } );
            const JSON& c = readArray( aInput, "coords" );
            if( c.size() != 4 )
                throw std::invalid_argument( "expected array: coords" );
            return segment( SEG( readInt( c[0] ), readInt( c[1] ), readInt( c[2] ), readInt( c[3] ) ) );
        }
        if( ctor == "points" )
        {
            checkKeys( aInput, { "op", "ctor", "seg" } );
            const SEG s = readSeg( aInput, "seg" );
            return segment( SEG( s.A, s.B ) );
        }
        if( ctor == "indexed" )
        {
            checkKeys( aInput, { "op", "ctor", "seg", "index" } );
            const SEG s = readSeg( aInput, "seg" );
            return segment( SEG( s.A, s.B, readInt( aInput, "index" ) ) );
        }
        throw std::invalid_argument( "unknown constructor" );
    }
    if( op == "seg_copy" )
    {
        checkKeys( aInput, { "op", "seg", "index" }, { "assign_to" } );
        const SEG s0 = readSeg( aInput, "seg" );
        const SEG source( s0.A, s0.B, readInt( aInput, "index" ) );
        if( !aInput.contains( "assign_to" ) )
        {
            const SEG copy( source );
            return segment( copy );
        }
        const JSON& target = aInput.at( "assign_to" );
        checkKeys( target, { "seg", "index" } );
        const SEG t0 = readSeg( target, "seg" );
        SEG assigned( t0.A, t0.B, readInt( target, "index" ) );
        assigned = source;
        return segment( assigned );
    }
    if( op == "seg_reverse" )
    {
        checkKeys( aInput, { "op", "seg", "index" } );
        const SEG s0 = readSeg( aInput, "seg" );
        SEG       s( s0.A, s0.B, readInt( aInput, "index" ) );
        s.Reverse();
        return segment( s );
    }
    if( op == "seg_square" )
    {
        checkKeys( aInput, { "op", "value" } );
        return SEG::Square( readInt( aInput, "value" ) );
    }

    if( op == "seg_canonical" || op == "seg_length" || op == "seg_squared_length"
        || op == "seg_center" || op == "seg_stream" || op == "seg_reversed" )
    {
        checkKeys( aInput, { "op", "seg" } );
        const SEG s = readSeg( aInput, "seg" );

        if( op == "seg_canonical" )
        {
            int64_t a, b, c;
            guardCanonical( s, a, b, c );
            SEG::ecoord qa = 0, qb = 0, qc = 0;
            s.CanonicalCoefs( qa, qb, qc );
            return JSON::array( { qa, qb, qc } );
        }
        if( op == "seg_length" || op == "seg_squared_length" )
        {
            subI( s.A, s.B );
            return op == "seg_length" ? JSON( s.Length() ) : JSON( s.SquaredLength() );
        }
        if( op == "seg_center" )
        {
            addI( s.A, subI( s.B, s.A ) / 2 );
            return point( s.Center() );
        }
        if( op == "seg_stream" )
        {
            std::ostringstream stream;
            stream << s;
            return stream.str();
        }
        return segment( s.Reversed() );
    }
    if( op == "seg_side" || op == "seg_tcoef" )
    {
        checkKeys( aInput, { "op", "seg", "point" } );
        const SEG      s = readSeg( aInput, "seg" );
        const VECTOR2I p = readPoint( aInput, "point" );
        const VECTOR2I d = subI( s.B, s.A );
        const VECTOR2I q = subI( p, s.A );
        if( op == "seg_side" )
            return s.Side( p );
        dotI( d, q );
        return s.TCoef( p );
    }
    if( op == "seg_compare" || op == "seg_collinear" || op == "seg_overlaps"
        || op == "seg_contains_seg" || op == "seg_intersect_lines" || op == "seg_approx_overlaps" )
    {
        if( op == "seg_approx_overlaps" )
            checkKeys( aInput, { "op", "seg", "other" }, { "threshold" } );
        else
            checkKeys( aInput, { "op", "seg", "other" } );
        const SEG s = readSeg( aInput, "seg" );
        const SEG o = readSeg( aInput, "other" );

        if( op == "seg_compare" )
            return JSON{ { "eq", s == o }, { "ne", s != o }, { "lt", s < o }, { "gt", o < s } };
        if( op == "seg_collinear" )
        {
            guardCollinear( s, o );
            return s.Collinear( o );
        }
        if( op == "seg_overlaps" )
        {
            guardOverlaps( s, o );
            return s.Overlaps( o );
        }
        if( op == "seg_contains_seg" )
        {
            guardContainsSeg( s, o );
            return s.Contains( o );
        }
        if( op == "seg_intersect_lines" )
        {
            guardIntersects( s, o, false, true, true );
            const OPT_VECTOR2I p = s.IntersectLines( o );
            return p ? point( *p ) : JSON( nullptr );
        }
        if( aInput.contains( "threshold" ) )
        {
            const int threshold = readInt( aInput, "threshold" );
            guardApproxOverlaps( s, o, threshold );
            return s.ApproxOverlaps( o, threshold );
        }
        guardApproxOverlaps( s, o, 3 );
        return s.ApproxOverlaps( o );
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterShapeHeadersP003( REGISTRY& aRegistry )
{
    aRegistry.Add( P003_UNIT, "geometry/seg.h, shape*.h", run );
}
} // namespace oracle
