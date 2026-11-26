// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.include.core.application.thin.replacement.p001.
// It instantiates the pinned libs/core/include/core/map_helpers.h templates and
// calls its inline overloads with the distribution wxWidgets 3.2.4 (wchar_t build).
// Only validation, conversion and serialization live here; no reference algorithm
// is copied. Calls without default arguments go through a function pointer of the
// overload's exact signature, so the overload each operation names is the one that
// runs; default-argument calls are written as callers write them.
#include "registry.h"
#include <core/map_helpers.h>
#include <wx/string.h>
#include <algorithm>
#include <climits>
#include <cstdint>
#include <cwchar>
#include <initializer_list>
#include <map>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

#if !wxUSE_UNICODE_WCHAR
#error "map_helpers cases model the wchar_t wxString build"
#endif

static_assert( sizeof( wchar_t ) == 4 && WCHAR_MIN < 0 && sizeof( int ) == 4,
               "map_helpers cases encode wxString as signed 32-bit wchar_t units" );

namespace
{
using oracle::JSON;
using WX_MAP = std::map<wxString, wxString>;

// The two const char* key get_def overloads have identical conversions for a
// two-argument call, so their default arguments are unusable: the call is ambiguous.
template <typename MAP>
concept TWO_ARG_CSTR_GET_DEF = requires( const MAP& aMap, const char* aKey ) {
    get_def( aMap, aKey );
};
template <typename MAP>
concept THREE_ARG_CSTR_GET_DEF = requires( const MAP& aMap, const char* aKey, const char* aDef,
                                           const wxString& aWxDef ) {
    get_def( aMap, aKey, aDef );
    get_def( aMap, aKey, aWxDef );
};
static_assert( !TWO_ARG_CSTR_GET_DEF<WX_MAP>, "two-argument const char* get_def is ambiguous" );
static_assert( THREE_ARG_CSTR_GET_DEF<WX_MAP>, "three-argument const char* get_def resolves" );

constexpr size_t MAX_ENTRIES = 256;
constexpr size_t MAX_WIDE = 4096;
constexpr size_t MAX_BYTES = 4096;

[[noreturn]] void Fail( const char* aMessage )
{
    throw std::invalid_argument( aMessage );
}


void Fields( const JSON& aValue, std::initializer_list<const char*> aRequired,
             std::initializer_list<const char*> aOptional = {} )
{
    if( !aValue.is_object() )
        Fail( "unexpected or missing field" );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            Fail( "unexpected or missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        auto same = [&]( const char* aName ) { return key == aName; };

        if( std::none_of( aRequired.begin(), aRequired.end(), same )
            && std::none_of( aOptional.begin(), aOptional.end(), same ) )
        {
            Fail( "unexpected or missing field" );
        }
    }
}


/// A JSON integer token within [aMin, aMax] (aMax >= 0).
bool Integer( const JSON& aValue, int64_t aMin, int64_t aMax, int64_t& aOut )
{
    if( !aValue.is_number_integer() )
        return false;

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > static_cast<uint64_t>( aMax ) )
            return false;

        aOut = static_cast<int64_t>( number );
        return aOut >= aMin;
    }

    aOut = aValue.get<int64_t>();
    return aOut >= aMin && aOut <= aMax;
}


template <typename T>
T Parse( const JSON& aValue );


template <>
int Parse<int>( const JSON& aValue )
{
    int64_t number = 0;

    if( !Integer( aValue, INT_MIN, INT_MAX, number ) )
        Fail( "expected int" );

    return static_cast<int>( number );
}


/// wxString( const wchar_t*, size_t ): every unit, including NULs and surrogates.
template <>
wxString Parse<wxString>( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() > MAX_WIDE )
        Fail( "expected wide string" );

    std::vector<wchar_t> units;

    for( const JSON& unit : aValue )
    {
        int64_t number = 0;

        if( !Integer( unit, INT32_MIN, INT32_MAX, number ) )
            Fail( "expected wide string" );

        units.push_back( static_cast<wchar_t>( number ) );
    }

    if( units.empty() )
        return wxString();

    return wxString( units.data(), units.size() );
}


JSON Emit( int aValue )
{
    return aValue;
}


JSON Emit( const wxString& aValue )
{
    JSON                     units = JSON::array();
    const wxStringCharType* data = aValue.wx_str();

    for( size_t i = 0; i < aValue.length(); ++i )
        units.push_back( static_cast<int32_t>( data[i] ) );

    return units;
}


/// A const char* argument: null, or the given bytes followed by a terminator. The
/// callee reads only up to the first NUL.
struct CSTR
{
    bool        null = true;
    std::string bytes;

