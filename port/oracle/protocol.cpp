// SPDX-License-Identifier: GPL-3.0-or-later
#include "protocol.h"
#include <cmath>
#include <set>
#include <vector>

namespace oracle
{
static JSON Failure( const JSON& aUnit, const JSON& aCase, const std::string& aKind,
                     const std::string& aMessage )
{
    return JSON{ { "unit", aUnit }, { "case", aCase },
                 { "error", { { "kind", aKind }, { "message", aMessage } } } };
}


static bool FiniteJSON( const JSON& aValue )
{
    if( aValue.is_number_float() )
        return std::isfinite( aValue.get<double>() );

    if( aValue.is_structured() )
    {
        for( const auto& child : aValue )
        {
            if( !FiniteJSON( child ) )
                return false;
        }
    }

    return true;
}


JSON Dispatch( const std::string& aLine, const oracle::REGISTRY& aRegistry )
{
    JSON unit = nullptr;
    JSON caseId = nullptr;
    JSON request;

    try
    {
        // Reject duplicate object keys instead of silently accepting the last value.
        std::vector<std::set<std::string>> keys;
        request = JSON::parse( aLine, [&keys]( int depth, JSON::parse_event_t event, JSON& value )
        {
            // Match the native Rust request's serde_json::Value recursion bound:
            // 127 open containers, including the root and empty containers.
            // nlohmann reports zero-based container-start depth; scalars do not count.
            if( ( event == JSON::parse_event_t::object_start
                  || event == JSON::parse_event_t::array_start ) && depth >= 127 )
            {
                throw std::invalid_argument( "JSON nesting exceeds request bound" );
            }

            if( event == JSON::parse_event_t::object_start )
                keys.emplace_back();
            else if( event == JSON::parse_event_t::object_end )
                keys.pop_back();
            else if( event == JSON::parse_event_t::key
                     && !keys.back().insert( value.get<std::string>() ).second )
                throw std::invalid_argument( "duplicate object key" );

            return true;
        } );

        if( request.is_object() )
        {
            if( request.contains( "unit" ) && request["unit"].is_string() )
                unit = request["unit"];

            if( request.contains( "case" ) && request["case"].is_string() )
                caseId = request["case"];
        }

        if( !request.is_object() || request.size() != 3 || !unit.is_string()
            || !caseId.is_string() || unit.get<std::string>().empty()
            || caseId.get<std::string>().empty() || !request.contains( "input" )
            || !request["input"].is_object() )
        {
            return Failure( unit, caseId, "invalid_request", "expected unit, case, input object" );
        }
    }
    catch( const std::exception& )
    {
        return Failure( unit, caseId, "invalid_request", "invalid JSON or duplicate key" );
    }

    const auto entry = aRegistry.Units().find( unit.get<std::string>() );

    if( entry == aRegistry.Units().end() )
        return Failure( unit, caseId, "unknown_unit", "unit has no registered shim" );

    try
    {
        JSON result = entry->second.run( request["input"] );

        // JSON has no NaN/infinity. Never let dump() silently replace them with null.
        if( !FiniteJSON( result ) )
            return Failure( unit, caseId, "execution_error", "non-finite shim result" );

        try
        {
            (void) result.dump();
        }
        catch( const JSON::exception& )
        {
            return Failure( unit, caseId, "execution_error", "invalid JSON shim result" );
        }

        return JSON{ { "unit", unit }, { "case", caseId }, { "result", std::move( result ) } };
    }
    catch( const oracle::UNPORTED& error )
    {
        return JSON{ { "unit", unit }, { "case", caseId },
                     { "error", { { "kind", "unported" }, { "upstream", error.upstream },
                                  { "unit", error.unit } } } };
    }
    catch( const std::invalid_argument& error )
    {
        return Failure( unit, caseId, "invalid_input", error.what() );
    }
    catch( const JSON::exception& error )
    {
        return Failure( unit, caseId, "invalid_input", error.what() );
    }
    catch( const std::exception& error )
    {
        return Failure( unit, caseId, "execution_error", error.what() );
    }
}
} // namespace oracle
