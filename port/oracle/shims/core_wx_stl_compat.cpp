// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.application.thin.replacement.p001: the four bodies of
// the pinned libs/core/wx_stl_compat.cpp, compiled into KiCad's own core library and linked
// here (std::less<wxPoint>::operator(), TowxString and the wxSize/wxPoint stream insertions),
// on the distribution wxWidgets 3.2 wchar_t build. Only validation, conversion, the caller's
// stream setup and serialization live here; every result comes from the library bodies.
// The schema is in rust/kicad-wx-stl-compat-shim/README.md; the Rust adapter mirrors this
// file's validation order and messages.
#include "registry.h"
#include <core/wx_stl_compat.h>
#include <wx/gdicmn.h>
#include <wx/string.h>
#include <algorithm>
#include <climits>
#include <clocale>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <ios>
#include <locale>
#include <map>
#include <ostream>
#include <stdexcept>
#include <streambuf>
#include <string>
#include <string_view>
#include <type_traits>
#include <vector>

#if !wxUSE_UNICODE_WCHAR
#error "TowxString cases model the wchar_t wxString build"
#endif

static_assert( sizeof( wchar_t ) == 4 && WCHAR_MIN < 0 && sizeof( int ) == 4,
               "wxString results are encoded as signed 32-bit wchar_t units" );
static_assert( std::is_same_v<decltype( wxPoint::x ), int> && std::is_same_v<decltype( wxSize::x ), int> );
static_assert( CHAR_MIN < 0, "numpunct grouping bytes are signed chars on the reference platform" );
static_assert( sizeof( std::streamsize ) == 8, "widths and precisions are 64-bit" );