    const char* Get() const { return null ? nullptr : bytes.c_str(); }
};


CSTR Bytes( const JSON& aValue )
{
    CSTR result;

    if( aValue.is_null() )
        return result;

    if( !aValue.is_array() || aValue.size() > MAX_BYTES )
        Fail( "expected bytes" );

    result.null = false;

    for( const JSON& byte : aValue )
    {
        int64_t number = 0;

        if( !Integer( byte, 0, 255, number ) )
            Fail( "expected bytes" );

        result.bytes.push_back( static_cast<char>( static_cast<unsigned char>( number ) ) );
    }

    return result;
}


/// Insert in request order with emplace: the first entry for a key is kept.
template <typename K, typename V>
std::map<K, V> ParseMap( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() > MAX_ENTRIES )
        Fail( "expected map" );

    std::map<K, V> map;

    for( const JSON& entry : aValue )
    {
        if( !entry.is_array() || entry.size() != 2 )
            Fail( "expected map entry" );

        K key = Parse<K>( entry.at( 0 ) );
        V value = Parse<V>( entry.at( 1 ) );
        map.emplace( std::move( key ), std::move( value ) );
    }

    return map;
}


template <typename V>
JSON Optional( const std::optional<V>& aValue )
{
    if( !aValue )
        return JSON{ { "found", false }, { "value", nullptr } };

    return JSON{ { "found", true }, { "value", Emit( *aValue ) } };
}


enum class TYPE
{
    INT,
    WX
};


TYPE Type( const JSON& aInput, const char* aName, const char* aMessage )
{
    if( aInput.contains( aName ) && aInput.at( aName ).is_string() )
    {
        const std::string type = aInput.at( aName ).get<std::string>();

        if( type == "int" )
            return TYPE::INT;

        if( type == "wx" )
            return TYPE::WX;
    }

    Fail( aMessage );
}


TYPE KeyType( const JSON& aInput )
{
    return Type( aInput, "key_type", "expected type: key_type" );
}


TYPE ValueType( const JSON& aInput )
{
    return Type( aInput, "value_type", "expected type: value_type" );
}


template <typename K, typename V>
JSON MapOrder( const JSON& aInput )
{
    const std::map<K, V> map = ParseMap<K, V>( aInput.at( "map" ) );
    JSON                 keys = JSON::array();
    JSON                 values = JSON::array();

    for( const auto& [key, value] : map )
    {
        keys.push_back( Emit( key ) );
        values.push_back( Emit( value ) );
    }

    return JSON{ { "keys", keys }, { "values", values } };
}


template <typename V>
JSON GetOptWx( const JSON& aInput )
{
    const std::map<wxString, V> map = ParseMap<wxString, V>( aInput.at( "map" ) );
    const wxString              key = Parse<wxString>( aInput.at( "key" ) );
    std::optional<V> ( *fn )( const std::map<wxString, V>&, const wxString& ) = &get_opt<V>;
    return Optional( fn( map, key ) );
}


template <typename V>
JSON GetOptCstr( const JSON& aInput )
{
    const std::map<wxString, V> map = ParseMap<wxString, V>( aInput.at( "map" ) );
    const CSTR                  key = Bytes( aInput.at( "key" ) );
    std::optional<V> ( *fn )( const std::map<wxString, V>&, const char* ) = &get_opt<V>;
    return Optional( fn( map, key.Get() ) );
}


template <typename K, typename V>
JSON GetOpt( const JSON& aInput )
{
    const std::map<K, V> map = ParseMap<K, V>( aInput.at( "map" ) );
    const K              key = Parse<K>( aInput.at( "key" ) );
    std::optional<V> ( *fn )( const std::map<K, V>&, const K& ) = &get_opt<K, V>;
    return Optional( fn( map, key ) );
}


template <typename K, typename V>
JSON GetDef( const JSON& aInput )
{
    const std::map<K, V> map = ParseMap<K, V>( aInput.at( "map" ) );
    const K              key = Parse<K>( aInput.at( "key" ) );

    if( !aInput.contains( "defval" ) )
    {
        // The default argument V(), as callers write it. With a wxString key the
        // non-template get_def would win; explicit arguments select the template.
        if constexpr( std::is_same_v<K, wxString> )
            return JSON{ { "value", Emit( get_def<K, V>( map, key ) ) } };
        else
            return JSON{ { "value", Emit( get_def( map, key ) ) } };
    }

    const V defval = Parse<V>( aInput.at( "defval" ) );
    V ( *fn )( const std::map<K, V>&, const K&, const V& ) = &get_def<K, V>;
    return JSON{ { "value", Emit( fn( map, key, defval ) ) } };
}


