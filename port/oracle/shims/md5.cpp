// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"
#include <md5_hash.h>
#include <algorithm>
#include <array>
#include <bit>
#include <climits>
#include <initializer_list>
#include <memory>
#include <vector>

namespace oracle
{
static_assert( sizeof( int ) == 4 && CHAR_BIT == 8 && std::endian::native == std::endian::little,
               "MD5 integer cases require the pinned Linux little-endian int contract" );

using STATES = std::array<std::unique_ptr<MD5_HASH>, 16>;


static void Keys( const JSON& aValue, std::initializer_list<const char*> aRequired,
                  std::initializer_list<const char*> aOptional = {} )
{
    if( !aValue.is_object() )
        throw std::invalid_argument( "expected object" );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            throw std::invalid_argument( "unexpected or missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        if( std::find( aRequired.begin(), aRequired.end(), key ) == aRequired.end()
            && std::find( aOptional.begin(), aOptional.end(), key ) == aOptional.end() )
        {
            throw std::invalid_argument( "unexpected or missing field" );
        }
    }
}


static MD5_HASH& State( STATES& aStates, int aId )
{
    if( !aStates[aId] )
        throw std::invalid_argument( "state does not exist" );

    return *aStates[aId];
}


static JSON Snapshot( MD5_HASH& aHash, int aId )
{
    return { { "id", aId }, { "valid", aHash.IsValid() }, { "compact", aHash.Format( true ) },
             { "spaced", aHash.Format( false ) }, { "default", aHash.Format() } };
}


static JSON Md5( const JSON& aInput )
{
    Keys( aInput, { "steps" } );
    const JSON& steps = aInput.at( "steps" );

    if( !steps.is_array() )
        throw std::invalid_argument( "expected steps array" );

    if( steps.empty() || steps.size() > 4096 )
        throw std::invalid_argument( "steps outside 1..4096" );

    STATES states;
    JSON events = JSON::array();
    uint64_t total = 0;
    constexpr uint64_t MAX_BYTES = 600ULL * 1024 * 1024;

    for( const JSON& step : steps )
    {
        if( !step.contains( "op" ) || !step.at( "op" ).is_string() )
            throw std::invalid_argument( "expected op string" );

        const std::string op = step.at( "op" ).get<std::string>();
        const int id = Integer( step, "id", 0, 15 );

        if( op == "compare" )
        {
            Keys( step, { "op", "id", "other" } );
            const int other = Integer( step, "other", 0, 15 );
            MD5_HASH& left = State( states, id );
            MD5_HASH& right = State( states, other );
            events.push_back( { { "equal", left == right }, { "not_equal", left != right } } );
            continue;
        }

        if( op == "new" )
        {
            Keys( step, { "op", "id" } );
            states[id] = std::make_unique<MD5_HASH>();
        }
        else if( op == "clone" || op == "assign" )
        {
            Keys( step, { "op", "id", "source" } );
            const int source = Integer( step, "source", 0, 15 );

            if( op == "clone" )
                states[id] = std::make_unique<MD5_HASH>( State( states, source ) );
            else
                State( states, id ) = State( states, source );
        }
        else
        {
            MD5_HASH& hash = State( states, id );

            if( op == "init" || op == "finalize" || op == "inspect" )
            {
                Keys( step, { "op", "id" } );

                if( op == "init" )
                    hash.Init();
                else if( op == "finalize" )
                    hash.Finalize();
            }
            else if( op == "valid" )
            {
                Keys( step, { "op", "id", "value" } );

                if( !step.at( "value" ).is_boolean() )
                    throw std::invalid_argument( "expected boolean value" );

                hash.SetValid( step.at( "value" ).get<bool>() );
            }
            else if( op == "int" )
            {
                Keys( step, { "op", "id", "value" } );
                const int value = Integer( step, "value", INT_MIN, INT_MAX );
                total += 4;

                if( total > MAX_BYTES )
                    throw std::invalid_argument( "byte budget exceeded" );

                hash.Hash( value );
            }
            else if( op == "bytes" )
            {
                Keys( step, { "op", "id", "data" }, { "repeat" } );
                const JSON& array = step.at( "data" );

                if( !array.is_array() )
                    throw std::invalid_argument( "expected byte array" );

                if( array.size() > 65536 )
                    throw std::invalid_argument( "byte array too large" );

                std::vector<uint8_t> data;

                for( const JSON& value : array )
                {
                    if( !value.is_number_integer()
                        || ( value.is_number_unsigned() ? value.get<uint64_t>() > 255
                                                       : value.get<int64_t>() < 0
                                                                 || value.get<int64_t>() > 255 ) )
                    {
                        throw std::invalid_argument( "byte out of range" );
                    }

                    data.push_back( value.get<uint8_t>() );
                }

                const int repeat = step.contains( "repeat" ) ? Integer( step, "repeat", 0, 8193 ) : 1;
                total += data.size() * static_cast<uint64_t>( repeat );

                if( total > MAX_BYTES )
                    throw std::invalid_argument( "byte budget exceeded" );

                // Empty Hash calls read no bytes; provide a real address rather than a null pointer.
                uint8_t empty = 0;

                for( int count = 0; count < repeat; ++count )
                    hash.Hash( data.empty() ? &empty : data.data(),
                               static_cast<uint32_t>( data.size() ) );
            }
            else
                throw std::invalid_argument( "unknown operation" );
        }

        events.push_back( Snapshot( State( states, id ), id ) );
    }

    return { { "events", std::move( events ) } };
}


void RegisterMd5( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.src.md5_hash.application.pure.p001",
                   "libs/kimath/src/md5_hash.cpp:MD5_HASH", Md5 );
}
} // namespace oracle
