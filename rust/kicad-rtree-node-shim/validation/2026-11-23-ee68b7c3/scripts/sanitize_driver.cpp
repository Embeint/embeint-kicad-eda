// SPDX-License-Identifier: GPL-3.0-or-later
// Focused sanitizer probe: the real protocol, registry and rtree_node adapter with the
// pinned headers, registering only this unit.  Not a replacement for the audited oracle.
#include "protocol.h"
#include <iostream>

namespace oracle
{
void RegisterRtreeNode( REGISTRY& aRegistry );
void RegisterExample( REGISTRY& ) {}
void RegisterMd5( REGISTRY& ) {}
void RegisterIec60664( REGISTRY& ) {}
void RegisterResistors( REGISTRY& ) {}
} // namespace oracle

int main()
{
    oracle::REGISTRY registry;
    oracle::RegisterRtreeNode( registry );
    std::string line;

    while( std::getline( std::cin, line ) )
        std::cout << oracle::Dispatch( line, registry ).dump() << std::endl;

    return std::cin.bad() || !std::cout ? 1 : 0;
}
