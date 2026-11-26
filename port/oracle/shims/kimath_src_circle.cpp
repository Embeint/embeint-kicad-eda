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
// ConstructFromTanTanPt is accepted only inside a stated sufficient domain (see
// guardTanTanDomain); it is not guarded operation by operation.
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

// Lines and the point of ConstructFromTanTanPt stay within this magnitude (see guardTanTanDomain).
constexpr int64_t TAN_TAN_BOUND = int64_t( 1 ) << 16;


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


// ConstructFromTanTanPt is not guarded operation by operation. It is accepted only for
// non-degenerate lines and a point within TAN_TAN_BOUND, where the pinned ApproxParallel is
// either true or the lines meet at an angle whose sine is at least 1/8. Every intermediate then
// stays below about 2^24 (the intersection and tangent points are within a small multiple of
// the input extent), far from any int or int64 limit. Other requests are refused as outside
// the audited domain, which is a harness limit, not a statement about the C++.
void guardTanTanDomain( const SEG& aLineA, const SEG& aLineB, const VECTOR2I& aP )
{
    const auto bounded = []( const VECTOR2I& v )
    {
        return std::llabs( v.x ) <= TAN_TAN_BOUND && std::llabs( v.y ) <= TAN_TAN_BOUND;
    };

    if( !bounded( aLineA.A ) || !bounded( aLineA.B ) || !bounded( aLineB.A ) || !bounded( aLineB.B )
        || !bounded( aP ) || aLineA.A == aLineA.B || aLineB.A == aLineB.B )
    {
        throw std::invalid_argument( "tan-tan request outside the audited domain" );
    }

    if( aLineA.ApproxParallel( aLineB ) )
        return;

    const VECTOR2L dA = wideSub( aLineA.B, aLineA.A );
    const VECTOR2L dB = wideSub( aLineB.B, aLineB.A );
    const __int128 cross = __int128( dA.x ) * dB.y - __int128( dA.y ) * dB.x;
    const __int128 normA = __int128( dA.x ) * dA.x + __int128( dA.y ) * dA.y;
    const __int128 normB = __int128( dB.x ) * dB.x + __int128( dB.y ) * dB.y;

    if( cross * cross * 64 < normA * normB )
        throw std::invalid_argument( "tan-tan request outside the audited domain" );
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
        guardTanTanDomain( lineA, lineB, p );
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
