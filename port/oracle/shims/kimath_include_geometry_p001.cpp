// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned kimath geometry headers of libs.kimath.include.geometry.application.pure.p001:
// direction45.h, geometry_predicates.h, fracture_edge_index_utils.h, geometry_utils.h (the
// header templates), half_line.h, line.h, arc_chord_params.h, arc_construction.h, circle.h and
// the type-only headers (approximation.h, corner_strategy.h, corner_operations.h,
// intersection.h, nearest.h, oval.h), compiled unchanged into this adapter. Out-of-line callees
// (ARC_CHORD_PARAMS::Compute, the CIRCLE and SHAPE_ARC constructors) are the genuine bodies in
// the kimath archive.
//
// The adapter decodes 32-bit integer points, 64-bit unsigned sizes and IEEE bit strings, calls
// the pinned inline functions and encodes every observable value exactly; it never computes a
// reply itself. DIRECTION_45's private m_dir and m_90deg are read through explicit template
// instantiation of member pointers (access checking does not apply there), never by changing the
// header.
//
// Before each call a guard walks the pinned evaluation and refuses the request as "undefined
// geometry header operation" when it would reach undefined behaviour: the negation of INT_MIN or
// an int difference that overflows in the DIRECTION_45 constructors, an OppositeMap index outside
// 0..8, a Mask shift count outside 0..31, an int overflow or std::abs( INT_MIN ) in Angle, Right
// and Left, std::abs( INT_MIN ) in the int snapping templates, a GetClampedCoords padding above
// INT_MAX (std::clamp's lower bound would exceed its upper bound, [alg.clamp]) and a floating
// GetClampedCoords coordinate that is NaN while the bounds differ (std::clamp's operator< must
// then induce a strict weak ordering on the three values, which a NaN breaks, [alg.clamp]/2).
// Every other body is defined for every input.
//
// The three-point SHAPE_ARC constructor (shape_arc.cpp) is the genuine body; guardArcConstructor
// replays its evaluation and refuses, with the same message, exactly the arcs whose construction
// is undefined. Every other three-point arc is constructed and compared.
#include "registry.h"
#include <geometry/approximation.h>
#include <geometry/arc_chord_params.h>
#include <geometry/arc_construction.h>
#include <geometry/circle.h>
#include <geometry/corner_operations.h>
#include <geometry/corner_strategy.h>
#include <geometry/direction45.h>
#include <geometry/eda_angle.h>
#include <geometry/fracture_edge_index_utils.h>
#include <geometry/geometry_predicates.h>
#include <geometry/geometry_utils.h>
#include <geometry/half_line.h>
#include <geometry/intersection.h>
#include <geometry/line.h>
#include <geometry/nearest.h>
#include <geometry/oval.h>
#include <geometry/seg.h>
#include <geometry/shape_arc.h>
#include <math/util.h>
#include <math/vector2d.h>
#include <trigo.h>
#include <algorithm>
#include <bit>
#include <climits>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* GEOMETRY_P001_UNIT = "libs.kimath.include.geometry.application.pure.p001";

[[noreturn]] void undefined()
{
    throw std::invalid_argument( "undefined geometry header operation" );
}


// DIRECTION_45's private members.
template <typename TAG, typename TAG::TYPE MEMBER>
struct ACCESS
{
    friend typename TAG::TYPE Get( TAG ) { return MEMBER; }
};

struct DIR_TAG
{
    using TYPE = DIRECTION_45::Directions DIRECTION_45::*;
    friend TYPE Get( DIR_TAG );
};

struct DEG90_TAG
{
    using TYPE = bool DIRECTION_45::*;
    friend TYPE Get( DEG90_TAG );
};

template struct ACCESS<DIR_TAG, &DIRECTION_45::m_dir>;
template struct ACCESS<DEG90_TAG, &DIRECTION_45::m_90deg>;


int dirOf( const DIRECTION_45& aDir )
{
    return static_cast<int>( aDir.*Get( DIR_TAG() ) );
}


bool deg90Of( const DIRECTION_45& aDir )
{
    return aDir.*Get( DEG90_TAG() );
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


// Like checkKeys, with the optional fields allowed to be absent.
void checkKeysOptional( const JSON& aInput, std::initializer_list<const char*> aFields,
                        std::initializer_list<const char*> aOptional )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    size_t present = aFields.size();
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : aOptional )
        if( aInput.contains( field ) )
            present++;
    if( aInput.size() != present )
        throw std::invalid_argument( "unexpected or missing field" );
}


