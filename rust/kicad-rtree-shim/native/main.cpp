// SPDX-License-Identifier: GPL-3.0-or-later
// Small standalone host for this header-only unit. Uses the shared real protocol.
#include "protocol.h"
#include <iostream>
namespace oracle
{
void RegisterDynamicRtree( REGISTRY& );
void REGISTRY::Add( const std::string& unit, const std::string& source, SHIM shim )
{
    if( unit.empty() || !shim || !m_units.emplace( unit, UNIT{ source, shim } ).second )
        throw std::logic_error( "duplicate registration" );
}
} // namespace oracle
int main()
{
    oracle::REGISTRY registry;
    oracle::RegisterDynamicRtree( registry );
    std::string line;
    while( std::getline( std::cin, line ) )
        std::cout << oracle::Dispatch( line, registry ).dump() << std::endl;
    return std::cin.bad() ? 1 : 0;
}
