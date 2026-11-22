// SPDX-License-Identifier: GPL-3.0-or-later
#include "protocol.h"
#include <cmath>
#include <iostream>

int main()
{
    const oracle::REGISTRY registry;
    std::string line;

    while( std::getline( std::cin, line ) )
    {
        oracle::JSON reply = oracle::Dispatch( line, registry );
        oracle::JSON observation{ { "reply", reply } };

        if( reply["error"]["kind"] == "unknown_unit" )
        {
            const auto request = oracle::JSON::parse( line );
            observation["input_json"] = request["input"].dump();

            if( request["input"].contains( "n" ) && request["input"]["n"].is_number() )
            {
                const auto& number = request["input"]["n"];
                observation["integer"] = number.is_number_integer();
                observation["unsigned"] = number.is_number_unsigned();
                observation["negative_zero"] = number.is_number_float()
                                               && number.get<double>() == 0.0
                                               && std::signbit( number.get<double>() );
            }
        }

        std::cout << observation.dump() << '\n';
    }
}