const JSON& field( const JSON& aInput, const char* aName )
{
    if( !aInput.is_object() || !aInput.contains( aName ) )
        throw std::invalid_argument( std::string( "missing: " ) + aName );
    return aInput.at( aName );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.is_object() || !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
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


uint64_t readU64( const JSON& aValue )
{
    if( !aValue.is_number_unsigned() )
        throw std::invalid_argument( "integer out of range" );
    return aValue.get<uint64_t>();
}


uint32_t readU32( const JSON& aValue )
{
    if( !aValue.is_number_unsigned() || aValue.get<uint64_t>() > UINT32_MAX )
        throw std::invalid_argument( "integer out of range" );
    return static_cast<uint32_t>( aValue.get<uint64_t>() );
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


// Every multi-field read below is sequenced (x before y, fields in request order), so a
// malformed request gets the same message as from the Rust shim.
VECTOR2I readPointI( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    const int x = readInt( aValue[0] );
    const int y = readInt( aValue[1] );
    return VECTOR2I( x, y );
}


VECTOR2D readPointD( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected point" );
    const double x = decodeBits( aValue[0] );
    const double y = decodeBits( aValue[1] );
    return VECTOR2D( x, y );
}


// The optional a90 argument of the DIRECTION_45 constructors (default false).
bool readA90( const JSON& aSpec )
{
    return aSpec.contains( "a90" ) ? readBool( aSpec.at( "a90" ) ) : false;
}


JSON pointI( const VECTOR2I& aValue )
{
    return JSON::array( { aValue.x, aValue.y } );
}


JSON pointD( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
}


JSON segState( const SEG& aSeg )
{
    return JSON{ { "a", pointI( aSeg.A ) }, { "b", pointI( aSeg.B ) }, { "index", aSeg.Index() } };
}


// The template's type T (or in_type / ret_type).
bool readFloating( const JSON& aInput, const char* aName )
{
    const std::string type = readText( aInput, aName );

    if( type == "int" )
        return false;
    if( type == "double" )
        return true;
    throw std::invalid_argument( "unknown type" );
}


// ---------------------------------------------------------------------------------------------
// DIRECTION_45.

JSON dirState( const DIRECTION_45& aDir )
{
    return JSON{ { "dir", dirOf( aDir ) }, { "deg90", deg90Of( aDir ) } };
}


// The constructor's vector with y negated: undefined for y == INT_MIN.
void guardConstructVector( const VECTOR2I& aVec )
{
    if( aVec.y == INT_MIN )
        undefined();
}


// The int difference aB - aA of VECTOR2I operator-.
VECTOR2I guardedDifference( const VECTOR2I& aB, const VECTOR2I& aA )
{
    const int64_t x = int64_t( aB.x ) - aA.x;
    const int64_t y = int64_t( aB.y ) - aA.y;
    if( x < INT_MIN || x > INT_MAX || y < INT_MIN || y > INT_MAX )
        undefined();
    return VECTOR2I( static_cast<int>( x ), static_cast<int>( y ) );
}


// int64_t ( SEG::ecoord ) arithmetic that must not overflow.
int64_t checkedAdd( int64_t aA, int64_t aB )
{
    int64_t result;

    if( __builtin_add_overflow( aA, aB, &result ) )
        undefined();

    return result;
}


int64_t checkedSub( int64_t aA, int64_t aB )
{
    int64_t result;

    if( __builtin_sub_overflow( aA, aB, &result ) )
        undefined();

    return result;
}


int64_t checkedMul( int64_t aA, int64_t aB )
{
    int64_t result;

    if( __builtin_mul_overflow( aA, aB, &result ) )
        undefined();

    return result;
}


// seg.cpp's sgn.
int signOf( int64_t aValue )
{
    return ( 0 < aValue ) - ( aValue < 0 );
}


// SHAPE_ARC::IsEffectiveLine for the stored points: SEG( start, mid ).ApproxCollinear( SEG( mid,
// end ) ) with its default threshold 1 (SEG::mutualDistanceSquared), then the Dot of
// mid - start and end - mid. Refuses the undefined operations on the way.
bool guardedEffectiveLine( const VECTOR2I& aStart, const VECTOR2I& aMid, const VECTOR2I& aEnd )
{
    SEG a( aStart, aMid );
    SEG b( aMid, aEnd );

    // SquaredLength is ( A - B ).SquaredEuclideanNorm(), an int difference.
    const int64_t lengthA = guardedDifference( a.A, a.B ).SquaredEuclideanNorm();
    const int64_t lengthB = guardedDifference( b.A, b.B ).SquaredEuclideanNorm();

    if( lengthA < lengthB )
        std::swap( a, b );

    const int64_t p = int64_t( a.A.y ) - a.B.y;
    const int64_t q = int64_t( a.B.x ) - a.A.x;
    const int64_t r = checkedSub( checkedMul( -p, a.A.x ), checkedMul( q, a.A.y ) );
    const int64_t l = checkedAdd( checkedMul( p, p ), checkedMul( q, q ) );

    // A point line: ApproxCollinear is false and the Dot is not evaluated.
    if( l == 0 )
        return false;

    auto det = [&]( const VECTOR2I& aPoint )
               {
                   return checkedAdd( checkedAdd( checkedMul( p, aPoint.x ), checkedMul( q, aPoint.y ) ),
                                      r );
               };

    const int64_t det1 = det( b.A );
    const int64_t det2 = det( b.B );

    // The genuine util.cpp rescale: its __int128 quotient converts to int64_t modulo 2^64.
    const int64_t d1 = checkedMul( signOf( det1 ), rescale( det1, det1, l ) );
    const int64_t d2 = checkedMul( signOf( det2 ), rescale( det2, det2, l ) );

    // std::abs( INT64_MIN ) is undefined; the second abs runs only after the first passes.
    if( d1 == INT64_MIN )
        undefined();
    if( std::abs( d1 ) > 1 )
        return false;
    if( d2 == INT64_MIN )
        undefined();
    if( std::abs( d2 ) > 1 )
        return false;

    const VECTOR2I u = guardedDifference( aMid, aStart );
    const VECTOR2I v = guardedDifference( aEnd, aMid );

    return checkedAdd( checkedMul( u.x, v.x ), checkedMul( u.y, v.y ) ) > 0;
}


// SHAPE_ARC::IsCCW: the int64_t cross product of end - mid and start - mid.
bool guardedCCW( const VECTOR2I& aStart, const VECTOR2I& aMid, const VECTOR2I& aEnd )
{
    const VECTOR2L mid = aMid;
    const VECTOR2L v1 = aEnd - mid;
    const VECTOR2L v2 = aStart - mid;

    return checkedSub( checkedMul( v1.x, v2.y ), checkedMul( v1.y, v2.x ) ) > 0;
}


// SHAPE_ARC( aStart, aMid, aEnd, aWidth ) (shape_arc.cpp) copies its arguments and runs
// update_values. This replays that evaluation with the same expressions and types, on the
// genuine CalcArcCenter, EDA_ANGLE and rescale, and refuses exactly the arcs whose construction
// is undefined:
// - CalcArcCenter (trigo.cpp) is defined for every int point: 64- and 128-bit differences and
//   products, a division by a nonzero determinant, finite doubles, std::clamp of a finite value
//   and KiROUND of the clamped value;
// - the radius, GetStartAngle and the central angle's arithmetic use doubles of int values,
//   int64_t differences and EDA_ANGLE of finite vectors;
// - GetCentralAngle, for distinct start and end, evaluates IsEffectiveLine and, when that is
//   false, IsCCW (guarded above);
// - each quadrant point adds the rounded radius to an int centre coordinate;
// - BOX2I::Compute subtracts the int minimum of the points from their int maximum.
// The width is only stored. The floating values that select the quadrant points are therefore
// the constructor's own.
void guardArcConstructor( const VECTOR2I& aStart, const VECTOR2I& aMid, const VECTOR2I& aEnd )
{
    const VECTOR2I center = CalcArcCenter( aStart, aMid, aEnd );
    const double   radius = std::sqrt( ( VECTOR2D( aStart ) - center ).SquaredEuclideanNorm() );
    const VECTOR2L wideCenter = center;

    EDA_ANGLE startAngle( aStart - wideCenter );
    startAngle = startAngle.Normalize();

    EDA_ANGLE centralAngle = ANGLE_360;

    if( aStart != aEnd )
    {
        if( guardedEffectiveLine( aStart, aMid, aEnd ) )
        {
            centralAngle = ANGLE_0;
        }
        else
        {
            EDA_ANGLE angle = EDA_ANGLE( aEnd - wideCenter ) - EDA_ANGLE( aStart - wideCenter );

            if( guardedCCW( aStart, aMid, aEnd ) )
            {
                if( angle < ANGLE_0 )
                    angle += ANGLE_360;
            }
            else
            {
                if( angle > ANGLE_0 )
                    angle -= ANGLE_360;
            }

            centralAngle = angle;
        }
    }

    EDA_ANGLE endAngle = startAngle + centralAngle;

    if( startAngle > endAngle )
        std::swap( startAngle, endAngle );

    const int quadStart = std::ceil( startAngle.AsDegrees() / 90.0 );
    const int quadEnd = std::floor( endAngle.AsDegrees() / 90.0 );

    std::vector<VECTOR2I> points = { aStart, aMid, aEnd };

    if( radius < (double) INT_MAX / 2.0 )
    {
        const int rounded = KiROUND( radius );

        for( int quad = quadStart; quad <= quadEnd; ++quad )
        {
            VECTOR2I offset;

            switch( quad % 4 )
            {
            case 0:          offset = { rounded, 0 };  break;
            case 1: case -3: offset = { 0, rounded };  break;
            case 2: case -2: offset = { -rounded, 0 }; break;
            default:         offset = { 0, -rounded }; break;
            }

            int x, y;

            if( __builtin_add_overflow( center.x, offset.x, &x )
                || __builtin_add_overflow( center.y, offset.y, &y ) )
            {
                undefined();
            }

            points.emplace_back( x, y );
        }
    }

    VECTOR2I low = points[0];
    VECTOR2I high = points[0];

    for( const VECTOR2I& point : points )
    {
        low = VECTOR2I( std::min( low.x, point.x ), std::min( low.y, point.y ) );
        high = VECTOR2I( std::max( high.x, point.x ), std::max( high.y, point.y ) );
    }

    guardedDifference( high, low );
}


std::unique_ptr<SHAPE_ARC> buildArc( const JSON& aSpec )
{
    const std::string ctor = readText( aSpec, "ctor" );

    if( ctor == "default" )
    {
        checkKeys( aSpec, { "ctor" } );
        return std::make_unique<SHAPE_ARC>();
    }
    if( ctor == "three_point" )
    {
        checkKeys( aSpec, { "ctor", "start", "mid", "end", "width" } );
        const VECTOR2I start = readPointI( aSpec.at( "start" ) );
        const VECTOR2I mid = readPointI( aSpec.at( "mid" ) );
        const VECTOR2I end = readPointI( aSpec.at( "end" ) );
        const int      width = readInt( aSpec.at( "width" ) );
        guardArcConstructor( start, mid, end );
        return std::make_unique<SHAPE_ARC>( start, mid, end, width );
    }
    throw std::invalid_argument( "unknown arc constructor" );
}


DIRECTION_45 buildDirection( const JSON& aSpec )
{
    const std::string ctor = readText( aSpec, "ctor" );

    if( ctor == "default" )
    {
        checkKeys( aSpec, { "ctor" } );
        return DIRECTION_45();
    }
    if( ctor == "directions" )
    {
        checkKeys( aSpec, { "ctor", "value" } );
        // Directions has the fixed underlying type int: every int is a valid value.
        return DIRECTION_45( static_cast<DIRECTION_45::Directions>( readInt( aSpec.at( "value" ) ) ) );
    }
    if( ctor == "vector" )
    {
        checkKeysOptional( aSpec, { "ctor", "vec" }, { "a90" } );
        const VECTOR2I vec = readPointI( aSpec.at( "vec" ) );
        const bool     a90 = readA90( aSpec );
        guardConstructVector( vec );
        if( aSpec.contains( "a90" ) )
            return DIRECTION_45( vec, a90 );
        return DIRECTION_45( vec );
    }
    if( ctor == "seg" )
    {
        checkKeysOptional( aSpec, { "ctor", "a", "b" }, { "a90" } );
        const VECTOR2I a = readPointI( aSpec.at( "a" ) );
        const VECTOR2I b = readPointI( aSpec.at( "b" ) );
        const SEG      seg( a, b );
        const bool     a90 = readA90( aSpec );
        guardConstructVector( guardedDifference( seg.B, seg.A ) );
        if( aSpec.contains( "a90" ) )
            return DIRECTION_45( seg, a90 );
        return DIRECTION_45( seg );
    }
    if( ctor == "arc" )
    {
        checkKeysOptional( aSpec, { "ctor", "arc" }, { "a90" } );
        const std::unique_ptr<SHAPE_ARC> arc = buildArc( aSpec.at( "arc" ) );
        const bool                       a90 = readA90( aSpec );
        guardConstructVector( guardedDifference( arc->GetP1(), arc->GetP0() ) );
        if( aSpec.contains( "a90" ) )
            return DIRECTION_45( *arc, a90 );
        return DIRECTION_45( *arc );
    }
    throw std::invalid_argument( "unknown direction constructor" );
}


// Angle: the int difference and its std::abs, when neither direction is UNDEFINED.
void guardAngle( const DIRECTION_45& aDir, const DIRECTION_45& aOther )
{
    const int a = dirOf( aDir );
    const int b = dirOf( aOther );

    if( a == DIRECTION_45::UNDEFINED || b == DIRECTION_45::UNDEFINED )
        return;

    const int64_t d = int64_t( a ) - b;
    if( d <= INT_MIN || d > INT_MAX )
        undefined();
}


// Right and Left: m_dir + 2, m_dir + 1, m_dir + LAST (then - 2 or - 1), when defined.
void guardTurn( const DIRECTION_45& aDir, bool aRight )
{
    const int dir = dirOf( aDir );

    if( dir == DIRECTION_45::UNDEFINED )
        return;

    const int64_t step = aRight ? ( deg90Of( aDir ) ? 2 : 1 ) : DIRECTION_45::LAST;
    if( int64_t( dir ) + step > INT_MAX )
        undefined();
}


JSON runDirection( const JSON& aInput )
{
    const std::string member = readText( aInput, "member" );
    const bool        binary = member == "angle" || member == "is_obtuse" || member == "eq"
                        || member == "ne";

    if( binary )
        checkKeys( aInput, { "op", "member", "dir", "other" } );
    else
        checkKeys( aInput, { "op", "member", "dir" } );

    const DIRECTION_45 dir = buildDirection( aInput.at( "dir" ) );
    JSON               result = { { "state", dirState( dir ) } };

    if( binary )
    {
        const DIRECTION_45 other = buildDirection( aInput.at( "other" ) );
        result["other"] = dirState( other );

        if( member == "angle" )
        {
            guardAngle( dir, other );
            result["value"] = static_cast<int>( dir.Angle( other ) );
        }
        else if( member == "is_obtuse" )
        {
            guardAngle( dir, other );
            result["value"] = dir.IsObtuse( other );
        }
        else if( member == "eq" )
        {
            result["value"] = dir == other;
        }
        else
        {
            result["value"] = dir != other;
        }
        return result;
    }

    if( member == "state" )
        return result;
    if( member == "format" )
        result["value"] = dir.Format();
    else if( member == "opposite" )
    {
        if( dirOf( dir ) < 0 || dirOf( dir ) > 8 )
            undefined();
        result["value"] = dirState( dir.Opposite() );
    }
    else if( member == "is_diagonal" )
        result["value"] = dir.IsDiagonal();
    else if( member == "is_defined" )
        result["value"] = dir.IsDefined();
    else if( member == "right" )
    {
        guardTurn( dir, true );
        result["value"] = dirState( dir.Right() );
    }
    else if( member == "left" )
    {
        guardTurn( dir, false );
        result["value"] = dirState( dir.Left() );
    }
    else if( member == "to_vector" )
        result["value"] = pointI( dir.ToVector() );
    else if( member == "mask" )
    {
        if( dirOf( dir ) < 0 || dirOf( dir ) > 31 )
            undefined();
        result["value"] = dir.Mask();
    }
    else if( member == "all_directions_mask" )
        result["value"] = DIRECTION_45::AllDirectionsMask();
    else
        throw std::invalid_argument( "unknown member" );

    return result;
}


// ---------------------------------------------------------------------------------------------
// HALF_LINE and LINE.

SEG buildSeg( const JSON& aSpec )
{
    checkKeys( aSpec, { "a", "b", "index" } );
    const VECTOR2I a = readPointI( aSpec.at( "a" ) );
    const VECTOR2I b = readPointI( aSpec.at( "b" ) );
    const int      index = readInt( aSpec.at( "index" ) );
    return SEG( a, b, index );
}


template <typename T>
T buildRay( const JSON& aSpec )
{
    const std::string ctor = readText( aSpec, "ctor" );

    if( ctor == "seg" )
    {
        checkKeys( aSpec, { "ctor", "seg" } );
        return T( buildSeg( aSpec.at( "seg" ) ) );
    }
    if( ctor == "points" )
    {
        checkKeys( aSpec, { "ctor", "a", "b" } );
        const VECTOR2I a = readPointI( aSpec.at( "a" ) );
        const VECTOR2I b = readPointI( aSpec.at( "b" ) );
        return T( a, b );
    }
    throw std::invalid_argument( "unknown line constructor" );
}


JSON runHalfLine( const JSON& aInput )
{
    checkKeys( aInput, { "op", "line", "other" } );
    const HALF_LINE line = buildRay<HALF_LINE>( aInput.at( "line" ) );
    const HALF_LINE other = buildRay<HALF_LINE>( aInput.at( "other" ) );
    return JSON{ { "start", pointI( line.GetStart() ) },
                 { "contained_point", pointI( line.GetContainedPoint() ) },
                 { "seg", segState( line.GetContainedSeg() ) },
                 { "other_seg", segState( other.GetContainedSeg() ) },
                 { "eq", line == other } };
}


JSON runLine( const JSON& aInput )
{
    checkKeys( aInput, { "op", "line", "other" } );
    const LINE line = buildRay<LINE>( aInput.at( "line" ) );
    const LINE other = buildRay<LINE>( aInput.at( "other" ) );
    return JSON{ { "seg", segState( line.GetContainedSeg() ) },
                 { "other_seg", segState( other.GetContainedSeg() ) },
                 { "eq", line == other } };
}


// ---------------------------------------------------------------------------------------------
// geometry_utils.h templates.

JSON runSnapped( const JSON& aInput, bool a45 )
{
    if( a45 )
        checkKeysOptional( aInput, { "op", "type", "vec" }, { "only45" } );
    else
        checkKeys( aInput, { "op", "type", "vec" } );

    const bool floating = readFloating( aInput, "type" );
    const bool hasOnly45 = a45 && aInput.contains( "only45" );
    const bool only45 = hasOnly45 ? readBool( aInput.at( "only45" ) ) : false;

    if( floating )
    {
        const VECTOR2D vec = readPointD( aInput.at( "vec" ) );

        if( !a45 )
            return JSON{ { "vec", pointD( GetVectorSnapped90( vec ) ) } };
        if( hasOnly45 )
            return JSON{ { "vec", pointD( GetVectorSnapped45( vec, only45 ) ) } };
        return JSON{ { "vec", pointD( GetVectorSnapped45( vec ) ) } };
    }

    const VECTOR2I vec = readPointI( aInput.at( "vec" ) );

    // std::abs( INT_MIN ) of either coordinate.
    if( vec.x == INT_MIN || vec.y == INT_MIN )
        undefined();

    if( !a45 )
        return JSON{ { "vec", pointI( GetVectorSnapped90( vec ) ) } };
    if( hasOnly45 )
        return JSON{ { "vec", pointI( GetVectorSnapped45( vec, only45 ) ) } };
    return JSON{ { "vec", pointI( GetVectorSnapped45( vec ) ) } };
}


JSON runClamped( const JSON& aInput )
{
    checkKeysOptional( aInput, { "op", "in", "ret", "coords" }, { "padding" } );
    const bool     inFloating = readFloating( aInput, "in" );
    const bool     retFloating = readFloating( aInput, "ret" );
    const bool     hasPadding = aInput.contains( "padding" );
    const uint32_t padding = hasPadding ? readU32( aInput.at( "padding" ) ) : 1u;

    if( !inFloating && retFloating )
        throw std::invalid_argument( "unsupported instantiation" );

    // A padding above INT_MAX makes min = -max exceed max: std::clamp requires !( hi < lo ).
    auto guardPadding = [&]()
    {
        if( padding > uint32_t( INT_MAX ) )
            undefined();
    };

    if( !inFloating )
    {
        const VECTOR2I coords = readPointI( aInput.at( "coords" ) );
        guardPadding();
        const VECTOR2I v = hasPadding ? GetClampedCoords<int, int>( coords, padding )
                                      : GetClampedCoords<int, int>( coords );
        return JSON{ { "coords", pointI( v ) } };
    }

    const VECTOR2D coords = readPointD( aInput.at( "coords" ) );
    guardPadding();

    // std::clamp<double>( v, min, max ) requires operator< to induce a strict weak ordering on
    // the three values ([alg.clamp]/2): a NaN is equivalent to both bounds, so it breaks the
    // transitivity of equivalence unless the bounds are equivalent (padding INT_MAX gives -0.0
    // and 0.0).
    const double max = static_cast<double>( INT_MAX ) - padding;
    const double min = -max;

    if( ( std::isnan( coords.x ) || std::isnan( coords.y ) ) && min < max )
        undefined();

    if( retFloating )
    {
        const VECTOR2D v = hasPadding ? GetClampedCoords<double, double>( coords, padding )
                                      : GetClampedCoords<double, double>( coords );
        return JSON{ { "coords", pointD( v ) } };
    }

    const VECTOR2I v = hasPadding ? GetClampedCoords<double, int>( coords, padding )
                                  : GetClampedCoords<double, int>( coords );
    return JSON{ { "coords", pointI( v ) } };
}


JSON runSafe( const JSON& aInput )
{
    checkKeys( aInput, { "op", "type", "vec" } );

    if( readFloating( aInput, "type" ) )
        return JSON{ { "value", IsVec2SafeXY( readPointD( aInput.at( "vec" ) ) ) } };
    return JSON{ { "value", IsVec2SafeXY( readPointI( aInput.at( "vec" ) ) ) } };
}


// ---------------------------------------------------------------------------------------------
// geometry_predicates.h.

JSON runPredicate( const JSON& aInput, const std::string& aOp )
{
    if( aOp == "orientation_sign" || aOp == "is_sliver_triangle" )
    {
        checkKeys( aInput, { "op", "a", "b", "c" } );
        const VECTOR2I a = readPointI( aInput.at( "a" ) );
        const VECTOR2I b = readPointI( aInput.at( "b" ) );
        const VECTOR2I c = readPointI( aInput.at( "c" ) );

        if( aOp == "orientation_sign" )
            return JSON{ { "value", KIGEOM::OrientationSign( a, b, c ) } };
        return JSON{ { "value", KIGEOM::IsSliverTriangle( a, b, c ) } };
    }

    checkKeys( aInput, { "op", "a", "b", "c", "p" } );
    const VECTOR2I a = readPointI( aInput.at( "a" ) );
    const VECTOR2I b = readPointI( aInput.at( "b" ) );
    const VECTOR2I c = readPointI( aInput.at( "c" ) );
    const VECTOR2I p = readPointI( aInput.at( "p" ) );

    if( aOp == "in_circle_legal_double" )
        return JSON{ { "value", KIGEOM::detail::inCircleLegalDouble( a, b, c, p ) } };
    return JSON{ { "value", KIGEOM::InCircleDelaunayLegal( a, b, c, p ) } };
}


// ---------------------------------------------------------------------------------------------
// fracture_edge_index_utils.h.

JSON runFracture( const JSON& aInput, const std::string& aOp )
{
    using namespace KIGEOM::FRACTURE_INDEX;

    if( aOp == "stripe_count_for" )
    {
        checkKeys( aInput, { "op", "edge_count" } );
        return JSON{ { "value", StripeCountFor( readU64( aInput.at( "edge_count" ) ) ) } };
    }
    if( aOp == "map_y_to_stripe" )
    {
        checkKeys( aInput, { "op", "y", "min_y", "max_y", "stripe_count" } );
        const int      y = readInt( aInput.at( "y" ) );
        const int      minY = readInt( aInput.at( "min_y" ) );
        const int      maxY = readInt( aInput.at( "max_y" ) );
        const uint32_t stripes = readU32( aInput.at( "stripe_count" ) );
        return JSON{ { "value", MapYToStripe( y, minY, maxY, stripes ) } };
    }
    if( aOp == "stripe_span" )
    {
        checkKeys( aInput, { "op", "y1", "y2", "min_y", "max_y", "stripe_count" } );
        const int      y1 = readInt( aInput.at( "y1" ) );
        const int      y2 = readInt( aInput.at( "y2" ) );
        const int      minY = readInt( aInput.at( "min_y" ) );
        const int      maxY = readInt( aInput.at( "max_y" ) );
        const uint32_t stripes = readU32( aInput.at( "stripe_count" ) );
        const auto     span = StripeSpan( y1, y2, minY, maxY, stripes );
        return JSON{ { "value", JSON::array( { span.first, span.second } ) } };
    }
    if( aOp == "checked_add" )
    {
        checkKeys( aInput, { "op", "total", "count", "element_size" } );
        size_t       total = readU64( aInput.at( "total" ) );
        const size_t count = readU64( aInput.at( "count" ) );
        const size_t size = readU64( aInput.at( "element_size" ) );
        const bool   value = CheckedAdd( total, count, size );
        return JSON{ { "value", value }, { "total", uint64_t( total ) } };
    }
    if( aOp == "capacity_budget" )
    {
        checkKeys( aInput, { "op", "edge_count", "edge_size" } );
        const size_t count = readU64( aInput.at( "edge_count" ) );
        const size_t size = readU64( aInput.at( "edge_size" ) );
        return JSON{ { "value", uint64_t( CapacityBudget( count, size ) ) } };
    }
    if( aOp == "capacity_fits" )
    {
        checkKeys( aInput, { "op", "bucket_ids", "long_ids", "stripe_count", "hole_count", "node_size",
                             "budget" } );
        const size_t bucketIds = readU64( aInput.at( "bucket_ids" ) );
        const size_t longIds = readU64( aInput.at( "long_ids" ) );
        const size_t stripes = readU64( aInput.at( "stripe_count" ) );
        const size_t holes = readU64( aInput.at( "hole_count" ) );
        const size_t nodeSize = readU64( aInput.at( "node_size" ) );
        const size_t budget = readU64( aInput.at( "budget" ) );
        return JSON{ { "value", CapacityFits( bucketIds, longIds, stripes, holes, nodeSize, budget ) } };
    }
    if( aOp == "actual_capacity_fits" )
    {
        checkKeysOptional( aInput, { "op", "bucket_ids", "long_ids", "offsets", "scratch", "heads", "nodes",
                                     "node_size", "budget" },
                           { "bytes" } );
        const size_t bucketIds = readU64( aInput.at( "bucket_ids" ) );
        const size_t longIds = readU64( aInput.at( "long_ids" ) );
        const size_t offsets = readU64( aInput.at( "offsets" ) );
        const size_t scratch = readU64( aInput.at( "scratch" ) );
        const size_t heads = readU64( aInput.at( "heads" ) );
        const size_t nodes = readU64( aInput.at( "nodes" ) );
        const size_t nodeSize = readU64( aInput.at( "node_size" ) );
        const size_t budget = readU64( aInput.at( "budget" ) );

        if( !aInput.contains( "bytes" ) )
        {
            return JSON{ { "value", ActualCapacityFits( bucketIds, longIds, offsets, scratch, heads, nodes,
                                                        nodeSize, budget ) } };
        }

        size_t     bytes = readU64( aInput.at( "bytes" ) );
        const bool value = ActualCapacityFits( bucketIds, longIds, offsets, scratch, heads, nodes, nodeSize,
                                               budget, &bytes );
        return JSON{ { "value", value }, { "bytes", uint64_t( bytes ) } };
    }
    if( aOp == "should_index" )
    {
        checkKeys( aInput, { "op", "estimated_visits", "hole_count" } );
        const uint64_t visits = readU64( aInput.at( "estimated_visits" ) );
        const size_t   holes = readU64( aInput.at( "hole_count" ) );
        return JSON{ { "value", ShouldIndex( visits, holes ) } };
    }
    throw std::invalid_argument( "unknown operation" );
}


// ---------------------------------------------------------------------------------------------
// The adopted co-ports and the header types.

JSON chordState( const ARC_CHORD_PARAMS& aParams )
{
    return JSON{ { "valid", aParams.IsValid() },
                 { "chord_length", bits( aParams.GetChordLength() ) },
                 { "half_chord", bits( aParams.GetHalfChord() ) },
                 { "chord_unit_vec", pointD( aParams.GetChordUnitVec() ) },
                 { "normal_unit_vec", pointD( aParams.GetNormalUnitVec() ) },
                 { "sagitta", bits( aParams.GetSagitta() ) },
                 { "radius", bits( aParams.GetRadius() ) },
                 { "center_offset", bits( aParams.GetCenterOffset() ) },
                 { "chord_midpoint", pointD( aParams.GetChordMidpoint() ) },
                 { "ux", bits( aParams.GetUx() ) },
                 { "uy", bits( aParams.GetUy() ) },
                 { "nx", bits( aParams.GetNx() ) },
                 { "ny", bits( aParams.GetNy() ) },
                 { "mid_x", bits( aParams.GetMidX() ) },
                 { "mid_y", bits( aParams.GetMidY() ) } };
}


// A default-constructed ARC_CHORD_PARAMS, then Compute for each point triple in order (members
// keep their values across a refused Compute); every accessor afterwards.
JSON runArcChord( const JSON& aInput )
{
    checkKeys( aInput, { "op", "computes" } );
    const JSON& computes = aInput.at( "computes" );

    if( !computes.is_array() || computes.size() > 4 )
        throw std::invalid_argument( "expected at most four computes" );

    ARC_CHORD_PARAMS params;
    JSON             returns = JSON::array();

    for( const JSON& triple : computes )
    {
        if( !triple.is_array() || triple.size() != 3 )
            throw std::invalid_argument( "expected three points" );
        const VECTOR2I start = readPointI( triple[0] );
        const VECTOR2I mid = readPointI( triple[1] );
        const VECTOR2I end = readPointI( triple[2] );
        returns.push_back( params.Compute( start, mid, end ) );
    }

    return JSON{ { "returns", returns }, { "state", chordState( params ) } };
}


CIRCLE buildCircle( const JSON& aSpec )
{
    const std::string ctor = readText( aSpec, "ctor" );

    if( ctor == "default" )
    {
        checkKeys( aSpec, { "ctor" } );
        return CIRCLE();
    }
    if( ctor == "center_radius" )
    {
        checkKeys( aSpec, { "ctor", "center", "radius" } );
        const VECTOR2I center = readPointI( aSpec.at( "center" ) );
        const int      radius = readInt( aSpec.at( "radius" ) );
        return CIRCLE( center, radius );
    }
    throw std::invalid_argument( "unknown circle constructor" );
}


JSON runCircleEq( const JSON& aInput )
{
    checkKeys( aInput, { "op", "a", "b" } );
    const CIRCLE a = buildCircle( aInput.at( "a" ) );
    const CIRCLE b = buildCircle( aInput.at( "b" ) );
    CIRCLE       assigned;
    assigned = a;
    return JSON{ { "a", { { "radius", a.Radius }, { "center", pointI( a.Center ) } } },
                 { "b", { { "radius", b.Radius }, { "center", pointI( b.Center ) } } },
                 { "eq", a == b },
                 { "assigned_eq", assigned == a } };
}


JSON optionalSeg( const std::optional<SEG>& aSeg )
{
    return aSeg ? segState( *aSeg ) : JSON();
}


// An enumerator as its int value.
template <typename E>
int I( E aValue )
{
    return static_cast<int>( aValue );
}


JSON runHeaderTypes( const JSON& aInput )
{
    checkKeys( aInput, { "op" } );
    using namespace KIGEOM::FRACTURE_INDEX;

    const KIGEOM::ARC_SOLUTION solution;
    const INTERSECTION_CONTACT contact;
    const CHAMFER_PARAMS       chamferParams{};
    const CHAMFER_RESULT       chamfer{};
    const DOGBONE_RESULT       dogbone{};

    return JSON{
        { "error_loc", JSON::array( { I( ERROR_OUTSIDE ), I( ERROR_INSIDE ) } ) },
        { "corner_strategy",
          JSON::array( { static_cast<int>( CORNER_STRATEGY::ALLOW_ACUTE_CORNERS ),
                         static_cast<int>( CORNER_STRATEGY::CHAMFER_ACUTE_CORNERS ),
                         static_cast<int>( CORNER_STRATEGY::ROUND_ACUTE_CORNERS ),
                         static_cast<int>( CORNER_STRATEGY::CHAMFER_ALL_CORNERS ),
                         static_cast<int>( CORNER_STRATEGY::ROUND_ALL_CORNERS ) } ) },
        { "leader_mode", JSON::array( { static_cast<int>( LEADER_MODE::DIRECT ),
                                        static_cast<int>( LEADER_MODE::DEG45 ),
                                        static_cast<int>( LEADER_MODE::DEG90 ) } ) },
        { "oval_key_points",
          JSON::array( { I( KIGEOM::OVAL_CENTER ), I( KIGEOM::OVAL_CAP_TIPS ), I( KIGEOM::OVAL_CAP_CENTERS ),
                         I( KIGEOM::OVAL_SIDE_MIDPOINTS ), I( KIGEOM::OVAL_SIDE_ENDS ),
                         I( KIGEOM::OVAL_CARDINAL_EXTREMES ), I( KIGEOM::OVAL_ALL_KEY_POINTS ) } ) },
        { "directions",
          JSON::array( { I( DIRECTION_45::N ), I( DIRECTION_45::NE ), I( DIRECTION_45::E ), I( DIRECTION_45::SE ),
                         I( DIRECTION_45::S ), I( DIRECTION_45::SW ), I( DIRECTION_45::W ), I( DIRECTION_45::NW ),
                         I( DIRECTION_45::LAST ), I( DIRECTION_45::UNDEFINED ) } ) },
        { "corner_mode", JSON::array( { I( DIRECTION_45::MITERED_45 ), I( DIRECTION_45::ROUNDED_45 ),
                                        I( DIRECTION_45::MITERED_90 ), I( DIRECTION_45::ROUNDED_90 ) } ) },
        { "angle_type",
          JSON::array( { I( DIRECTION_45::ANG_OBTUSE ), I( DIRECTION_45::ANG_RIGHT ), I( DIRECTION_45::ANG_ACUTE ),
                         I( DIRECTION_45::ANG_STRAIGHT ), I( DIRECTION_45::ANG_HALF_FULL ),
                         I( DIRECTION_45::ANG_UNDEFINED ) } ) },
        { "fracture_constants",
          JSON::array( { uint64_t( MAX_STRIPES ), uint64_t( MAX_BUCKET_SPAN ),
                         uint64_t( EDGE_SET_BUDGET_MULTIPLE ), uint64_t( MIN_EDGE_VISITS ),
                         uint64_t( MIN_HOLE_COUNT ) } ) },
        { "arc_solution",
          { { "valid", solution.valid },
            { "start", pointI( solution.start ) },
            { "mid", pointI( solution.mid ) },
            { "end", pointI( solution.end ) },
            { "center", pointD( solution.center ) },
            { "radius", bits( solution.radius ) } } },
        { "intersection_contact",
          { { "tangent", contact.m_Tangent }, { "overlapping", contact.m_Overlapping } } },
        { "chamfer_params",
          JSON::array( { chamferParams.m_chamfer_setback_a, chamferParams.m_chamfer_setback_b } ) },
        { "chamfer_result",
          { { "chamfer", segState( chamfer.m_chamfer ) },
            { "updated_seg_a", optionalSeg( chamfer.m_updated_seg_a ) },
            { "updated_seg_b", optionalSeg( chamfer.m_updated_seg_b ) } } },
        { "dogbone_result",
          { { "arc", JSON::array( { pointI( dogbone.m_arc.GetP0() ), pointI( dogbone.m_arc.GetArcMid() ),
                                    pointI( dogbone.m_arc.GetP1() ), dogbone.m_arc.GetWidth() } ) },
            { "updated_seg_a", optionalSeg( dogbone.m_updated_seg_a ) },
            { "updated_seg_b", optionalSeg( dogbone.m_updated_seg_b ) },
            { "small_arc_mouth", dogbone.m_small_arc_mouth } } },
    };
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "direction45" )
        return runDirection( aInput );
    if( op == "half_line" )
        return runHalfLine( aInput );
    if( op == "line" )
        return runLine( aInput );
    if( op == "snapped45" )
        return runSnapped( aInput, true );
    if( op == "snapped90" )
        return runSnapped( aInput, false );
    if( op == "clamped_coords" )
        return runClamped( aInput );
    if( op == "is_vec2_safe_xy" )
        return runSafe( aInput );
    if( op == "orientation_sign" || op == "is_sliver_triangle" || op == "in_circle_legal_double"
        || op == "in_circle_delaunay_legal" )
        return runPredicate( aInput, op );
    if( op == "stripe_count_for" || op == "map_y_to_stripe" || op == "stripe_span" || op == "checked_add"
        || op == "capacity_budget" || op == "capacity_fits" || op == "actual_capacity_fits"
        || op == "should_index" )
        return runFracture( aInput, op );
    if( op == "arc_chord_params" )
        return runArcChord( aInput );
    if( op == "circle_eq" )
        return runCircleEq( aInput );
    if( op == "header_types" )
        return runHeaderTypes( aInput );
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterGeometryHeadersP001( REGISTRY& aRegistry )
{
    aRegistry.Add( GEOMETRY_P001_UNIT, "geometry/{direction45,geometry_predicates,...}.h", run );
}
} // namespace oracle
