// SPDX-License-Identifier: GPL-3.0-or-later
#include "protocol.h"
#include <limits>

namespace
{
oracle::JSON Deferred( const oracle::JSON& )
{
    throw oracle::UNPORTED( "upstream/synthetic-dependency", "test.dependency" );
}


oracle::JSON Failed( const oracle::JSON& )
{
    throw std::runtime_error( "synthetic execution failure" );
}


oracle::JSON NonFinite( const oracle::JSON& )
{
    return { { "nested", { std::numeric_limits<double>::infinity() } } };
}


oracle::JSON BadString( const oracle::JSON& )
{
    return std::string( 1, static_cast<char>( 0xFF ) );
}
} // namespace


int main()
{
    oracle::REGISTRY registry;
    registry.Add( "test.deferred", "synthetic", Deferred );
    registry.Add( "test.failed", "synthetic", Failed );
    registry.Add( "test.nonfinite", "synthetic", NonFinite );
    registry.Add( "test.badstring", "synthetic", BadString );

    const oracle::JSON deferred = oracle::Dispatch(
            R"({"unit":"test.deferred","case":"c","input":{}})", registry );
    const oracle::JSON expected = {
        { "unit", "test.deferred" }, { "case", "c" },
        { "error", { { "kind", "unported" }, { "upstream", "upstream/synthetic-dependency" },
                     { "unit", "test.dependency" } } }
    };

    if( deferred != expected )
        return 1;

    for( const std::string& unit : { "test.failed", "test.nonfinite", "test.badstring" } )
    {
        const oracle::JSON request = { { "unit", unit }, { "case", "c" },
                                      { "input", oracle::JSON::object() } };
        const auto reply = oracle::Dispatch( request.dump(), registry );

        if( reply.at( "error" ).at( "kind" ) != "execution_error" )
            return 2;
    }

    try
    {
        registry.Add( "test.failed", "synthetic", Failed );
        return 3;
    }
    catch( const std::logic_error& )
    {
    }

    return 0;
}
