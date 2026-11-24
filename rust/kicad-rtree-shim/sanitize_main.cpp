// SPDX-License-Identifier: GPL-3.0-or-later
// Diagnostic driver only: real protocol + registered owning-unit native adapter.
// Built separately from the audited oracle, never eligible as golden provenance.
#include "protocol.h"
#include <iostream>
namespace oracle { void RegisterDynamicRtree( REGISTRY& ); }
int main()
{
    oracle::REGISTRY registry;
    oracle::RegisterDynamicRtree( registry );
    std::string line;
    while( std::getline( std::cin, line ) )
        std::cout << oracle::Dispatch( line, registry ).dump() << std::endl;
    return std::cin.bad() || !std::cout ? 1 : 0;
}
