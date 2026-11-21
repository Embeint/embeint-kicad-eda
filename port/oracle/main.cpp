// SPDX-License-Identifier: GPL-3.0-or-later
#include "protocol.h"
#include "oracle_provenance.h"
#include <iostream>
#include <string>

int main( int argc, char** argv )
{
    try
    {
        const oracle::REGISTRY registry = oracle::MakeRegistry();

        if( argc == 2 && std::string( argv[1] ) == "--provenance" )
        {
            std::cout << ORACLE_PROVENANCE << '\n';
            return 0;
        }

        if( argc == 2 && std::string( argv[1] ) == "--list" )
        {
            oracle::JSON units = oracle::JSON::array();

            for( const auto& [id, entry] : registry.Units() )
                units.push_back( { { "unit", id }, { "upstream", entry.upstream } } );

            std::cout << units.dump() << '\n';
            return 0;
        }

        if( argc != 1 )
        {
            std::cerr << "usage: kicad-oracle [--list|--provenance]\n";
            return 2;
        }

        std::string line;

        while( std::getline( std::cin, line ) )
            std::cout << oracle::Dispatch( line, registry ).dump() << std::endl;

        return std::cin.bad() || !std::cout ? 1 : 0;
    }
    catch( const std::exception& error )
    {
        std::cerr << "oracle startup failure: " << error.what() << '\n';
        return 1;
    }
}
