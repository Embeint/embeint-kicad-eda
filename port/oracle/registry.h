// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <nlohmann/json.hpp>
#include <map>
#include <stdexcept>
#include <string>
#include <utility>

namespace oracle
{
using JSON = nlohmann::json;
using SHIM = JSON ( * )( const JSON& );

struct UNIT
{
    std::string upstream;
    SHIM        run;
};

class UNPORTED : public std::runtime_error
{
public:
    UNPORTED( std::string aUpstream, std::string aUnit ) :
            std::runtime_error( "unported" ), upstream( std::move( aUpstream ) ),
            unit( std::move( aUnit ) )
    {}

    std::string upstream;
    std::string unit;
};

class REGISTRY
{
public:
    void Add( const std::string& aUnit, const std::string& aUpstream, SHIM aShim );
    const std::map<std::string, UNIT>& Units() const { return m_units; }

private:
    std::map<std::string, UNIT> m_units;
};

// Each shims/*.cpp supplies one function. Call it explicitly in registry.cpp.
REGISTRY MakeRegistry();

// Require a JSON integer before conversion; nlohmann get<int>() alone can narrow.
int Integer( const JSON& aInput, const std::string& aName, int aMin, int aMax );
} // namespace oracle
