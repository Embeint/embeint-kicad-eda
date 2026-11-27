// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned CIRCLE (libs/kimath/src/geometry/circle.cpp).
//
// The adapter decodes 32-bit integer points and IEEE bit strings, calls the pinned CIRCLE
// bodies and encodes every observable value exactly; it never computes a reply itself. Before
// each call a guard walks the pinned integer evaluation and refuses the request when it would
// reach undefined behaviour (signed overflow, std::abs of INT_MIN). Where the pinned control
// flow or a later operand depends on a computed value, the guard takes it from the genuine
// header or SEG function, called only after that function's own guard has passed.
//
// ConstructFromTanTanPt is guarded operation by operation too (guardTanTanPt), through its SEG,
// CalcArcMid and RotatePoint callees, so a request is refused exactly when its pinned
// evaluation reaches undefined behaviour.
#include "registry.h"
#include <geometry/circle.h>
#include <geometry/eda_angle.h>
#include <geometry/seg.h>
#include <geometry/shape.h>
#include <math/util.h>
#include <trigo.h>
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
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* CIRCLE_UNIT = "libs.kimath.src.geometry.circle.application.ui.p001";

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined circle operation" );
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


SEG readSeg( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected segment" );
    return SEG( readPointI( aValue[0] ), readPointI( aValue[1] ) );
}


CIRCLE readCircle( const JSON& aValue )
{
    checkKeys( aValue, { "center", "radius" } );
    return CIRCLE( readPointI( aValue.at( "center" ) ), readInt( aValue.at( "radius" ) ) );
}


