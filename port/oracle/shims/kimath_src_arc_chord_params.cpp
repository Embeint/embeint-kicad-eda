// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned ARC_CHORD_PARAMS (libs/kimath/src/geometry/arc_chord_params.cpp).
// The adapter only decodes 32-bit integer points and encodes every observable value as
// exact IEEE bits; it never computes a reply itself.
#include "registry.h"
#include <geometry/arc_chord_params.h>
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
constexpr const char* ARC_CHORD_UNIT = "libs.kimath.src.geometry.arc_chord_params.application.pure.p001";
constexpr size_t      MAX_COMPUTES = 8;


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


const JSON& readArray( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    return aInput.at( aName );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON vector( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
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


// An observation whose pinned evaluation is undefined is reported in place of its value; the
// other observations of the same state are kept.
JSON undefinedObservation()
{
    return JSON{ { "error", { { "kind", "invalid_input" }, { "message", "undefined arc chord operation" } } } };
}


// GetArcAngle passes m_halfChord / m_radius to std::clamp, which C++20 [alg.clamp] defines only
// for Cpp17LessThanComparable values; NaN is not (libstdc++ happens to pass it through, which does
// not make the call defined). The quotient is NaN when both members are still 0 (a default object,
// or one whose every Compute so far was refused). The guard evaluates the same IEEE quotient from
// the header accessors and never calls the pinned getter for those states. No other getter or
// Compute makes an ordering-dependent library call on a NaN: comparisons are built-in operators,
// and atan2/asin of NaN are IEC 60559 defined.
JSON arcAngle( const ARC_CHORD_PARAMS& aParams )
{
    if( std::isnan( aParams.GetHalfChord() / aParams.GetRadius() ) )
        return undefinedObservation();
    return bits( aParams.GetArcAngle() );
}


// Every public observation of the object: the header accessors and the out-of-line getters.
JSON state( const ARC_CHORD_PARAMS& aParams )
{
    JSON out = JSON::object();
    out["valid"] = aParams.IsValid();
    out["chord_length"] = bits( aParams.GetChordLength() );
    out["half_chord"] = bits( aParams.GetHalfChord() );
    out["chord_unit"] = vector( aParams.GetChordUnitVec() );
    out["normal_unit"] = vector( aParams.GetNormalUnitVec() );
    out["sagitta"] = bits( aParams.GetSagitta() );
    out["radius"] = bits( aParams.GetRadius() );
    out["center_offset"] = bits( aParams.GetCenterOffset() );
    out["chord_midpoint"] = vector( aParams.GetChordMidpoint() );
    out["raw"] = JSON::array( { bits( aParams.GetUx() ), bits( aParams.GetUy() ), bits( aParams.GetNx() ),
                                bits( aParams.GetNy() ), bits( aParams.GetMidX() ),
                                bits( aParams.GetMidY() ) } );
    out["center"] = vector( aParams.GetCenterPoint() );
    out["start_angle"] = bits( aParams.GetStartAngle().AsDegrees() );
    out["end_angle"] = bits( aParams.GetEndAngle().AsDegrees() );
    out["arc_angle"] = arcAngle( aParams );
    return out;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "default" )
    {
        checkKeys( aInput, { "op" } );
        return state( ARC_CHORD_PARAMS() );
    }
    if( op == "compute" )
    {
        // One object receives every Compute in order, so a refused Compute is observed
        // with the members it left from the previous one.
        checkKeys( aInput, { "op", "computes" } );
        const JSON& computes = readArray( aInput, "computes" );
        if( computes.empty() || computes.size() > MAX_COMPUTES )
            throw std::invalid_argument( "computes out of range" );
        ARC_CHORD_PARAMS params;
        JSON             steps = JSON::array();
        for( const JSON& points : computes )
        {
            if( !points.is_array() || points.size() != 3 )
                throw std::invalid_argument( "expected start, mid, end" );
            const VECTOR2I start = readPoint( points[0] );
            const VECTOR2I mid = readPoint( points[1] );
            const VECTOR2I end = readPoint( points[2] );
            const bool     returned = params.Compute( start, mid, end );
            steps.push_back( JSON{ { "returned", returned }, { "state", state( params ) } } );
        }
        return steps;
    }
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterArcChordParams( REGISTRY& aRegistry )
{
    aRegistry.Add( ARC_CHORD_UNIT, "geometry/arc_chord_params.cpp", run );
}
} // namespace oracle