JSON Run( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        Fail( "expected string: op" );

    const std::string op = aInput.at( "op" ).get<std::string>();

    if( op == "from_utf8" )
    {
        Fields( aInput, { "op", "text" } );
        const CSTR text = Bytes( aInput.at( "text" ) );
        return JSON{ { "value", Emit( wxString::FromUTF8( text.Get() ) ) } };
    }

    if( op == "map_order" )
    {
        Fields( aInput, { "op", "key_type", "value_type", "map" } );
        const TYPE key = KeyType( aInput );
        const TYPE value = ValueType( aInput );

        if( key == TYPE::INT )
            return value == TYPE::INT ? MapOrder<int, int>( aInput ) : MapOrder<int, wxString>( aInput );

        return value == TYPE::INT ? MapOrder<wxString, int>( aInput )
                                  : MapOrder<wxString, wxString>( aInput );
    }

    if( op == "get_opt_wx" )
    {
        Fields( aInput, { "op", "value_type", "map", "key" } );
        return ValueType( aInput ) == TYPE::INT ? GetOptWx<int>( aInput ) : GetOptWx<wxString>( aInput );
    }

    if( op == "get_opt_cstr" )
    {
        Fields( aInput, { "op", "value_type", "map", "key" } );
        return ValueType( aInput ) == TYPE::INT ? GetOptCstr<int>( aInput )
                                                : GetOptCstr<wxString>( aInput );
    }

    if( op == "get_opt" )
    {
        Fields( aInput, { "op", "key_type", "value_type", "map", "key" } );
        const TYPE key = KeyType( aInput );
        const TYPE value = ValueType( aInput );

        if( key == TYPE::INT )
            return value == TYPE::INT ? GetOpt<int, int>( aInput ) : GetOpt<int, wxString>( aInput );

        return value == TYPE::INT ? GetOpt<wxString, int>( aInput ) : GetOpt<wxString, wxString>( aInput );
    }

    if( op == "get_def_cstr" )
    {
        Fields( aInput, { "op", "map", "key", "defval" } );
        const WX_MAP map = ParseMap<wxString, wxString>( aInput.at( "map" ) );
        const CSTR   key = Bytes( aInput.at( "key" ) );
        const CSTR   defval = Bytes( aInput.at( "defval" ) );
        wxString ( *fn )( const WX_MAP&, const char*, const char* ) = &get_def;
        return JSON{ { "value", Emit( fn( map, key.Get(), defval.Get() ) ) } };
    }

    if( op == "get_def_cstr_wx" )
    {
        Fields( aInput, { "op", "map", "key", "defval" } );
        const WX_MAP   map = ParseMap<wxString, wxString>( aInput.at( "map" ) );
        const CSTR     key = Bytes( aInput.at( "key" ) );
        const wxString defval = Parse<wxString>( aInput.at( "defval" ) );
        wxString ( *fn )( const WX_MAP&, const char*, const wxString& ) = &get_def;
        return JSON{ { "value", Emit( fn( map, key.Get(), defval ) ) } };
    }

    if( op == "get_def_wx" )
    {
        Fields( aInput, { "op", "map", "key" }, { "defval" } );
        const WX_MAP   map = ParseMap<wxString, wxString>( aInput.at( "map" ) );
        const wxString key = Parse<wxString>( aInput.at( "key" ) );

        // The default argument wxString(); overload resolution prefers the
        // non-template over the generic template here.
        if( !aInput.contains( "defval" ) )
            return JSON{ { "value", Emit( get_def( map, key ) ) } };

        const wxString defval = Parse<wxString>( aInput.at( "defval" ) );
        wxString ( *fn )( const WX_MAP&, const wxString&, const wxString& ) = &get_def;
        return JSON{ { "value", Emit( fn( map, key, defval ) ) } };
    }

    if( op == "get_def" )
    {
        Fields( aInput, { "op", "key_type", "value_type", "map", "key" }, { "defval" } );
        const TYPE key = KeyType( aInput );
        const TYPE value = ValueType( aInput );

        if( key == TYPE::INT )
            return value == TYPE::INT ? GetDef<int, int>( aInput ) : GetDef<int, wxString>( aInput );

        return value == TYPE::INT ? GetDef<wxString, int>( aInput ) : GetDef<wxString, wxString>( aInput );
    }

    Fail( "unknown op" );
}
} // namespace


namespace oracle
{
void RegisterCoreMapHelpers( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.core.include.core.application.thin.replacement.p001",
                   "libs/core/include/core/map_helpers.h", Run );
}
} // namespace oracle