namespace
{
using oracle::JSON;
constexpr const char* WX_STL_COMPAT_UNIT = "libs.core.application.thin.replacement.p001";

constexpr size_t  MAX_ENTRIES = 256;
constexpr size_t  MAX_VIEW = 4096;
constexpr size_t  MAX_GROUPING = 16;
constexpr size_t  MAX_SINK_CAPACITY = size_t( 1 ) << 20;
constexpr int64_t MAX_UNBOUNDED_WIDTH = int64_t( 1 ) << 20;

[[noreturn]] void Fail( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}


void Keys( const JSON& aValue, std::initializer_list<const char*> aFields, const char* aMessage )
{
    if( !aValue.is_object() || aValue.size() != aFields.size() )
        Fail( aMessage );

    for( const char* field : aFields )
    {
        if( !aValue.contains( field ) )
            Fail( aMessage );
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


int Int32( const JSON& aValue, const char* aMessage )
{
    int64_t number = 0;

    if( !Integer( aValue, INT_MIN, INT_MAX, number ) )
        Fail( aMessage );

    return static_cast<int>( number );
}


std::pair<int, int> Point( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        Fail( "expected point" );

    const int x = Int32( aValue.at( 0 ), "component out of range" );
    const int y = Int32( aValue.at( 1 ), "component out of range" );
    return { x, y };
}


/// Lowercase hexadecimal bytes. Returns false for null (a view with a null data()).
bool View( const JSON& aValue, std::string& aBytes )
{
    if( aValue.is_null() )
        return false;

    if( !aValue.is_string() )
        Fail( "expected view" );

    const std::string& text = aValue.get_ref<const std::string&>();

    if( text.size() % 2 != 0 || text.size() > 2 * MAX_VIEW )
        Fail( "expected view" );

    auto nibble = []( char aChar ) -> int
    {
        if( aChar >= '0' && aChar <= '9' )
            return aChar - '0';

        if( aChar >= 'a' && aChar <= 'f' )
            return aChar - 'a' + 10;

        Fail( "expected view" );
    };

    for( size_t i = 0; i < text.size(); i += 2 )
        aBytes.push_back( static_cast<char>( nibble( text[i] ) << 4 | nibble( text[i + 1] ) ) );

    return true;
}


std::string Hex( const std::string& aBytes )
{
    static const char digits[] = "0123456789abcdef";
    std::string       out;

    for( unsigned char byte : aBytes )
    {
        out.push_back( digits[byte >> 4] );
        out.push_back( digits[byte & 0xF] );
    }

    return out;
}


struct NAMED_STATE
{
    const char*        name;
    std::ios::iostate  bit;
};

const NAMED_STATE IOSTATE[] = { { "bad", std::ios::badbit },
                                { "eof", std::ios::eofbit },
                                { "fail", std::ios::failbit } };

struct NAMED_FLAG
{
    const char*        name;
    std::ios::fmtflags bit;
};

const NAMED_FLAG FMTFLAGS[] = {
    { "boolalpha", std::ios::boolalpha }, { "dec", std::ios::dec },
    { "fixed", std::ios::fixed },         { "hex", std::ios::hex },
    { "internal", std::ios::internal },   { "left", std::ios::left },
    { "oct", std::ios::oct },             { "right", std::ios::right },
    { "scientific", std::ios::scientific }, { "showbase", std::ios::showbase },
    { "showpoint", std::ios::showpoint }, { "showpos", std::ios::showpos },
    { "skipws", std::ios::skipws },       { "unitbuf", std::ios::unitbuf },
    { "uppercase", std::ios::uppercase }
};


/// A JSON array of distinct names, as the union of their bits.
template <typename BIT, typename NAMED, size_t N>
BIT Names( const JSON& aValue, const NAMED ( &aNames )[N], const char* aMessage )
{
    if( !aValue.is_array() )
        Fail( aMessage );

    BIT              bits{};
    std::vector<bool> seen( N, false );

    for( const JSON& item : aValue )
    {
        if( !item.is_string() )
            Fail( aMessage );

        const std::string& name = item.get_ref<const std::string&>();
        size_t             index = N;

        for( size_t i = 0; i < N; ++i )
        {
            if( name == aNames[i].name )
                index = i;
        }

        if( index == N || seen[index] )
            Fail( aMessage );

        seen[index] = true;
        bits |= aNames[index].bit;
    }

    return bits;
}


template <typename BIT, typename NAMED, size_t N>
JSON EmitNames( BIT aBits, const NAMED ( &aNames )[N] )
{
    JSON out = JSON::array();

    for( const NAMED& named : aNames )
    {
        if( ( aBits & named.bit ) == named.bit )
            out.push_back( named.name );
    }

    return out;
}


int64_t StreamSize( const JSON& aValue )
{
    int64_t number = 0;

    if( !Integer( aValue, INT64_MIN, INT64_MAX, number ) )
        Fail( "expected streamsize" );

    return number;
}


char Byte( const JSON& aValue, const char* aMessage = "expected byte" )
{
    int64_t number = 0;

    if( !Integer( aValue, 0, 255, number ) )
        Fail( aMessage );

    return static_cast<char>( static_cast<unsigned char>( number ) );
}


/// The caller's numpunct<char>: a separator and grouping string, otherwise classic.
class NUMPUNCT : public std::numpunct<char>
{
public:
    NUMPUNCT( char aSeparator, std::string aGrouping ) :
            m_separator( aSeparator ), m_grouping( std::move( aGrouping ) )
    {}

protected:
    char        do_thousands_sep() const override { return m_separator; }
    std::string do_grouping() const override { return m_grouping; }

private:
    char        m_separator;
    std::string m_grouping;
};


/// The caller's stream buffer: no put area, so sputc and the default xsputn reach
/// overflow() byte by byte; it accepts up to a capacity, then returns eof(). sync() counts
/// its calls and returns -1 when configured to fail.
class SINK : public std::streambuf
{
public:
    SINK( size_t aCapacity, bool aSyncFails ) : m_capacity( aCapacity ), m_syncFails( aSyncFails ) {}

    const std::string& Bytes() const { return m_bytes; }
    uint64_t           SyncCalls() const { return m_syncCalls; }

protected:
    int_type overflow( int_type aChar ) override
    {
        if( traits_type::eq_int_type( aChar, traits_type::eof() ) )
            return traits_type::not_eof( aChar );

        if( m_bytes.size() >= m_capacity )
            return traits_type::eof();

        m_bytes.push_back( traits_type::to_char_type( aChar ) );
        return aChar;
    }

    int sync() override
    {
        ++m_syncCalls;
        return m_syncFails ? -1 : 0;
    }

private:
    size_t      m_capacity;
    bool        m_syncFails;
    std::string m_bytes;
    uint64_t    m_syncCalls = 0;
};


template <typename VALUE>
JSON Insert( const JSON& aInput )
{
    Keys( aInput, { "op", "value", "stream" }, "unexpected or missing field" );
    const auto [x, y] = Point( aInput.at( "value" ) );
    const JSON& s = aInput.at( "stream" );
    Keys( s, { "state", "exceptions", "flags", "width", "precision", "fill", "thousands_sep",
               "grouping", "capacity", "sync_fails" },
          "unexpected or missing stream field" );

    const std::ios::iostate  state = Names<std::ios::iostate>( s.at( "state" ), IOSTATE, "expected iostate" );
    const std::ios::iostate  mask = Names<std::ios::iostate>( s.at( "exceptions" ), IOSTATE, "expected iostate" );
    const std::ios::fmtflags flags = Names<std::ios::fmtflags>( s.at( "flags" ), FMTFLAGS, "expected fmtflags" );
    const int64_t            width = StreamSize( s.at( "width" ) );
    const int64_t            precision = StreamSize( s.at( "precision" ) );
    const char               fill = Byte( s.at( "fill" ) );
    const char               separator = Byte( s.at( "thousands_sep" ) );

    const JSON& groupingJson = s.at( "grouping" );

    if( !groupingJson.is_array() || groupingJson.size() > MAX_GROUPING )
        Fail( "expected grouping" );

    std::string grouping;

    for( const JSON& byte : groupingJson )
        grouping.push_back( Byte( byte, "expected grouping" ) );

    const JSON& capacityJson = s.at( "capacity" );
    bool        bounded = !capacityJson.is_null();
    int64_t     capacity = 0;

    if( bounded && !Integer( capacityJson, 0, static_cast<int64_t>( MAX_SINK_CAPACITY ), capacity ) )
        Fail( "expected capacity" );

    if( !s.at( "sync_fails" ).is_boolean() )
        Fail( "expected sync_fails" );

    const bool syncFails = s.at( "sync_fails" ).get<bool>();

    // Setting such a mask on such a state throws from exceptions( mask ).
    if( state & mask )
        Fail( "state intersects exceptions" );

    // On the reference runtime ~sentry would call pubsync(), fail, and throw badbit from a
    // noexcept destructor: std::terminate.
    if( ( flags & std::ios::unitbuf ) && syncFails && ( mask & std::ios::badbit ) )
        Fail( "sentry destructor would terminate" );

    if( !bounded && width > MAX_UNBOUNDED_WIDTH )
        Fail( "unbounded width exceeds limit" );

    SINK         sink( bounded ? static_cast<size_t>( capacity ) : SIZE_MAX, syncFails );
    std::ostream out( &sink );
    out.imbue( std::locale( std::locale::classic(), new NUMPUNCT( separator, grouping ) ) );
    out.flags( flags );
    out.width( width );
    out.precision( precision );
    out.fill( fill );
    out.clear( state );
    out.exceptions( mask );

    const VALUE value( x, y );
    bool        threw = false;
    bool        same = false;

    try
    {
        std::ostream& ( *insert )( std::ostream&, const VALUE& ) = &operator<<;
        same = &insert( out, value ) == &out;
    }
    catch( const std::ios_base::failure& )
    {
        threw = true;
        same = true;
    }

    return JSON{ { "bytes", Hex( sink.Bytes() ) },
                 { "exceptions", EmitNames( out.exceptions(), IOSTATE ) },
                 { "fill", static_cast<int>( static_cast<unsigned char>( out.fill() ) ) },
                 { "flags", EmitNames( out.flags(), FMTFLAGS ) },
                 { "outcome", threw ? "ios_failure" : "returned" },
                 { "precision", static_cast<int64_t>( out.precision() ) },
                 { "same_stream", same },
                 { "state", EmitNames( out.rdstate(), IOSTATE ) },
                 { "sync_calls", sink.SyncCalls() },
                 { "width", static_cast<int64_t>( out.width() ) } };
}


/// Runs TowxString with the C library's LC_CTYPE set to aLocale, restoring the previous
/// locale afterwards. A locale the C library does not have is a harness failure.
JSON ToWxString( const JSON& aInput )
{
    Keys( aInput, { "op", "ctype", "view" }, "unexpected or missing field" );
    const JSON& ctype = aInput.at( "ctype" );

    if( !ctype.is_string()
        || ( ctype.get_ref<const std::string&>() != "C" && ctype.get_ref<const std::string&>() != "C.UTF-8" ) )
    {
        Fail( "expected ctype" );
    }

    std::string bytes;
    const bool  nonNull = View( aInput.at( "view" ), bytes );
    const std::string_view view = nonNull ? std::string_view( bytes.data(), bytes.size() )
                                          : std::string_view();

    const char* current = std::setlocale( LC_CTYPE, nullptr );
    const std::string previous = current ? current : "C";

    if( !std::setlocale( LC_CTYPE, ctype.get_ref<const std::string&>().c_str() ) )
        throw std::runtime_error( "C library locale unavailable: " + ctype.get<std::string>() );

    wxString text;

    try
    {
        wxString ( *convert )( const std::string_view& ) = &TowxString;
        text = convert( view );
    }
    catch( ... )
    {
        std::setlocale( LC_CTYPE, previous.c_str() );
        throw;
    }

    std::setlocale( LC_CTYPE, previous.c_str() );

    JSON                    units = JSON::array();
    const wxStringCharType* data = text.wx_str();

    for( size_t i = 0; i < text.length(); ++i )
        units.push_back( static_cast<int32_t>( data[i] ) );

    return JSON{ { "units", units } };
}


JSON Run( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        Fail( "expected string: op" );

    const std::string op = aInput.at( "op" ).get<std::string>();

    if( op == "less_point" )
    {
        Keys( aInput, { "op", "a", "b" }, "unexpected or missing field" );
        const auto [ax, ay] = Point( aInput.at( "a" ) );
        const auto [bx, by] = Point( aInput.at( "b" ) );
        const std::less<wxPoint> less;
        return JSON{ { "less", less( wxPoint( ax, ay ), wxPoint( bx, by ) ) } };
    }

    if( op == "point_map" )
    {
        Keys( aInput, { "op", "entries" }, "unexpected or missing field" );
        const JSON& entries = aInput.at( "entries" );

        if( !entries.is_array() || entries.size() > MAX_ENTRIES )
            Fail( "expected entries" );

        // std::map<wxPoint, int> orders and deduplicates its keys with std::less<wxPoint>.
        std::map<wxPoint, int> map;

        for( const JSON& entry : entries )
        {
            if( !entry.is_array() || entry.size() != 2 )
                Fail( "expected entry" );

            const auto [x, y] = Point( entry.at( 0 ) );
            const int value = Int32( entry.at( 1 ), "value out of range" );
            map.emplace( wxPoint( x, y ), value );
        }

        JSON keys = JSON::array();
        JSON values = JSON::array();

        for( const auto& [key, value] : map )
        {
            keys.push_back( JSON::array( { key.x, key.y } ) );
            values.push_back( value );
        }

        return JSON{ { "keys", keys }, { "values", values } };
    }

    if( op == "to_wx_string" )
        return ToWxString( aInput );

    if( op == "insert_size" )
        return Insert<wxSize>( aInput );

    if( op == "insert_point" )
        return Insert<wxPoint>( aInput );

    Fail( "unknown operation" );
}
} // namespace


namespace oracle
{
void RegisterCoreWxStlCompat( REGISTRY& aRegistry )
{
    aRegistry.Add( WX_STL_COMPAT_UNIT, "libs/core/wx_stl_compat.cpp", Run );
}
} // namespace oracle
