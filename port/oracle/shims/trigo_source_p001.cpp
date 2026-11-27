// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned trigo.cpp (libs/kimath/src/trigo.cpp), compiled unchanged into the kimath
// archive.
//
// The adapter decodes 32-bit integer points and IEEE bit strings, calls the pinned trigo.cpp
// functions and encodes every observable value exactly; it never computes a reply itself. Before
// each call a guard walks the pinned integer evaluation in C++ order and refuses the request when
// it would reach undefined behaviour (signed overflow, negation or std::abs of the minimum).
// Where a later operation depends on a computed value, the guard takes it from the genuine
// function, called only after its own operations have been checked.
//
// The three-point VECTOR2D CalcArcCenter passes each coordinate triple to std::minmax, whose
// operator< must induce a strict weak ordering on those values. A NaN with two non-equivalent
// values violates that library precondition, so the adapter refuses such a triple
// (breaksStrictWeakOrder); a NaN with equivalent values, or only NaNs, is defined and accepted.
// Every other double input is accepted (the arithmetic is defined).
#include "registry.h"
#include <geometry/eda_angle.h>
#include <geometry/seg.h>
#include <math/util.h>
#include <math/vector2d.h>
#include <trigo.h>
#include <algorithm>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace
{
using oracle::JSON;
constexpr const char* TRIGO_UNIT = "libs.kimath.src.trigo.application.ui.p001";

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined trigo operation" );
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


bool readBool( const JSON& aValue )
{
    if( !aValue.is_boolean() )
        throw std::invalid_argument( "expected boolean" );
    return aValue.get<bool>();
}


int readInt( const JSON& aValue )
{
    if( !aValue.is_number_integer() || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
        || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<int>();
}


double decodeBits( const JSON& aValue )
{
    if( !aValue.is_string() )
        throw std::invalid_argument( "expected hex" );
    const std::string text = aValue.get<std::string>();
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( "invalid hex" );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


VECTOR2I readPointI( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2I( readInt( aValue[0] ), readInt( aValue[1] ) );
}


VECTOR2D readPointD( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    return VECTOR2D( decodeBits( aValue[0] ), decodeBits( aValue[1] ) );
}


// An angle is given as the exact bits of its value in degrees.
EDA_ANGLE readAngle( const JSON& aValue )
{
    return EDA_ANGLE( decodeBits( aValue ), DEGREES_T );
}


JSON pointI( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON pointD( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
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


// VECTOR2I - VECTOR2I: int components.
VECTOR2I subI( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2I( I( int64_t( aA.x ) - aB.x ), I( int64_t( aA.y ) - aB.y ) );
}


VECTOR2L wideSub( const VECTOR2I& aA, const VECTOR2I& aB )
{
    return VECTOR2L( int64_t( aA.x ) - aB.x, int64_t( aA.y ) - aB.y );
}


// VECTOR2<int64_t>::Dot: both products, then the sum.
int64_t dotL( const VECTOR2L& aA, const VECTOR2L& aB )
{
    const int64_t x = L( __int128( aA.x ) * aB.x );
    const int64_t y = L( __int128( aA.y ) * aB.y );
    return L( __int128( x ) + y );
}


// IsPointOnSegment: both int differences, the (always defined) cross product, then the dot
// product and squared norm sums of int64 products, each of which can reach 2^63.
void guardPointOnSegment( const VECTOR2I& aStart, const VECTOR2I& aEnd, const VECTOR2I& aPoint )
{
    const VECTOR2I seg = subI( aEnd, aStart );
    const VECTOR2I point = subI( aPoint, aStart );

    if( (long long) seg.x * point.y - (long long) seg.y * point.x )
        return;

    L( __int128( (long long) seg.x * point.x ) + (long long) seg.y * point.y );
    L( __int128( (long long) point.x * point.x ) + (long long) point.y * point.y );
}


// SegmentIntersectsSegment: the int64 deltas always fit; den, num_a and num_b are differences of
// int64 products. With an output point, each KiROUND (which saturates and logs) is added to an
// int. Then the three negations when den < 0.
void guardSegmentIntersects( const VECTOR2I& aP1L1, const VECTOR2I& aP2L1, const VECTOR2I& aP1L2,
                             const VECTOR2I& aP2L2, bool aPoint )
{
    const int64_t dX_a = int64_t{ aP2L1.x } - aP1L1.x;
    const int64_t dY_a = int64_t{ aP2L1.y } - aP1L1.y;
    const int64_t dX_b = int64_t{ aP2L2.x } - aP1L2.x;
    const int64_t dY_b = int64_t{ aP2L2.y } - aP1L2.y;
    const int64_t dX_ab = int64_t{ aP1L2.x } - aP1L1.x;
    const int64_t dY_ab = int64_t{ aP1L2.y } - aP1L1.y;

    const int64_t den = L( __int128( L( __int128( dY_a ) * dX_b ) ) - L( __int128( dY_b ) * dX_a ) );

    if( den == 0 )
        return;

    const int64_t num_a = L( __int128( L( __int128( dY_ab ) * dX_b ) ) - L( __int128( dY_b ) * dX_ab ) );
    const int64_t num_b = L( __int128( L( __int128( dY_ab ) * dX_a ) ) - L( __int128( dY_a ) * dX_ab ) );

    if( aPoint )
    {
        // KiROUND is called quietly here so the guard does not log; the genuine call logs.
        I( int64_t( aP1L1.x ) + KiROUND( dX_a * (double) num_a / (double) den, true ) );
        I( int64_t( aP1L1.y ) + KiROUND( dY_a * (double) num_b / (double) den, true ) );
    }

    if( den < 0 && ( den == INT64_MIN || num_a == INT64_MIN || num_b == INT64_MIN ) )
        undefined();
}


// SEG::SquaredDistance( const VECTOR2I& ) (seg.cpp), as the SEG unit's adapter guards it:
// int64 Dot products and sums; SquaredEuclideanNorm saturates and the rest is wide or floating.
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
}


// TestSegmentHit: aStart - aRefPoint, the four short-circuited bounding box differences, the
// std::abs of the easy cases, then SquaredDistance and aDist + 1 (unsequenced operands of <,
// both always evaluated).
void guardSegmentHit( const VECTOR2I& aRef, const VECTOR2I& aStart, const VECTOR2I& aEnd, int aDist )
{
    const VECTOR2I delta = subI( aStart, aRef );
    const int      xmin = std::min( aStart.x, aEnd.x );
    const int      xmax = std::max( aStart.x, aEnd.x );
    const int      ymin = std::min( aStart.y, aEnd.y );
    const int      ymax = std::max( aStart.y, aEnd.y );

    if( I( int64_t( ymin ) - aRef.y ) > aDist || I( int64_t( aRef.y ) - ymax ) > aDist )
        return;

    if( I( int64_t( xmin ) - aRef.x ) > aDist || I( int64_t( aRef.x ) - xmax ) > aDist )
        return;

    if( aStart.x == aEnd.x && aRef.y > ymin && aRef.y < ymax )
    {
        if( delta.x == INT_MIN )
            undefined();
        return;
    }

    if( aStart.y == aEnd.y && aRef.x > xmin && aRef.x < xmax )
    {
        if( delta.y == INT_MIN )
            undefined();
        return;
    }

    guardSquaredDistance( SEG( aStart, aEnd ), aRef );
    I( int64_t( aDist ) + 1 );
}


// RotatePoint( int*, int*, const EDA_ANGLE& ): the negations of the 90/180/270 degree branches.
// The general branch is double arithmetic and KiROUND, which saturates.
void guardRotate( int aX, int aY, const EDA_ANGLE& aAngle )
{
    EDA_ANGLE angle = aAngle;
    angle.Normalize();

    if( ( angle == ANGLE_90 || angle == ANGLE_180 ) && aX == INT_MIN )
        undefined();
    if( ( angle == ANGLE_180 || angle == ANGLE_270 ) && aY == INT_MIN )
        undefined();
}


// RotatePoint( int*, int*, int cx, int cy, const EDA_ANGLE& ): both differences, the rotation
// (the genuine function, called after its own guard) and both sums.
void guardRotateAbout( int aX, int aY, int aCx, int aCy, const EDA_ANGLE& aAngle )
{
    int ox = I( int64_t( aX ) - aCx );
    int oy = I( int64_t( aY ) - aCy );

    guardRotate( ox, oy, aAngle );
    RotatePoint( &ox, &oy, aAngle );
    I( int64_t( ox ) + aCx );
    I( int64_t( oy ) + aCy );
}


// CalcArcMid: both differences with aCenter, then RotatePoint( newMid = aStart, aCenter,
// midPointRotAngle ) (trigo.h, then trigo.cpp). The angle is computed with the same genuine
// EDA_ANGLE expressions as CalcArcMid.
void guardCalcArcMid( const VECTOR2I& aStart, const VECTOR2I& aEnd, const VECTOR2I& aCenter,
                      bool aMinArcAngle )
{
    const VECTOR2I startVector = subI( aStart, aCenter );
    const VECTOR2I endVector = subI( aEnd, aCenter );

    const EDA_ANGLE startAngle( startVector );
    const EDA_ANGLE endAngle( endVector );
    EDA_ANGLE       midPointRotAngle = ( startAngle - endAngle ).Normalize180() / 2;

    if( !aMinArcAngle )
        midPointRotAngle += ANGLE_180;

    guardRotateAbout( aStart.x, aStart.y, aCenter.x, aCenter.y, midPointRotAngle );
}


// std::minmax( { a, b, c } ) requires operator< to induce a strict weak ordering on the given
// values ([alg.sorting.general]). For doubles only the transitivity of equivalence can fail: a
// NaN is equivalent to every value, so the call is undefined exactly when a NaN is given with
// two non-NaN values that are not equivalent to each other (+0 and -0 are equivalent).
bool breaksStrictWeakOrder( double aA, double aB, double aC )
{
    bool   nan = false;
    bool   distinct = false;
    bool   haveFirst = false;
    double first = 0.0;

    for( double value : { aA, aB, aC } )
    {
        if( std::isnan( value ) )
        {
            nan = true;
        }
        else if( !haveFirst )
        {
            first = value;
            haveFirst = true;
        }
        else if( value < first || first < value )
        {
            distinct = true;
        }
    }

    return nan && distinct;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "point_on_segment" )
    {
        checkKeys( aInput, { "op", "start", "end", "point" } );
        const VECTOR2I start = readPointI( aInput.at( "start" ) );
        const VECTOR2I end = readPointI( aInput.at( "end" ) );
        const VECTOR2I point = readPointI( aInput.at( "point" ) );
        guardPointOnSegment( start, end, point );
        return JSON{ { "value", IsPointOnSegment( start, end, point ) } };
    }
    if( op == "segment_intersects" )
    {
        // "point" is null for the default nullptr, otherwise the output point's initial value.
        checkKeys( aInput, { "op", "a1", "a2", "b1", "b2", "point" } );
        const VECTOR2I a1 = readPointI( aInput.at( "a1" ) );
        const VECTOR2I a2 = readPointI( aInput.at( "a2" ) );
        const VECTOR2I b1 = readPointI( aInput.at( "b1" ) );
        const VECTOR2I b2 = readPointI( aInput.at( "b2" ) );

        if( aInput.at( "point" ).is_null() )
        {
            guardSegmentIntersects( a1, a2, b1, b2, false );
            return JSON{ { "value", SegmentIntersectsSegment( a1, a2, b1, b2 ) }, { "point", nullptr } };
        }

        VECTOR2I point = readPointI( aInput.at( "point" ) );
        guardSegmentIntersects( a1, a2, b1, b2, true );
        const bool value = SegmentIntersectsSegment( a1, a2, b1, b2, &point );
        return JSON{ { "value", value }, { "point", pointI( point ) } };
    }
    if( op == "segment_hit" )
    {
        checkKeys( aInput, { "op", "ref", "start", "end", "dist" } );
        const VECTOR2I ref = readPointI( aInput.at( "ref" ) );
        const VECTOR2I start = readPointI( aInput.at( "start" ) );
        const VECTOR2I end = readPointI( aInput.at( "end" ) );
        const int      dist = readInt( aInput.at( "dist" ) );
        guardSegmentHit( ref, start, end, dist );
        return JSON{ { "value", TestSegmentHit( ref, start, end, dist ) } };
    }
    if( op == "arc_mid" )
    {
        // Without "min_arc_angle" the call uses the default argument.
        const bool defaulted = !aInput.contains( "min_arc_angle" );

        if( defaulted )
            checkKeys( aInput, { "op", "start", "end", "center" } );
        else
            checkKeys( aInput, { "op", "start", "end", "center", "min_arc_angle" } );

        const VECTOR2I start = readPointI( aInput.at( "start" ) );
        const VECTOR2I end = readPointI( aInput.at( "end" ) );
        const VECTOR2I center = readPointI( aInput.at( "center" ) );
        const bool     minArc = defaulted ? true : readBool( aInput.at( "min_arc_angle" ) );
        guardCalcArcMid( start, end, center, minArc );
        const VECTOR2I mid = defaulted ? CalcArcMid( start, end, center )
                                       : CalcArcMid( start, end, center, minArc );
        return JSON{ { "point", pointI( mid ) } };
    }
    if( op == "rotate_i" )
    {
        checkKeys( aInput, { "op", "point", "angle" } );
        VECTOR2I        p = readPointI( aInput.at( "point" ) );
        const EDA_ANGLE angle = readAngle( aInput.at( "angle" ) );
        guardRotate( p.x, p.y, angle );
        RotatePoint( &p.x, &p.y, angle );
        return JSON{ { "point", pointI( p ) } };
    }
    if( op == "rotate_i_about" )
    {
        checkKeys( aInput, { "op", "point", "center", "angle" } );
        VECTOR2I        p = readPointI( aInput.at( "point" ) );
        const VECTOR2I  c = readPointI( aInput.at( "center" ) );
        const EDA_ANGLE angle = readAngle( aInput.at( "angle" ) );
        guardRotateAbout( p.x, p.y, c.x, c.y, angle );
        RotatePoint( &p.x, &p.y, c.x, c.y, angle );
        return JSON{ { "point", pointI( p ) } };
    }
    if( op == "rotate_d" )
    {
        // Double arithmetic only: no undefined operation.
        checkKeys( aInput, { "op", "point", "angle" } );
        VECTOR2D p = readPointD( aInput.at( "point" ) );
        RotatePoint( &p.x, &p.y, readAngle( aInput.at( "angle" ) ) );
        return JSON{ { "point", pointD( p ) } };
    }
    if( op == "rotate_d_about" )
    {
        checkKeys( aInput, { "op", "point", "center", "angle" } );
        VECTOR2D       p = readPointD( aInput.at( "point" ) );
        const VECTOR2D c = readPointD( aInput.at( "center" ) );
        RotatePoint( &p.x, &p.y, c.x, c.y, readAngle( aInput.at( "angle" ) ) );
        return JSON{ { "point", pointD( p ) } };
    }
    if( op == "arc_center_angle" )
    {
        checkKeys( aInput, { "op", "start", "end", "angle" } );
        const VECTOR2D start = readPointD( aInput.at( "start" ) );
        const VECTOR2D end = readPointD( aInput.at( "end" ) );
        return JSON{ { "center", pointD( CalcArcCenter( start, end, readAngle( aInput.at( "angle" ) ) ) ) } };
    }
    if( op == "arc_center_d" )
    {
        checkKeys( aInput, { "op", "start", "mid", "end" } );
        const VECTOR2D start = readPointD( aInput.at( "start" ) );
        const VECTOR2D mid = readPointD( aInput.at( "mid" ) );
        const VECTOR2D end = readPointD( aInput.at( "end" ) );

        // degenerateArcCenter calls std::minmax on the x then the y coordinates before anything
        // else. snapArcCenter's radii minmax is reached only for a finite centre, which needs
        // finite inputs, so its values are never NaN.
        if( breaksStrictWeakOrder( start.x, mid.x, end.x )
            || breaksStrictWeakOrder( start.y, mid.y, end.y ) )
            undefined();

        return JSON{ { "center", pointD( CalcArcCenter( start, mid, end ) ) } };
    }
    if( op == "arc_center_i" || op == "is_straight_arc" )
    {
        // 64-bit deltas and 128-bit numerators: no undefined operation.
        checkKeys( aInput, { "op", "start", "mid", "end" } );
        const VECTOR2I start = readPointI( aInput.at( "start" ) );
        const VECTOR2I mid = readPointI( aInput.at( "mid" ) );
        const VECTOR2I end = readPointI( aInput.at( "end" ) );

        if( op == "arc_center_i" )
            return JSON{ { "center", pointI( CalcArcCenter( start, mid, end ) ) } };

        return JSON{ { "value", IsStraightArc( start, mid, end ) } };
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterTrigoSource( REGISTRY& aRegistry )
{
    aRegistry.Add( TRIGO_UNIT, "trigo.cpp", run );
}
} // namespace oracle
