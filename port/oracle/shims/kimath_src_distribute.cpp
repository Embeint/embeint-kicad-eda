// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned GetDeltasForDistributeByGaps / GetDeltasForDistributeByPoints
// (libs/kimath/src/geometry/distribute.cpp). The adapter decodes 32-bit integer inputs, calls
// the reference functions and encodes their deltas; it never computes a delta itself.
//
// Both functions compute in int. A signed int overflow is undefined (C++ [expr.pre]/4), so a
// call whose evaluation would overflow has no reference result. Before each call the guard
// below replays only the int operations of the pinned source, in source order, with checked
// arithmetic, and reports the first overflowing operation instead of calling the function. It
// refuses nothing else: every input without an overflowing operation reaches the reference.
#include "registry.h"
#include <geometry/distribute.h>
#include <math/util.h>
#include <climits>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace
{
using oracle::JSON;
constexpr const char* DISTRIBUTE_UNIT = "libs.kimath.src.geometry.distribute.application.ui.p001";
constexpr size_t      MAX_CALLS = 8;
constexpr size_t      MAX_ITEMS = 4096;


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


const JSON& readArray( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        throw std::invalid_argument( std::string( "expected array: " ) + aName );
    return aInput.at( aName );
}


int readInt( const JSON& aValue )
{
    if( !aValue.is_number_integer() || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() > INT_MAX )
        || aValue.get<int64_t>() < INT_MIN || aValue.get<int64_t>() > INT_MAX )
        throw std::invalid_argument( "coordinate out of range" );
    return aValue.get<int>();
}


const JSON& readItems( const JSON& aCall, const char* aName )
{
    const JSON& items = readArray( aCall, aName );
    if( items.size() > MAX_ITEMS )
        throw std::invalid_argument( "items out of range" );
    return items;
}


std::string overflow( const char* aOperation )
{
    return std::string( "signed overflow: " ) + aOperation;
}


std::string overflow( const char* aOperation, size_t aIndex )
{
    return overflow( aOperation ) + " at " + std::to_string( aIndex );
}


// The int operations of GetDeltasForDistributeByGaps in source order. KiROUND's value is the
// genuine header function (quiet: the reference call below performs the real, non-quiet one).
std::optional<std::string> gapsOverflow( const std::vector<std::pair<int, int>>& aItems )
{
    if( aItems.size() < 3 )
        return std::nullopt;

    int totalGap = 0;
    if( __builtin_sub_overflow( aItems.back().first, aItems.front().second, &totalGap ) )
        return overflow( "gaps total space" );

    for( size_t i = 1; i < aItems.size() - 1; ++i )
    {
        int span = 0;
        if( __builtin_sub_overflow( aItems[i].second, aItems[i].first, &span ) )
            return overflow( "gaps item span", i );
        if( __builtin_sub_overflow( totalGap, span, &totalGap ) )
            return overflow( "gaps total gap", i );
    }

    const double perItemGap = totalGap / double( aItems.size() - 1 );
    int          targetPos = aItems.front().second;

    for( size_t i = 1; i < aItems.size() - 1; ++i )
    {
        const int rounded = KiROUND<double, int>( i * perItemGap, true );
        int       offset = 0;
        int       delta = 0;
        if( __builtin_sub_overflow( targetPos, aItems[i].first, &offset ) )
            return overflow( "gaps target offset", i );
        if( __builtin_add_overflow( offset, rounded, &delta ) )
            return overflow( "gaps delta", i );
        // The span was already evaluated without overflow above.
        const int span = aItems[i].second - aItems[i].first;
        if( __builtin_add_overflow( targetPos, span, &targetPos ) )
            return overflow( "gaps target advance", i );
    }

    return std::nullopt;
}


// The int operations of GetDeltasForDistributeByPoints in source order.
std::optional<std::string> pointsOverflow( const std::vector<int>& aItems )
{
    if( aItems.size() < 3 )
        return std::nullopt;

    int totalGaps = 0;
    if( __builtin_sub_overflow( aItems.back(), aItems.front(), &totalGaps ) )
        return overflow( "points total gaps" );

    const double itemGap = totalGaps / double( aItems.size() - 1 );

    for( size_t i = 1; i < aItems.size() - 1; ++i )
    {
        int targetPos = 0;
        int delta = 0;
        if( __builtin_add_overflow( aItems.front(), KiROUND<double, int>( i * itemGap, true ), &targetPos ) )
            return overflow( "points target", i );
        if( __builtin_sub_overflow( targetPos, aItems[i], &delta ) )
            return overflow( "points delta", i );
    }

    return std::nullopt;
}


JSON refused( const std::string& aMessage )
{
    return JSON{ { "refused", { { "kind", "signed_overflow" }, { "message", aMessage } } } };
}


JSON deltas( const std::vector<int>& aDeltas )
{
    return JSON{ { "deltas", aDeltas } };
}


// One decoded call: extents for "gaps", positions for "points".
struct CALL
{
    bool                             gaps = false;
    std::vector<std::pair<int, int>> extents;
    std::vector<int>                 positions;
};


CALL readCall( const JSON& aCall )
{
    if( !aCall.is_object() || !aCall.contains( "fn" ) || !aCall.at( "fn" ).is_string() )
        throw std::invalid_argument( "expected string: fn" );
    const std::string fn = aCall.at( "fn" ).get<std::string>();
    CALL              call;

    if( fn == "gaps" )
    {
        checkKeys( aCall, { "fn", "extents" } );
        call.gaps = true;
        for( const JSON& extent : readItems( aCall, "extents" ) )
        {
            if( !extent.is_array() || extent.size() != 2 )
                throw std::invalid_argument( "expected extent" );
            call.extents.emplace_back( readInt( extent[0] ), readInt( extent[1] ) );
        }
        return call;
    }
    if( fn == "points" )
    {
        checkKeys( aCall, { "fn", "positions" } );
        for( const JSON& position : readItems( aCall, "positions" ) )
            call.positions.push_back( readInt( position ) );
        return call;
    }
    throw std::invalid_argument( "unknown function" );
}


JSON runCall( const CALL& aCall )
{
    if( aCall.gaps )
    {
        if( std::optional<std::string> ub = gapsOverflow( aCall.extents ) )
            return refused( *ub );
        return deltas( GetDeltasForDistributeByGaps( aCall.extents ) );
    }
    if( std::optional<std::string> ub = pointsOverflow( aCall.positions ) )
        return refused( *ub );
    return deltas( GetDeltasForDistributeByPoints( aCall.positions ) );
}


JSON run( const JSON& aInput )
{
    // Every call is decoded before any runs, so a malformed request never runs a call. A
    // refused call does not stop the calls after it: each later call is a fresh, independent
    // reference call, which is how the controls show a successful continuation.
    checkKeys( aInput, { "calls" } );
    const JSON& input = readArray( aInput, "calls" );
    if( input.empty() || input.size() > MAX_CALLS )
        throw std::invalid_argument( "calls out of range" );

    std::vector<CALL> calls;
    for( const JSON& call : input )
        calls.push_back( readCall( call ) );

    JSON results = JSON::array();
    for( const CALL& call : calls )
        results.push_back( runCall( call ) );
    return results;
}
} // namespace

namespace oracle
{
void RegisterDistribute( REGISTRY& aRegistry )
{
    aRegistry.Add( DISTRIBUTE_UNIT, "geometry/distribute.cpp", run );
}
} // namespace oracle