JSON pointI( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON pointD( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
}


JSON points( const std::vector<VECTOR2I>& aValues )
{
    JSON out = JSON::array();
    for( const VECTOR2I& p : aValues )
        out.push_back( pointI( p ) );
    return out;
}


JSON state( const CIRCLE& aCircle )
{
    return JSON{ { "center", pointI( aCircle.Center ) }, { "radius", aCircle.Radius } };
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


int64_t dotL( const VECTOR2L& aA, const VECTOR2L& aB )
{
    const int64_t x = L( __int128( aA.x ) * aB.x );
    const int64_t y = L( __int128( aA.y ) * aB.y );
    return L( __int128( x ) + y );
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


// VECTOR2<int64_t>::Cross: both products, then the difference.
int64_t crossL( const VECTOR2L& aA, const VECTOR2L& aB )
{
    const int64_t x = L( __int128( aA.x ) * aB.y );
    const int64_t y = L( __int128( aA.y ) * aB.x );
    return L( __int128( x ) - y );
}


// VECTOR2<int>::Resize: std::abs( x ) and std::abs( y ) are evaluated first (undefined for
// INT_MIN), and std::abs( aNewLength ) on the diagonal branch. rescale, sqrt, KiROUND (which
// saturates and logs), the negation of its non-negative result and * sign() are defined.
void guardResize( const VECTOR2I& aVec, int aLength )
{
    if( aVec.x == 0 && aVec.y == 0 )
        return;
    if( aVec.x == INT_MIN || aVec.y == INT_MIN )
        undefined();
    if( std::abs( aVec.x ) == std::abs( aVec.y ) && aLength == INT_MIN )
        undefined();
}


// SEG::LineProject (seg.cpp): VECTOR2I d = B - A, two Dots and the final int64 sums.
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


// SEG::SquaredDistance( const VECTOR2I& ), reached through SEG::Contains.
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


// NearestPoint / FurthestPoint( const VECTOR2I& ): the subtraction, Resize and the final sum.
void guardNearOrFar( const CIRCLE& aCircle, const VECTOR2I& aP, bool aFurthest )
{
    VECTOR2I vec = aFurthest ? subI( aCircle.Center, aP ) : subI( aP, aCircle.Center );

    if( vec.x == 0 && vec.y == 0 )
        vec.x = 1;

    guardResize( vec, aCircle.Radius );
    addI( vec.Resize( aCircle.Radius ), aCircle.Center );
}


// Intersect( const CIRCLE& ): int64 equation (3), the remainder, then each rotated solution's
// int sum with Center. The rotation is the genuine trigo.h RotatePoint.
void guardIntersectCircle( const CIRCLE& aThis, const CIRCLE& aOther )
{
    const VECTOR2L vecCtoC = VECTOR2L( aOther.Center ) - aThis.Center;
    const int64_t  d = vecCtoC.EuclideanNorm();
    const int64_t  r1 = aThis.Radius;
    const int64_t  r2 = aOther.Radius;

    if( d > ( r1 + r2 ) || ( d < ( std::abs( r1 - r2 ) ) ) )
        return;

    if( d == 0 )
        return;

    const int64_t numerator = L( __int128( L( __int128( L( __int128( d ) * d ) ) + L( __int128( r1 ) * r1 ) ) )
                                 - L( __int128( r2 ) * r2 ) );
    const int64_t x = numerator / ( int64_t( 2 ) * d );
    const int64_t r1sqMinusXsq = L( __int128( L( __int128( r1 ) * r1 ) ) - L( __int128( x ) * x ) );

    if( r1sqMinusXsq < 0 )
        return;

    const int64_t y = KiROUND( sqrt( r1sqMinusXsq ) );
    const EDA_ANGLE rotAngle( vecCtoC );

    VECTOR2I solution1( x, y );
    RotatePoint( solution1, -rotAngle );
    addI( solution1, aThis.Center );

    if( y != 0 )
    {
        VECTOR2I solution2( x, -y );
        RotatePoint( solution2, -rotAngle );
        addI( solution2, aThis.Center );
    }
}


// IntersectLine( const SEG& ): LineProject, then (past the tangent tests) B - A, its Resize
// and both int sums with m. -mTo1vec cannot overflow: |Resize| <= |mTo1dist| < 2^31.
void guardIntersectLine( const CIRCLE& aCircle, const SEG& aLine )
{
    guardLineProject( aLine, aCircle.Center );
    const VECTOR2I m = aLine.LineProject( aCircle.Center );
    const int64_t  omDist = ( VECTOR2L( m ) - aCircle.Center ).EuclideanNorm();

    if( omDist > ( (int64_t) aCircle.Radius + SHAPE::MIN_PRECISION_IU ) )
        return;
    if( omDist >= ( (int64_t) aCircle.Radius - SHAPE::MIN_PRECISION_IU ) )
        return;

    // omDist < Radius - 4 <= 2^31, so both squares and their difference fit, and the root
    // converts to int without overflow.
    const int64_t radiusSquared = (int64_t) aCircle.Radius * (int64_t) aCircle.Radius;
    const int     mTo1dist = sqrt( radiusSquared - omDist * omDist );

    const VECTOR2I dir = subI( aLine.B, aLine.A );
    guardResize( dir, mTo1dist );
    const VECTOR2I mTo1vec = dir.Resize( mTo1dist );
    addI( mTo1vec, m );
    addI( -mTo1vec, m );
}


// Intersect( const SEG& ): IntersectLine, then SEG::Contains (SquaredDistance) of each point.
void guardIntersectSeg( const CIRCLE& aCircle, const SEG& aSeg )
{
    guardIntersectLine( aCircle, aSeg );
    for( const VECTOR2I& p : aCircle.IntersectLine( aSeg ) )
        guardSquaredDistance( aSeg, p );
}


// ---------------------------------------------------------------------------------------------
// The callees of ConstructFromTanTanPt. The SEG guards follow the pinned seg.cpp bodies in the
// same way as the SEG unit's adapter (kimath_src_seg.cpp).

// SEG::LineDistance: the canonical line p, q, r, l and det; rescale, isqrt, sgn( det ) * dist
// (|dist| <= 3037000499) and the int conversion (C++20 modular) are defined.
void guardLineDistance( const SEG& aSeg, const VECTOR2I& aP )
{
    const int64_t p = int64_t( aSeg.A.y ) - aSeg.B.y;
    const int64_t q = int64_t( aSeg.B.x ) - aSeg.A.x;
    const int64_t r = L( __int128( L( __int128( -p ) * aSeg.A.x ) ) - L( __int128( q ) * aSeg.A.y ) );
    L( __int128( L( __int128( p ) * p ) ) + L( __int128( q ) * q ) );
    const int64_t x = L( __int128( p ) * aP.x );
    const int64_t y = L( __int128( q ) * aP.y );
    L( __int128( L( __int128( x ) + y ) ) + r );
}


// SEG::ApproxParallel( aSeg, 1 ): Square( 1 ), then mutualDistanceSquared (SquaredLength's
// A - B in int, the canonical line, both det, their rescale and sgn( det ) * dsq), then
// std::abs( d1_sq - d2_sq ).
void guardApproxParallel( const SEG& aThis, const SEG& aSeg )
{
    SEG a( aThis );
    SEG b( aSeg );

    if( subI( a.A, a.B ).SquaredEuclideanNorm() < subI( b.A, b.B ).SquaredEuclideanNorm() )
        std::swap( a, b );

    const int64_t p = int64_t( a.A.y ) - a.B.y;
    const int64_t q = int64_t( a.B.x ) - a.A.x;
    const int64_t r = L( __int128( L( __int128( -p ) * a.A.x ) ) - L( __int128( q ) * a.A.y ) );
    const int64_t l = L( __int128( L( __int128( p ) * p ) ) + L( __int128( q ) * q ) );

    if( l == 0 )
        return;

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

    const int64_t d1 = int64_t( ( det1 > 0 ) - ( det1 < 0 ) ) * dsq1;
    const int64_t d2 = int64_t( ( det2 > 0 ) - ( det2 < 0 ) ) * dsq2;

    if( L( __int128( d1 ) - d2 ) == INT64_MIN )
        undefined();
}


// SEG::IntersectLines = Intersect( aSeg, false, true ), the seg.cpp intersects with aLines and a
// point: int64 Cross products, the collinear midpoint ( A + aSeg.A ) in int (its halving is a
// defined KiROUND), and the rescaled int64 sum, whose range is checked before the narrowing.
void guardIntersectLines( const SEG& aThis, const SEG& aSeg )
{
    const VECTOR2L dir1 = wideSub( aThis.B, aThis.A );
    const VECTOR2L dir2 = wideSub( aSeg.B, aSeg.A );
    const VECTOR2L offset = wideSub( aSeg.A, aThis.A );
    const int64_t  determinant = crossL( dir2, dir1 );

    if( determinant == 0 )
    {
        if( crossL( dir1, offset ) != 0 )
            return;

        if( aSeg.A != aSeg.B && aThis.A != aThis.B )
            addI( aThis.A, aSeg.A );

        return;
    }

    crossL( dir2, offset );
    const int64_t param1 = crossL( dir1, offset );
    L( __int128( aSeg.A.x ) + rescale( param1, dir2.x, determinant ) );
    L( __int128( aSeg.A.y ) + rescale( param1, dir2.y, determinant ) );
}


// SEG::PerpendicularSeg: B - A, Perpendicular's -y, and the sum with aP.
void guardPerpendicular( const SEG& aSeg, const VECTOR2I& aP )
{
    const VECTOR2I slope = subI( aSeg.B, aSeg.A );

    if( slope.y == INT_MIN )
        undefined();

    addI( VECTOR2I( -slope.y, slope.x ), aP );
}


// SEG::ParallelSeg: B - A and the sum with aP.
void guardParallel( const SEG& aSeg, const VECTOR2I& aP )
{
    addI( subI( aSeg.B, aSeg.A ), aP );
}


// SEG::Center (seg.h): A + ( B - A ) / 2; the division rounds through a defined KiROUND.
void guardCenter( const SEG& aSeg )
{
    addI( aSeg.A, subI( aSeg.B, aSeg.A ) / 2 );
}


// CalcArcMid( aStart, aEnd, aCenter, true ) (trigo.cpp): both differences with aCenter, then
// RotatePoint( newMid = aStart, aCenter, midPointRotAngle ) (trigo.h, trigo.cpp): ox, oy, the
// rotation (negations on the 90/180/270 degree branches, otherwise KiROUND of double
// expressions, which saturates) and both int sums with the centre. The angle is computed with
// the same genuine EDA_ANGLE expressions as CalcArcMid; the rotation is the genuine trigo.cpp
// RotatePoint( int*, int*, const EDA_ANGLE& ), called only after its negations are checked.
void guardCalcArcMid( const VECTOR2I& aStart, const VECTOR2I& aEnd, const VECTOR2I& aCenter )
{
    const VECTOR2I startVector = subI( aStart, aCenter );
    const VECTOR2I endVector = subI( aEnd, aCenter );

    const EDA_ANGLE startAngle( startVector );
    const EDA_ANGLE endAngle( endVector );
    const EDA_ANGLE midPointRotAngle = ( startAngle - endAngle ).Normalize180() / 2;

    int ox = I( int64_t( aStart.x ) - aCenter.x );
    int oy = I( int64_t( aStart.y ) - aCenter.y );

    EDA_ANGLE angle = midPointRotAngle;
    angle.Normalize();

    if( ( angle == ANGLE_90 || angle == ANGLE_180 ) && ox == INT_MIN )
        undefined();
    if( ( angle == ANGLE_180 || angle == ANGLE_270 ) && oy == INT_MIN )
        undefined();

    RotatePoint( &ox, &oy, midPointRotAngle );
    I( int64_t( ox ) + aCenter.x );
    I( int64_t( oy ) + aCenter.y );
}


// The furthestFromIntersect and closestToIntersect lambdas: both differences in int, then the
// (defined) norms. Returns the lambda's choice.
VECTOR2I furthestFrom( const VECTOR2I& aIntersect, const VECTOR2I& aPt1, const VECTOR2I& aPt2 )
{
    return subI( aPt1, aIntersect ).EuclideanNorm() > subI( aPt2, aIntersect ).EuclideanNorm() ? aPt1
                                                                                                : aPt2;
}


VECTOR2I closestTo( const VECTOR2I& aIntersect, const VECTOR2I& aPt1, const VECTOR2I& aPt2 )
{
    return subI( aPt1, aIntersect ).EuclideanNorm() <= subI( aPt2, aIntersect ).EuclideanNorm()
                   ? aPt1
                   : aPt2;
}


// ConstructFromTanTanPt: every integer operation of the pinned body and its callees, in
// evaluation order, following the pinned control flow. Each value a later operation or branch
// depends on comes from the genuine function, called after its own guard; a failed
// wxCHECK_MSG ends the evaluation as it does in C++. Only the members' final state differs
// from the genuine call, which runs afterwards on the caller's circle.
void guardTanTanPt( const SEG& aLineA, const SEG& aLineB, const VECTOR2I& aP )
{
    guardApproxParallel( aLineA, aLineB );

    if( aLineA.ApproxParallel( aLineB ) )
    {
        guardLineProject( aLineB, aLineA.A );
        const SEG perpendicularAtoB( aLineA.A, aLineB.LineProject( aLineA.A ) );
        guardCenter( perpendicularAtoB );
        const VECTOR2I midPt = perpendicularAtoB.Center();
        const int      radius = subI( midPt, aLineA.A ).EuclideanNorm();

        guardParallel( aLineA, midPt );
        const SEG anglebisector = aLineA.ParallelSeg( midPt );

        // The construction circle: Center = aP with the new Radius.
        const CIRCLE construction( aP, radius );
        guardIntersectLine( construction, anglebisector );
        const std::vector<VECTOR2I> possibleCenters = construction.IntersectLine( anglebisector );

        if( possibleCenters.empty() )
            return; // wxCHECK_MSG at circle.cpp:102

        closestTo( aLineA.A, possibleCenters.front(), possibleCenters.back() );
        return;
    }

    guardIntersectLines( aLineA, aLineB );
    const OPT_VECTOR2I intersectCalc = aLineA.IntersectLines( aLineB );

    if( !intersectCalc )
        return; // wxCHECK_MSG at circle.cpp:115

    const VECTOR2I intersectPoint = *intersectCalc;

    if( aP == intersectPoint )
        return;

    const VECTOR2I lineApt = furthestFrom( intersectPoint, aLineA.A, aLineA.B );
    const VECTOR2I lineBpt = furthestFrom( intersectPoint, aLineB.A, aLineB.B );
    guardCalcArcMid( lineApt, lineBpt, intersectPoint );
    const VECTOR2I bisectorPt = CalcArcMid( lineApt, lineBpt, intersectPoint, true );
    const SEG      anglebisector( intersectPoint, bisectorPt );

    CIRCLE hSolution;
    guardLineProject( anglebisector, aP );
    hSolution.Center = anglebisector.LineProject( aP );
    guardLineDistance( aLineA, hSolution.Center );
    hSolution.Radius = aLineA.LineDistance( hSolution.Center );

    const SEG throughaP( intersectPoint, aP );
    guardIntersectLine( hSolution, throughaP );
    const std::vector<VECTOR2I> hProjections = hSolution.IntersectLine( throughaP );

    if( hProjections.empty() )
        return; // wxCHECK_MSG at circle.cpp:142

    const VECTOR2I hSelected = closestTo( intersectPoint, hProjections.front(), hProjections.back() );

    guardLineProject( aLineA, hSolution.Center );
    const VECTOR2I hTanLineA = aLineA.LineProject( hSolution.Center );
    guardLineProject( aLineB, hSolution.Center );
    const VECTOR2I hTanLineB = aLineB.LineProject( hSolution.Center );

    const bool useA =
            subI( hTanLineA, aP ).SquaredEuclideanNorm() > subI( hTanLineB, aP ).SquaredEuclideanNorm();
    const SEG& line = useA ? aLineA : aLineB;
    const SEG  hT( useA ? hTanLineA : hTanLineB, hSelected );

    guardParallel( hT, aP );
    const SEG parallel = hT.ParallelSeg( aP );
    guardIntersectLines( parallel, line );
    const OPT_VECTOR2I actTan = parallel.IntersectLines( line );

    if( !actTan )
        return; // wxCHECK_MSG at circle.cpp:157 or 172

    guardPerpendicular( line, *actTan );
    const SEG perpendicularToTan = line.PerpendicularSeg( *actTan );
    guardIntersectLines( perpendicularToTan, anglebisector );
    const OPT_VECTOR2I actCenter = perpendicularToTan.IntersectLines( anglebisector );

    if( !actCenter )
        return; // wxCHECK_MSG at circle.cpp:162 or 177

    guardLineDistance( line, *actCenter );
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "default" )
    {
        checkKeys( aInput, { "op" } );
        return JSON{ { "circle", state( CIRCLE() ) } };
    }
    if( op == "new" )
    {
        checkKeys( aInput, { "op", "center", "radius" } );
        const VECTOR2I center = readPointI( aInput.at( "center" ) );
        const int      radius = readInt( aInput.at( "radius" ) );
        return JSON{ { "circle", state( CIRCLE( center, radius ) ) } };
    }
    if( op == "copy" )
    {
        checkKeys( aInput, { "op", "circle" } );
        const CIRCLE source = readCircle( aInput.at( "circle" ) );
        const CIRCLE copy( source );
        return JSON{ { "circle", state( copy ) }, { "equal", copy == source } };
    }
    if( op == "equals" )
    {
        checkKeys( aInput, { "op", "circle", "other" } );
        const CIRCLE a = readCircle( aInput.at( "circle" ) );
        const CIRCLE b = readCircle( aInput.at( "other" ) );
        return JSON{ { "equal", a == b }, { "reverse", b == a } };
    }
    if( op == "contains" )
    {
        checkKeys( aInput, { "op", "circle", "point", "overload" } );
        CIRCLE            circle = readCircle( aInput.at( "circle" ) );
        const VECTOR2I    p = readPointI( aInput.at( "point" ) );
        const std::string overload = readText( aInput, "overload" );
        bool              value;

        if( overload == "const" )
        {
            // A const object selects the on-circumference overload.
            const CIRCLE& constCircle = circle;
            value = constCircle.Contains( p );
        }
        else if( overload == "non_const" )
        {
            // A non-const lvalue selects the strictly-inside overload.
            subI( p, circle.Center );
            value = circle.Contains( p );
        }
        else
        {
            throw std::invalid_argument( "unknown overload" );
        }
        return JSON{ { "value", value }, { "circle", state( circle ) } };
    }
    if( op == "nearest_i32" || op == "furthest_i32" )
    {
        checkKeys( aInput, { "op", "circle", "point" } );
        const CIRCLE   circle = readCircle( aInput.at( "circle" ) );
        const VECTOR2I p = readPointI( aInput.at( "point" ) );
        const bool     furthest = op == "furthest_i32";
        guardNearOrFar( circle, p, furthest );
        return JSON{ { "point", pointI( furthest ? circle.FurthestPoint( p ) : circle.NearestPoint( p ) ) } };
    }
    if( op == "nearest_f64" || op == "furthest_f64" )
    {
        // Double arithmetic only: no undefined operation.
        checkKeys( aInput, { "op", "circle", "point" } );
        const CIRCLE   circle = readCircle( aInput.at( "circle" ) );
        const VECTOR2D p = readPointD( aInput.at( "point" ) );
        return JSON{ { "point",
                       pointD( op == "furthest_f64" ? circle.FurthestPoint( p ) : circle.NearestPoint( p ) ) } };
    }
    if( op == "intersect_circle" )
    {
        checkKeys( aInput, { "op", "circle", "other" } );
        const CIRCLE a = readCircle( aInput.at( "circle" ) );
        const CIRCLE b = readCircle( aInput.at( "other" ) );
        guardIntersectCircle( a, b );
        return JSON{ { "points", points( a.Intersect( b ) ) } };
    }
    if( op == "intersect_seg" || op == "intersect_line" )
    {
        checkKeys( aInput, { "op", "circle", "seg" } );
        const CIRCLE circle = readCircle( aInput.at( "circle" ) );
        const SEG    seg = readSeg( aInput.at( "seg" ) );

        if( op == "intersect_seg" )
        {
            guardIntersectSeg( circle, seg );
            return JSON{ { "points", points( circle.Intersect( seg ) ) } };
        }
        guardIntersectLine( circle, seg );
        return JSON{ { "points", points( circle.IntersectLine( seg ) ) } };
    }
    if( op == "tan_tan_pt" )
    {
        checkKeys( aInput, { "op", "circle", "line_a", "line_b", "point" } );
        CIRCLE         circle = readCircle( aInput.at( "circle" ) );
        const SEG      lineA = readSeg( aInput.at( "line_a" ) );
        const SEG      lineB = readSeg( aInput.at( "line_b" ) );
        const VECTOR2I p = readPointI( aInput.at( "point" ) );
        guardTanTanPt( lineA, lineB, p );
        CIRCLE& result = circle.ConstructFromTanTanPt( lineA, lineB, p );
        return JSON{ { "circle", state( circle ) }, { "returns_self", &result == &circle } };
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterCircle( REGISTRY& aRegistry )
{
    aRegistry.Add( CIRCLE_UNIT, "geometry/circle.cpp", run );
}
} // namespace oracle
