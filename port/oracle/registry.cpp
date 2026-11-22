// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"

namespace oracle
{
void RegisterExample( REGISTRY& aRegistry );
void RegisterMd5( REGISTRY& aRegistry );
void RegisterIec60664( REGISTRY& aRegistry );
void RegisterResistors( REGISTRY& aRegistry );
void RegisterRtreeNode( REGISTRY& aRegistry );


void REGISTRY::Add( const std::string& aUnit, const std::string& aUpstream, SHIM aShim )
{
    if( aUnit.empty() || aUpstream.empty() || !aShim
        || !m_units.emplace( aUnit, UNIT{ aUpstream, aShim } ).second )
    {
        throw std::logic_error( "invalid or duplicate oracle registration: " + aUnit );
    }
}


REGISTRY MakeRegistry()
{
    REGISTRY registry;
    RegisterExample( registry );
    RegisterMd5( registry );
    RegisterIec60664( registry );
    RegisterResistors( registry );
    RegisterRtreeNode( registry );
    return registry;
}


int Integer( const JSON& aInput, const std::string& aName, int aMin, int aMax )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() )
        throw std::invalid_argument( "integer out of range: " + aName );

    if( value.is_number_unsigned() )
    {
        const uint64_t number = value.get<uint64_t>();

        if( aMax < 0 || number > static_cast<uint64_t>( aMax )
            || ( aMin > 0 && number < static_cast<uint64_t>( aMin ) ) )
        {
            throw std::invalid_argument( "integer out of range: " + aName );
        }

        return static_cast<int>( number );
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        throw std::invalid_argument( "integer out of range: " + aName );

    return static_cast<int>( number );
}
} // namespace oracle
