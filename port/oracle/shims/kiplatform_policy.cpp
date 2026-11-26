// SPDX-License-Identifier: GPL-3.0-or-later
// Adapter for libs.kiplatform.os.unix.application.thin.replacement.p001. The
// pinned libs/kiplatform/os/unix/policy.cpp is compiled unchanged into the oracle
// (CMakeLists.txt); this TU only builds a genuine wxString key from the request's
// wide code units and calls the reference functions through their declarations.
#include "registry.h"
#include <kiplatform/policy.h>
#include <wx/string.h>
#include <cstdint>
#include <cwchar>
#include <limits>
#include <stdexcept>
#include <string>

namespace oracle
{
// A key unit is one wchar_t of wxString's storage. The corpus is defined for the
// pinned wxGTK build, whose wxString stores signed 32-bit wchar_t units; keys are
// encoded as the accepted map_helpers adapter encodes wxString (one int32 per unit).
static_assert( wxUSE_UNICODE_WCHAR && !wxUSE_UNICODE_UTF8 && sizeof( wchar_t ) == 4
                       && WCHAR_MIN < 0,
               "policy keys require wxString with signed 32-bit wchar_t storage" );

static constexpr size_t MAX_KEY_UNITS = 4096;


static const char* PboolName( KIPLATFORM::POLICY::PBOOL aValue )
{
    switch( aValue )
    {
    case KIPLATFORM::POLICY::PBOOL::ENABLED:        return "ENABLED";
    case KIPLATFORM::POLICY::PBOOL::DISABLED:       return "DISABLED";
    case KIPLATFORM::POLICY::PBOOL::NOT_CONFIGURED: return "NOT_CONFIGURED";
    }

    throw std::logic_error( "unknown PBOOL value" );
}


static JSON Policy( const JSON& aInput )
{
    if( aInput.size() != 2 || !aInput.contains( "op" ) || !aInput.contains( "key" ) )
        throw std::invalid_argument( "unexpected or missing field" );

    if( !aInput.at( "op" ).is_string() )
        throw std::invalid_argument( "expected op string" );

    const std::string op = aInput.at( "op" ).get<std::string>();
    const JSON&       array = aInput.at( "key" );

    if( !array.is_array() )
        throw std::invalid_argument( "expected key array" );

    if( array.size() > MAX_KEY_UNITS )
        throw std::invalid_argument( "key longer than 4096 units" );

    std::wstring units;

    for( const JSON& value : array )
    {
        if( !value.is_number_integer()
            || ( value.is_number_unsigned()
                         ? value.get<uint64_t>() > uint64_t( std::numeric_limits<int32_t>::max() )
                         : value.get<int64_t>() < std::numeric_limits<int32_t>::min()
                                   || value.get<int64_t>() > std::numeric_limits<int32_t>::max() ) )
        {
            throw std::invalid_argument( "key unit out of range" );
        }

        units.push_back( static_cast<wchar_t>( value.get<int32_t>() ) );
    }

    // The genuine key: every unit, including NULs, by explicit length.
    const wxString key( units.data(), units.size() );

    if( key.length() != units.size() || std::wstring( key.wc_str(), key.length() ) != units )
        throw std::logic_error( "wxString did not retain every key unit" );

    JSON value;

    if( op == "get_policy_bool" )
    {
        value = PboolName( KIPLATFORM::POLICY::GetPolicyBool( key ) );
    }
    else if( op == "get_policy_enum_uint" )
    {
        const std::optional<std::uint32_t> result = KIPLATFORM::POLICY::GetPolicyEnumUInt( key );
        value = result ? JSON( *result ) : JSON( nullptr );
    }
    else
    {
        throw std::invalid_argument( "unknown operation" );
    }

    return { { "key_length", key.length() }, { "value", std::move( value ) } };
}


void RegisterKiplatformPolicy( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kiplatform.os.unix.application.thin.replacement.p001",
                   "libs/kiplatform/os/unix/policy.cpp:KIPLATFORM::POLICY", Policy );
}
} // namespace oracle
