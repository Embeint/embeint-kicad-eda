// SPDX-License-Identifier: GPL-3.0-or-later
// Adapter for libs.kiplatform.os.unix.application.ui.p001: the Linux bodies in
// libs/kiplatform/os/unix/{app,environment,io,printing,secrets}.cpp at the pin, compiled
// unchanged into the kiplatform-unix-ui-reference archive (with os/common/io.cpp, which supplies
// MAPPED_FILE::readIntoBuffer).
//
// Every operating-system, GLib/GIO, GTK, poppler, cairo, libsecret and process-state wx call
// those objects make is redirected with --wrap to a fake below. A fake is armed only while a
// case runs: it records the call (its arguments, in order) and answers from the case's host
// script; unarmed, a libc, GLib or wx fake forwards to the real function, and the others are
// never called. wxConvLibcPtr and wxConvFileName are replaced by scripted converters, and a
// recording log target and assertion handler are installed, for the duration of a case. The
// stateful printing bodies (s_lastSettings) and TerminateWithCode run in a forked child.
// Request bounds and messages are identical in the Rust shim (rust/kicad-kiplatform-unix-shim).
#include "registry.h"

#include <kiplatform/app.h>
#include <kiplatform/environment.h>
#include <kiplatform/io.h>
#include <kiplatform/secrets.h>
#include <printing.h>

#include <wx/cmndata.h>
#include <wx/filename.h>
#include <wx/gtk/print.h>
#include <wx/log.h>
#include <wx/string.h>
#include <wx/strconv.h>
#include <wx/window.h>

#include <cairo.h>
#include <dirent.h>
#include <fcntl.h>
#include <gio/gio.h>
#include <glib.h>
#include <gtk/gtk.h>
#include <libsecret/secret.h>
#include <poppler.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cerrno>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <functional>
#include <initializer_list>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

// printing.cpp defines this overload with external linkage; printing.h declares only the
// two-argument one, so the adapter declares it to call it.
namespace KIPLATFORM
{
namespace PRINTING
{
PRINT_RESULT PrintPDF( const std::string& aFile, bool fit_to_page, int aCurrentPage );
} // namespace PRINTING
} // namespace KIPLATFORM

namespace
{
using oracle::JSON;

constexpr const char* UNIX_UI_UNIT = "libs.kiplatform.os.unix.application.ui.p001";
// Request bounds, identical in the Rust shim.
constexpr size_t MAX_UNITS = 1024;
constexpr size_t MAX_BYTES = 4096;
constexpr size_t MAX_REPLIES = 256;
constexpr size_t MAX_ENTRIES = 64;
constexpr size_t MAX_HANDLES = 4000;

static_assert( wxUSE_UNICODE_WCHAR && !wxUSE_UNICODE_UTF8 && sizeof( wchar_t ) == 4,
               "strings are signed 32-bit wchar_t units" );
static_assert( sizeof( off_t ) == 8 && sizeof( time_t ) == 8, "64-bit off_t and time_t" );


// A refusal of the request: the reply is an invalid_input error with this message.
struct REFUSAL : std::invalid_argument
{
    using std::invalid_argument::invalid_argument;
};


struct STATE;
extern STATE g;
const std::string& pendingRefusal();


// A refusal of the request. When one is already pending (from a C library fake), that first
// refusal is raised instead, as the Rust port stops at it.
[[noreturn]] void refuse( const std::string& aMessage )
{
    const std::string& pending = pendingRefusal();
    throw REFUSAL( pending.empty() ? aMessage : pending );
}


std::string hexOf( const void* aData, size_t aSize )
{
    static const char digits[] = "0123456789abcdef";
    const auto*       bytes = static_cast<const unsigned char*>( aData );
    std::string       out;

    for( size_t i = 0; i < aSize; ++i )
    {
        out += digits[bytes[i] >> 4];
        out += digits[bytes[i] & 15];
    }

    return out;
}


std::string hexOf( const std::string& aBytes )
{
    return hexOf( aBytes.data(), aBytes.size() );
}


std::string bitsOf( double aValue )
{
    uint64_t bits;
    std::memcpy( &bits, &aValue, sizeof bits );
    char text[17];
    std::snprintf( text, sizeof text, "%016llx", static_cast<unsigned long long>( bits ) );
    return text;
}


JSON unitsOf( const wxString& aText )
{
    JSON out = JSON::array();

    for( wchar_t unit : aText.ToStdWstring() )
        out.push_back( static_cast<int32_t>( unit ) );

    return out;
}


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields, const char* aWhere )
{
    if( !aInput.is_object() || aInput.size() != aFields.size() )
        refuse( std::string( "unexpected or missing field: " ) + aWhere );

    for( const char* field : aFields )
    {
        if( !aInput.contains( field ) )
            refuse( std::string( "unexpected or missing field: " ) + aWhere );
    }
}


const JSON& member( const JSON& aInput, const char* aName )
{
    if( !aInput.is_object() || !aInput.contains( aName ) )
        refuse( std::string( "missing field: " ) + aName );

    return aInput.at( aName );
}


bool readBool( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_boolean() )
        refuse( "expected boolean: " + aName );

    return aValue.get<bool>();
}


int64_t readInteger( const JSON& aValue, const std::string& aName, int64_t aMin, int64_t aMax )
{
    if( !aValue.is_number_integer() )
        refuse( "expected integer: " + aName );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > static_cast<uint64_t>( aMax ) )
            refuse( "integer out of range: " + aName );

        return static_cast<int64_t>( number );
    }

    const int64_t number = aValue.get<int64_t>();

    if( number < aMin || number > aMax )
        refuse( "integer out of range: " + aName );

    return number;
}


std::string readText( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_string() )
        refuse( "expected string: " + aName );

    return aValue.get<std::string>();
}


// Lower-case hexadecimal bytes, at most MAX_BYTES.
std::string readHex( const JSON& aValue, const std::string& aName )
{
    const std::string text = readText( aValue, aName );

    if( text.size() % 2 != 0 || text.size() / 2 > MAX_BYTES )
        refuse( "bad hex: " + aName );

    std::string out;

    for( size_t i = 0; i < text.size(); i += 2 )
    {
        int value = 0;

        for( size_t j = 0; j < 2; ++j )
        {
            const char c = text[i + j];
            int        digit;

            if( c >= '0' && c <= '9' )
                digit = c - '0';
            else if( c >= 'a' && c <= 'f' )
                digit = c - 'a' + 10;
            else
                refuse( "bad hex: " + aName );

            value = value * 16 + digit;
        }

        out += static_cast<char>( value );
    }

    return out;
}


// Bytes with no NUL: what a C string the fake returns can hold.
std::string readCText( const JSON& aValue, const std::string& aName )
{
    const std::string bytes = readHex( aValue, aName );

    if( bytes.find( '\0' ) != std::string::npos )
        refuse( "NUL in C string: " + aName );

    return bytes;
}


wxString readUnits( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_array() || aValue.size() > MAX_UNITS )
        refuse( "expected unit array: " + aName );

    std::wstring units;

    for( const JSON& unit : aValue )
        units += static_cast<wchar_t>( readInteger( unit, aName, INT32_MIN, INT32_MAX ) );

    return wxString( units.data(), units.size() );
}


double readBits( const JSON& aValue, const std::string& aName )
{
    const std::string text = readText( aValue, aName );

    if( text.size() != 16 )
        refuse( "bad double bits: " + aName );

    uint64_t bits = 0;

    for( char c : text )
    {
        int digit;

        if( c >= '0' && c <= '9' )
            digit = c - '0';
        else if( c >= 'a' && c <= 'f' )
            digit = c - 'a' + 10;
        else
            refuse( "bad double bits: " + aName );

        bits = bits * 16 + static_cast<uint64_t>( digit );
    }

    double value;
    std::memcpy( &value, &bits, sizeof value );
    return value;
}


// ---- Scripted converters ------------------------------------------------------------------

enum class MODE
{
    ASCII,
    LATIN1,
    UTF8,
    FAIL
};


MODE readMode( const JSON& aValue, const std::string& aName )
{
    const std::string text = readText( aValue, aName );

    if( text == "ascii" )
        return MODE::ASCII;
    if( text == "latin1" )
        return MODE::LATIN1;
    if( text == "utf8" )
        return MODE::UTF8;
    if( text == "fail" )
        return MODE::FAIL;

    refuse( "unknown converter mode: " + aName );
}


// The units for bytes, or false. UTF-8 is the shortest-form encoding of U+0000..U+10FFFF
// without surrogates.
bool decodeBytes( MODE aMode, const char* aSrc, size_t aLen, std::vector<wchar_t>& aOut )
{
    const auto* s = reinterpret_cast<const unsigned char*>( aSrc );

    for( size_t i = 0; i < aLen; )
    {
        const unsigned c = s[i];

        if( aMode == MODE::FAIL )
            return false;

        if( aMode == MODE::ASCII )
        {
            if( c >= 0x80 )
                return false;

            aOut.push_back( static_cast<wchar_t>( c ) );
            ++i;
            continue;
        }

        if( aMode == MODE::LATIN1 )
        {
            aOut.push_back( static_cast<wchar_t>( c ) );
            ++i;
            continue;
        }

        size_t   n;
        uint32_t code;
        uint32_t min;

        if( c < 0x80 )
        {
            n = 1;
            code = c;
            min = 0;
        }
        else if( ( c & 0xE0 ) == 0xC0 )
        {
            n = 2;
            code = c & 0x1F;
            min = 0x80;
        }
        else if( ( c & 0xF0 ) == 0xE0 )
        {
            n = 3;
            code = c & 0x0F;
            min = 0x800;
        }
        else if( ( c & 0xF8 ) == 0xF0 )
        {
            n = 4;
            code = c & 0x07;
            min = 0x10000;
        }
        else
        {
            return false;
        }

        if( i + n > aLen )
            return false;

        for( size_t j = 1; j < n; ++j )
        {
            if( ( s[i + j] & 0xC0 ) != 0x80 )
                return false;

            code = ( code << 6 ) | ( s[i + j] & 0x3F );
        }

        if( code < min || code > 0x10FFFF || ( code >= 0xD800 && code <= 0xDFFF ) )
            return false;

        aOut.push_back( static_cast<wchar_t>( code ) );
        i += n;
    }

    return true;
}


bool encodeUnits( MODE aMode, const wchar_t* aSrc, size_t aLen, std::string& aOut )
{
    for( size_t i = 0; i < aLen; ++i )
    {
        const uint32_t code = static_cast<uint32_t>( aSrc[i] );

        if( aMode == MODE::FAIL )
            return false;

        if( aMode == MODE::ASCII || aMode == MODE::LATIN1 )
        {
            if( code > ( aMode == MODE::ASCII ? 0x7Fu : 0xFFu ) )
                return false;

            aOut += static_cast<char>( code );
            continue;
        }

        if( code > 0x10FFFF || ( code >= 0xD800 && code <= 0xDFFF ) )
            return false;

        if( code < 0x80 )
        {
            aOut += static_cast<char>( code );
        }
        else if( code < 0x800 )
        {
            aOut += static_cast<char>( 0xC0 | ( code >> 6 ) );
            aOut += static_cast<char>( 0x80 | ( code & 0x3F ) );
        }
        else if( code < 0x10000 )
        {
            aOut += static_cast<char>( 0xE0 | ( code >> 12 ) );
            aOut += static_cast<char>( 0x80 | ( ( code >> 6 ) & 0x3F ) );
            aOut += static_cast<char>( 0x80 | ( code & 0x3F ) );
        }
        else
        {
            aOut += static_cast<char>( 0xF0 | ( code >> 18 ) );
            aOut += static_cast<char>( 0x80 | ( ( code >> 12 ) & 0x3F ) );
            aOut += static_cast<char>( 0x80 | ( ( code >> 6 ) & 0x3F ) );
            aOut += static_cast<char>( 0x80 | ( code & 0x3F ) );
        }
    }

    return true;
}


// A process converter with a fixed rule; a NUL-terminated input converts its terminator too.
class SCRIPTED_CONV : public wxMBConv
{
public:
    explicit SCRIPTED_CONV( MODE aMode ) : m_mode( aMode ) {}

    size_t ToWChar( wchar_t* aDst, size_t aDstLen, const char* aSrc,
                    size_t aSrcLen = wxNO_LEN ) const override
    {
        const size_t         len = aSrcLen == wxNO_LEN ? std::strlen( aSrc ) + 1 : aSrcLen;
        std::vector<wchar_t> out;

        if( !decodeBytes( m_mode, aSrc, len, out ) )
            return wxCONV_FAILED;

        if( aDst )
        {
            if( out.size() > aDstLen )
                return wxCONV_FAILED;

            std::copy( out.begin(), out.end(), aDst );
        }

        return out.size();
    }

    size_t FromWChar( char* aDst, size_t aDstLen, const wchar_t* aSrc,
                      size_t aSrcLen = wxNO_LEN ) const override
    {
        const size_t len = aSrcLen == wxNO_LEN ? std::wcslen( aSrc ) + 1 : aSrcLen;
        std::string  out;

        if( !encodeUnits( m_mode, aSrc, len, out ) )
            return wxCONV_FAILED;

        if( aDst )
        {
            if( out.size() > aDstLen )
                return wxCONV_FAILED;

            std::copy( out.begin(), out.end(), aDst );
        }

        return out.size();
    }

    wxMBConv* Clone() const override { return new SCRIPTED_CONV( m_mode ); }

private:
    MODE m_mode;
};


// ---- Fake objects ---------------------------------------------------------------------------

// A window wxGetTopLevelParent can return: GetHandle() records the call and answers.
class FAKE_WINDOW : public wxWindow
{
public:
    WXWidget GetHandle() const override;
};


struct PAGE
{
    double width = 0;
    double height = 0;
};


struct CONTEXT
{
    bool   hasCairo = false;
    void*  cairo = nullptr;
    double width = 0;
    double height = 0;
};


struct ITEM
{
    bool        locked = false;
    bool        hasSecret = false;
    bool        hasText = false;
    std::string text;
};


struct PENDING
{
    bool             store = true;
    GAsyncReadyCallback callback = nullptr;
    gpointer         userData = nullptr;
};


struct CONNECTION
{
    std::string signal;
    GCallback   handler = nullptr;
    gpointer    data = nullptr;
};


using SETTINGS = std::map<std::string, std::string>;


struct STATE
{
    bool                                       armed = false;
    JSON                                       calls = JSON::array();
    std::map<std::string, std::deque<JSON>>    replies;
    JSON                                       env = JSON::object();
    bool                                       setenvResult = true;
    bool                                       trace = false;
    MODE                                       libc = MODE::ASCII;
    MODE                                       fileName = MODE::UTF8;
    size_t                                     nextHandle = 1;
    std::map<const void*, std::string>         labels;
    std::map<std::string, const void*>         byLabel;
    std::vector<std::unique_ptr<FAKE_WINDOW>>  windows;
    std::map<const void*, SETTINGS>            settings;
    std::map<const void*, PAGE>                pages;
    std::map<const void*, CONTEXT>             contexts;
    std::map<const void*, ITEM>                items;
    std::map<const void*, const ITEM*>         values;
    std::map<const void*, std::vector<std::string>> lists;
    std::vector<PENDING>                       pending;
    std::vector<CONNECTION>                    connections;
    std::map<const void*, const void*>         opSettings;
    std::map<const void*, std::string>         opResultSettings;
    std::vector<std::unique_ptr<struct dirent64>> dirents;
    std::vector<std::unique_ptr<std::string>>  buffers;
    std::vector<std::unique_ptr<ITEM>>         itemStore;
    bool                                       currentFit = true;
    bool                                       inDrawPage = false;
    // The first refusal raised inside a C library fake. glibc declares many of those
    // functions noexcept (__THROW), so their fakes never throw: they keep the refusal, return a
    // failure, and the operation raises it when the call returns.
    std::string                                refusal;
};

STATE g;


const std::string& pendingRefusal()
{
    return g.refusal;
}


// Run a C library fake's body: a refusal is kept (the first one) and aFallback returned; once
// one is pending, later fakes return aFallback without recording.
template <typename T, typename F>
T guarded( T aFallback, F&& aBody )
{
    if( !g.refusal.empty() )
        return aFallback;

    try
    {
        return aBody();
    }
    catch( const REFUSAL& error )
    {
        g.refusal = error.what();
        return aFallback;
    }
}

alignas( 16 ) unsigned char g_arena[( MAX_HANDLES + 1 ) * 16];


void record( JSON aCall )
{
    g.calls.push_back( std::move( aCall ) );
}


const void* label( const void* aPointer, const std::string& aKind )
{
    if( g.nextHandle > MAX_HANDLES )
        refuse( "too many handles" );

    const std::string name = aKind + "#" + std::to_string( g.nextHandle++ );
    g.labels[aPointer] = name;
    g.byLabel[name] = aPointer;
    return aPointer;
}


void* newHandle( const std::string& aKind )
{
    void* pointer = g_arena + g.nextHandle * 16;
    label( pointer, aKind );
    return pointer;
}


bool known( const void* aPointer )
{
    return g.labels.count( aPointer ) != 0;
}


JSON nameOf( const void* aPointer )
{
    if( !aPointer )
        return nullptr;

    auto it = g.labels.find( aPointer );

    if( it == g.labels.end() )
        return "unknown";

    return it->second;
}


JSON reply( const char* aFunction )
{
    auto it = g.replies.find( aFunction );

    if( it == g.replies.end() || it->second.empty() )
        refuse( std::string( "missing reply: " ) + aFunction );

    JSON value = std::move( it->second.front() );
    it->second.pop_front();
    return value;
}


int64_t replyInteger( const char* aFunction, int64_t aMin, int64_t aMax )
{
    return readInteger( reply( aFunction ), std::string( "reply " ) + aFunction, aMin, aMax );
}


bool replyBool( const char* aFunction )
{
    return readBool( reply( aFunction ), std::string( "reply " ) + aFunction );
}


// A reply that is null or C-string bytes.
bool replyCText( const char* aFunction, std::string& aOut )
{
    const JSON value = reply( aFunction );

    if( value.is_null() )
        return false;

    aOut = readCText( value, std::string( "reply " ) + aFunction );
    return true;
}


GError* newError( const JSON& aMessage, const std::string& aName )
{
    if( aMessage.is_null() )
        return nullptr;

    const std::string message = readCText( aMessage, aName );
    GError*           error = g_error_new_literal( g_quark_from_static_string( "oracle" ), 1,
                                                   message.c_str() );
    label( error, "error" );
    return error;
}


gchar* newGStr( const std::string& aBytes )
{
    gchar* text = static_cast<gchar*>( g_malloc( aBytes.size() + 1 ) );
    std::memcpy( text, aBytes.data(), aBytes.size() );
    text[aBytes.size()] = '\0';
    label( text, "gstr" );
    return text;
}


JSON schemaOf( const SecretSchema* aSchema )
{
    JSON attributes = JSON::array();
    int  terminator = -1;

    for( size_t i = 0; i < 32; ++i )
    {
        if( !aSchema->attributes[i].name )
        {
            terminator = aSchema->attributes[i].type;
            break;
        }

        attributes.push_back( JSON::array( { aSchema->attributes[i].name,
                                             static_cast<int>( aSchema->attributes[i].type ) } ) );
    }

    return { { "name", aSchema->name },
             { "flags", static_cast<int>( aSchema->flags ) },
             { "attributes", attributes },
             { "terminator_type", terminator } };
}


JSON attributesOf( va_list aArgs )
{
    JSON out = JSON::array();

    for( size_t i = 0; i < 32; ++i )
    {
        const char* name = va_arg( aArgs, const char* );

        if( !name )
            break;

        const char* value = va_arg( aArgs, const char* );
        out.push_back( JSON::array( { hexOf( name, std::strlen( name ) ),
                                      hexOf( value, std::strlen( value ) ) } ) );
    }

    return out;
}


[[noreturn]] void unarmed( const char* aFunction )
{
    std::fprintf( stderr, "kiplatform unix ui fake called unarmed: %s\n", aFunction );
    std::abort();
}


WXWidget FAKE_WINDOW::GetHandle() const
{
    record( { { "fn", "GetHandle" }, { "win", nameOf( this ) } } );
    const JSON value = reply( "GetHandle" );

    if( readBool( value, "reply GetHandle" ) )
        return static_cast<WXWidget>( newHandle( "widget" ) );

    return nullptr;
}


struct STAT_REPLY
{
    int         result;
    struct stat buf;
};


STAT_REPLY replyStat( const char* aFunction )
{
    const JSON  value = reply( aFunction );
    const std::string name = std::string( "reply " ) + aFunction;
    checkKeys( value, { "result", "mode", "mtime", "size" }, name.c_str() );
    STAT_REPLY out{};
    out.result = static_cast<int>( readInteger( value.at( "result" ), name, INT32_MIN, INT32_MAX ) );
    out.buf.st_mode = static_cast<mode_t>( readInteger( value.at( "mode" ), name, 0, UINT32_MAX ) );
    out.buf.st_mtime = static_cast<time_t>( readInteger( value.at( "mtime" ), name, INT64_MIN,
                                                         INT64_MAX ) );
    out.buf.st_size = static_cast<off_t>( readInteger( value.at( "size" ), name, INT64_MIN,
                                                       INT64_MAX ) );
    return out;
}
} // namespace


// ---- wrapped functions ----------------------------------------------------------------------

extern "C"
{
// wxWidgets (C++ symbols, wrapped by their mangled names).
bool __real__Z8wxGetEnvRK8wxStringPS_( const wxString& aVar, wxString* aValue );
bool __real__Z8wxSetEnvRK8wxStringS1_( const wxString& aVar, const wxString& aValue );
wxWindow* __real__Z17wxGetActiveWindowv();
wxWindow* __real__Z19wxGetTopLevelParentP12wxWindowBase( wxWindowBase* aWindow );

bool __wrap__Z8wxGetEnvRK8wxStringPS_( const wxString& aVar, wxString* aValue )
{
    if( !g.armed )
        return __real__Z8wxGetEnvRK8wxStringPS_( aVar, aValue );

    record( { { "fn", "wxGetEnv" }, { "var", unitsOf( aVar ) }, { "value_ptr", aValue != nullptr } } );
    const std::string name = aVar.ToStdString( wxConvUTF8 );

    if( !g.env.contains( name ) || g.env.at( name ).is_null() )
        return false;

    if( aValue )
        *aValue = readUnits( g.env.at( name ), "env" );

    return true;
}

bool __wrap__Z8wxSetEnvRK8wxStringS1_( const wxString& aVar, const wxString& aValue )
{
    if( !g.armed )
        return __real__Z8wxSetEnvRK8wxStringS1_( aVar, aValue );

    record( { { "fn", "wxSetEnv" }, { "var", unitsOf( aVar ) }, { "value", unitsOf( aValue ) } } );
    return g.setenvResult;
}

wxWindow* __wrap__Z17wxGetActiveWindowv()
{
    if( !g.armed )
        return __real__Z17wxGetActiveWindowv();

    record( { { "fn", "wxGetActiveWindow" } } );

    if( !replyBool( "wxGetActiveWindow" ) )
        return nullptr;

    g.windows.push_back( std::make_unique<FAKE_WINDOW>() );
    label( g.windows.back().get(), "window" );
    return g.windows.back().get();
}

wxWindow* __wrap__Z19wxGetTopLevelParentP12wxWindowBase( wxWindowBase* aWindow )
{
    if( !g.armed )
        return __real__Z19wxGetTopLevelParentP12wxWindowBase( aWindow );

    record( { { "fn", "wxGetTopLevelParent" }, { "win", nameOf( aWindow ) } } );

    if( !replyBool( "wxGetTopLevelParent" ) )
        return nullptr;

    g.windows.push_back( std::make_unique<FAKE_WINDOW>() );
    label( g.windows.back().get(), "window" );
    return g.windows.back().get();
}

// GLib.
void __real_g_error_free( GError* aError );
void __real_g_clear_error( GError** aError );
void __real_g_object_unref( gpointer aObject );
void __real_g_free( gpointer aMem );
void __real_g_strfreev( gchar** aList );
void __real_g_list_free_full( GList* aList, GDestroyNotify aFree );
void __real_g_hash_table_unref( GHashTable* aTable );
GTypeInstance* __real_g_type_check_instance_cast( GTypeInstance* aInstance, GType aType );

void __wrap_g_log_set_writer_func( GLogWriterFunc aFunc, gpointer aUserData,
                                   GDestroyNotify aUserDataFree )
{
    if( !g.armed )
        unarmed( "g_log_set_writer_func" );

    const int result = aFunc ? static_cast<int>( aFunc( G_LOG_LEVEL_MESSAGE, nullptr, 0, nullptr ) )
                             : -1;
    record( { { "fn", "g_log_set_writer_func" },
              { "writer_result", result },
              { "user_data_null", aUserData == nullptr },
              { "destroy_null", aUserDataFree == nullptr } } );
}

void __wrap_g_error_free( GError* aError )
{
    if( g.armed && known( aError ) )
        record( { { "fn", "g_error_free" }, { "error", nameOf( aError ) } } );

    __real_g_error_free( aError );
}

void __wrap_g_clear_error( GError** aError )
{
    if( g.armed )
        record( { { "fn", "g_clear_error" }, { "error", nameOf( aError ? *aError : nullptr ) } } );

    __real_g_clear_error( aError );
}

void __wrap_g_object_unref( gpointer aObject )
{
    if( g.armed && known( aObject ) )
    {
        record( { { "fn", "g_object_unref" }, { "object", nameOf( aObject ) } } );
        return;
    }

    __real_g_object_unref( aObject );
}

void __wrap_g_free( gpointer aMem )
{
    if( g.armed && ( !aMem || known( aMem ) ) )
        record( { { "fn", "g_free" }, { "mem", nameOf( aMem ) } } );

    __real_g_free( aMem );
}

void __wrap_g_strfreev( gchar** aList )
{
    if( g.armed && known( aList ) )
        record( { { "fn", "g_strfreev" }, { "list", nameOf( aList ) } } );

    __real_g_strfreev( aList );
}

void __wrap_g_list_free_full( GList* aList, GDestroyNotify aFree )
{
    if( !g.armed )
    {
        __real_g_list_free_full( aList, aFree );
        return;
    }

    record( { { "fn", "g_list_free_full" }, { "list", nameOf( aList ) } } );

    for( GList* it = aList; it; it = it->next )
        aFree( it->data );

    g_list_free( aList );
}

void __wrap_g_hash_table_unref( GHashTable* aTable )
{
    if( g.armed && known( aTable ) )
    {
        record( { { "fn", "g_hash_table_unref" }, { "table", nameOf( aTable ) } } );
        return;
    }

    __real_g_hash_table_unref( aTable );
}

GTypeInstance* __wrap_g_type_check_instance_cast( GTypeInstance* aInstance, GType aType )
{
    if( g.armed && known( aInstance ) )
        return aInstance;

    return __real_g_type_check_instance_cast( aInstance, aType );
}

static const gchar* userDir( const char* aFunction )
{
    record( { { "fn", aFunction } } );
    std::string bytes;

    if( !replyCText( aFunction, bytes ) )
        return nullptr;

    g.buffers.push_back( std::make_unique<std::string>( bytes ) );
    return g.buffers.back()->c_str();
}

const gchar* __wrap_g_get_user_data_dir()
{
    if( !g.armed )
        unarmed( "g_get_user_data_dir" );

    return userDir( "g_get_user_data_dir" );
}

const gchar* __wrap_g_get_user_config_dir()
{
    if( !g.armed )
        unarmed( "g_get_user_config_dir" );

    return userDir( "g_get_user_config_dir" );
}

const gchar* __wrap_g_get_user_cache_dir()
{
    if( !g.armed )
        unarmed( "g_get_user_cache_dir" );

    return userDir( "g_get_user_cache_dir" );
}

const gchar* __wrap_g_get_home_dir()
{
    if( !g.armed )
        unarmed( "g_get_home_dir" );

    return userDir( "g_get_home_dir" );
}

gchar* __wrap_g_filename_to_uri( const gchar* aFileName, const gchar* aHostName, GError** aError )
{
    if( !g.armed )
        unarmed( "g_filename_to_uri" );

    record( { { "fn", "g_filename_to_uri" },
              { "filename", hexOf( aFileName, std::strlen( aFileName ) ) },
              { "hostname_null", aHostName == nullptr },
              { "error_null", aError == nullptr } } );
    std::string bytes;

    if( !replyCText( "g_filename_to_uri", bytes ) )
        return nullptr;

    return newGStr( bytes );
}

gulong __wrap_g_signal_connect_data( gpointer aInstance, const gchar* aSignal, GCallback aHandler,
                                     gpointer aData, GClosureNotify aDestroy,
                                     GConnectFlags aFlags )
{
    if( !g.armed )
        unarmed( "g_signal_connect_data" );

    record( { { "fn", "g_signal_connect" },
              { "instance", nameOf( aInstance ) },
              { "signal", aSignal },
              { "destroy_null", aDestroy == nullptr },
              { "flags", static_cast<int>( aFlags ) } } );

    if( !g.connections.empty() && g.connections.front().data != aData )
        throw std::runtime_error( "signal handlers received different data" );

    g.connections.push_back( { aSignal, aHandler, aData } );
    return g.connections.size();
}

// GIO.
GFile* __wrap_g_file_new_for_path( const char* aPath )
{
    if( !g.armed )
        unarmed( "g_file_new_for_path" );

    record( { { "fn", "g_file_new_for_path" }, { "path", hexOf( aPath, std::strlen( aPath ) ) } } );
    return static_cast<GFile*>( newHandle( "file" ) );
}

GFile* __wrap_g_file_new_for_uri( const char* aUri )
{
    if( !g.armed )
        unarmed( "g_file_new_for_uri" );

    record( { { "fn", "g_file_new_for_uri" }, { "uri", hexOf( aUri, std::strlen( aUri ) ) } } );
    return static_cast<GFile*>( newHandle( "file" ) );
}

gboolean __wrap_g_file_trash( GFile* aFile, GCancellable* aCancellable, GError** aError )
{
    if( !g.armed )
        unarmed( "g_file_trash" );

    record( { { "fn", "g_file_trash" },
              { "file", nameOf( aFile ) },
              { "cancellable_null", aCancellable == nullptr },
              { "error_null", aError == nullptr } } );
    const JSON value = reply( "g_file_trash" );
    checkKeys( value, { "result", "error" }, "reply g_file_trash" );
    const bool result = readBool( value.at( "result" ), "reply g_file_trash" );
    GError*    error = newError( value.at( "error" ), "reply g_file_trash" );

    if( aError )
        *aError = error;

    return result;
}

static gpointer optionalHandle( const char* aFunction, const char* aKind )
{
    return replyBool( aFunction ) ? newHandle( aKind ) : nullptr;
}

GMount* __wrap_g_file_find_enclosing_mount( GFile* aFile, GCancellable* aCancellable,
                                            GError** aError )
{
    if( !g.armed )
        unarmed( "g_file_find_enclosing_mount" );

    record( { { "fn", "g_file_find_enclosing_mount" },
              { "file", nameOf( aFile ) },
              { "cancellable_null", aCancellable == nullptr },
              { "error_null", aError == nullptr } } );
    return static_cast<GMount*>( optionalHandle( "g_file_find_enclosing_mount", "mount" ) );
}

GDrive* __wrap_g_mount_get_drive( GMount* aMount )
{
    if( !g.armed )
        unarmed( "g_mount_get_drive" );

    record( { { "fn", "g_mount_get_drive" }, { "mount", nameOf( aMount ) } } );
    return static_cast<GDrive*>( optionalHandle( "g_mount_get_drive", "drive" ) );
}

gboolean __wrap_g_drive_is_removable( GDrive* aDrive )
{
    if( !g.armed )
        unarmed( "g_drive_is_removable" );

    record( { { "fn", "g_drive_is_removable" }, { "drive", nameOf( aDrive ) } } );
    return replyBool( "g_drive_is_removable" );
}

GFile* __wrap_g_file_get_parent( GFile* aFile )
{
    if( !g.armed )
        unarmed( "g_file_get_parent" );

    record( { { "fn", "g_file_get_parent" }, { "file", nameOf( aFile ) } } );
    return static_cast<GFile*>( optionalHandle( "g_file_get_parent", "file" ) );
}

static char* fileString( const char* aFunction, GFile* aFile )
{
    record( { { "fn", aFunction }, { "file", nameOf( aFile ) } } );
    std::string bytes;

    if( !replyCText( aFunction, bytes ) )
        return nullptr;

    return newGStr( bytes );
}

char* __wrap_g_file_get_basename( GFile* aFile )
{
    if( !g.armed )
        unarmed( "g_file_get_basename" );

    return fileString( "g_file_get_basename", aFile );
}

char* __wrap_g_file_get_path( GFile* aFile )
{
    if( !g.armed )
        unarmed( "g_file_get_path" );

    return fileString( "g_file_get_path", aFile );
}

GProxyResolver* __wrap_g_proxy_resolver_get_default()
{
    if( !g.armed )
        unarmed( "g_proxy_resolver_get_default" );

    record( { { "fn", "g_proxy_resolver_get_default" } } );
    return static_cast<GProxyResolver*>(
            optionalHandle( "g_proxy_resolver_get_default", "resolver" ) );
}

gchar** __wrap_g_proxy_resolver_lookup( GProxyResolver* aResolver, const gchar* aUri,
                                        GCancellable* aCancellable, GError** aError )
{
    if( !g.armed )
        unarmed( "g_proxy_resolver_lookup" );

    record( { { "fn", "g_proxy_resolver_lookup" },
              { "resolver", nameOf( aResolver ) },
              { "uri", hexOf( aUri, std::strlen( aUri ) ) },
              { "cancellable_null", aCancellable == nullptr },
              { "error_null", aError == nullptr } } );
    const JSON value = reply( "g_proxy_resolver_lookup" );
    checkKeys( value, { "list", "error" }, "reply g_proxy_resolver_lookup" );
    gchar** list = nullptr;

    if( !value.at( "list" ).is_null() )
    {
        const JSON& entries = value.at( "list" );

        if( !entries.is_array() || entries.size() > MAX_ENTRIES )
            refuse( "bad reply g_proxy_resolver_lookup" );

        list = g_new0( gchar*, entries.size() + 1 );

        for( size_t i = 0; i < entries.size(); ++i )
        {
            const std::string bytes = readCText( entries[i], "reply g_proxy_resolver_lookup" );
            list[i] = g_strndup( bytes.data(), bytes.size() );
        }

        label( list, "strv" );
    }

    GError* error = newError( value.at( "error" ), "reply g_proxy_resolver_lookup" );

    if( aError )
        *aError = error;

    return list;
}

// The C library.
long __real_syscall( long aNumber, ... );
int  __real_setpriority( int aWhich, id_t aWho, int aPrio );
char* __real_realpath( const char* aPath, char* aResolved );
void __real_free( void* aPointer );
FILE* __real_fopen( const char* aPath, const char* aMode );
FILE* __real_fopen64( const char* aPath, const char* aMode );
int  __real_fileno( FILE* aStream );
int  __real_posix_fadvise( int aFd, off_t aOffset, off_t aLen, int aAdvice );
int  __real_posix_fadvise64( int aFd, off_t aOffset, off_t aLen, int aAdvice );
int  __real_fclose( FILE* aStream );
int  __real_fflush( FILE* aStream );
int  __real_fsync( int aFd );
int  __real_stat( const char* aPath, struct stat* aBuf );
int  __real_stat64( const char* aPath, struct stat* aBuf );
int  __real_lstat( const char* aPath, struct stat* aBuf );
int  __real_lstat64( const char* aPath, struct stat* aBuf );
int  __real_fstat( int aFd, struct stat* aBuf );
int  __real_fstat64( int aFd, struct stat* aBuf );
int  __real_chmod( const char* aPath, mode_t aMode );
DIR* __real_opendir( const char* aName );
struct dirent64* __real_readdir64( DIR* aDir );
struct dirent* __real_readdir( DIR* aDir );
int  __real_closedir( DIR* aDir );
ssize_t __real_readlink( const char* aPath, char* aBuf, size_t aSize );
int  __real_fnmatch( const char* aPattern, const char* aString, int aFlags );
int  __real_open( const char* aPath, int aFlags, ... );
int  __real_open64( const char* aPath, int aFlags, ... );
int  __real_close( int aFd );
void* __real_mmap( void* aAddr, size_t aLength, int aProt, int aFlags, int aFd, off_t aOffset );
void* __real_mmap64( void* aAddr, size_t aLength, int aProt, int aFlags, int aFd, off_t aOffset );
int  __real_madvise( void* aAddr, size_t aLength, int aAdvice );
int  __real_munmap( void* aAddr, size_t aLength );
int  __real_access( const char* aPath, int aMode );
int  __real_fseek( FILE* aStream, long aOffset, int aWhence );
long __real_ftell( FILE* aStream );
size_t __real_fread( void* aPtr, size_t aSize, size_t aCount, FILE* aStream );

long __wrap_syscall( long aNumber, ... )
{
    va_list args;
    va_start( args, aNumber );
    long a[6];

    for( long& value : a )
        value = va_arg( args, long );

    va_end( args );

    if( !g.armed || aNumber != SYS_gettid )
        return __real_syscall( aNumber, a[0], a[1], a[2], a[3], a[4], a[5] );

    return guarded( -1L, [&]() -> decltype( -1L ) {
        record( { { "fn", "syscall" }, { "number", aNumber } } );
        return static_cast<long>( replyInteger( "syscall", INT64_MIN, INT64_MAX ) );
    } );
}

int __wrap_setpriority( int aWhich, id_t aWho, int aPrio )
{
    if( !g.armed )
        return __real_setpriority( aWhich, aWho, aPrio );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "setpriority" }, { "which", aWhich }, { "who", aWho }, { "prio", aPrio } } );
        return static_cast<int>( replyInteger( "setpriority", INT32_MIN, INT32_MAX ) );
    } );
}

char* __wrap_realpath( const char* aPath, char* aResolved )
{
    if( !g.armed )
        return __real_realpath( aPath, aResolved );

    return guarded( static_cast<char*>( nullptr ), [&]() -> decltype( static_cast<char*>( nullptr ) ) {
        record( { { "fn", "realpath" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "resolved_null", aResolved == nullptr } } );
        std::string bytes;

        if( !replyCText( "realpath", bytes ) )
            return nullptr;

        char* out = static_cast<char*>( std::malloc( bytes.size() + 1 ) );
        std::memcpy( out, bytes.data(), bytes.size() );
        out[bytes.size()] = '\0';
        label( out, "cstr" );
        return out;
    } );
}

void __wrap_free( void* aPointer )
{
    // Only the realpath() buffer is freed by the reference itself; free() calls the C and GLib
    // libraries make internally can reach this wrapper too and are not recorded. The label is
    // dropped once recorded, as the buffer no longer exists.
    if( g.armed && aPointer )
    {
        auto it = g.labels.find( aPointer );

        if( it != g.labels.end() && it->second.rfind( "cstr#", 0 ) == 0 )
        {
            record( { { "fn", "free" }, { "ptr", it->second } } );
            g.byLabel.erase( it->second );
            g.labels.erase( it );
        }
    }

    __real_free( aPointer );
}

static FILE* fakeFopen( const char* aPath, const char* aMode )
{
    return guarded( static_cast<FILE*>( nullptr ), [&]() -> decltype( static_cast<FILE*>( nullptr ) ) {
        record( { { "fn", "fopen" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "mode", hexOf( aMode, std::strlen( aMode ) ) } } );
        return static_cast<FILE*>( optionalHandle( "fopen", "FILE" ) );
    } );
}

FILE* __wrap_fopen( const char* aPath, const char* aMode )
{
    return g.armed ? fakeFopen( aPath, aMode ) : __real_fopen( aPath, aMode );
}

FILE* __wrap_fopen64( const char* aPath, const char* aMode )
{
    return g.armed ? fakeFopen( aPath, aMode ) : __real_fopen64( aPath, aMode );
}

int __wrap_fileno( FILE* aStream )
{
    if( !g.armed || !known( aStream ) )
        return __real_fileno( aStream );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fileno" }, { "stream", nameOf( aStream ) } } );
        return static_cast<int>( replyInteger( "fileno", INT32_MIN, INT32_MAX ) );
    } );
}

static int fakeFadvise( int aFd, off_t aOffset, off_t aLen, int aAdvice )
{
    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "posix_fadvise" },
                  { "fd", aFd },
                  { "offset", aOffset },
                  { "len", aLen },
                  { "advice", aAdvice } } );
        return static_cast<int>( replyInteger( "posix_fadvise", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_posix_fadvise( int aFd, off_t aOffset, off_t aLen, int aAdvice )
{
    return g.armed ? fakeFadvise( aFd, aOffset, aLen, aAdvice )
                   : __real_posix_fadvise( aFd, aOffset, aLen, aAdvice );
}

int __wrap_posix_fadvise64( int aFd, off_t aOffset, off_t aLen, int aAdvice )
{
    return g.armed ? fakeFadvise( aFd, aOffset, aLen, aAdvice )
                   : __real_posix_fadvise64( aFd, aOffset, aLen, aAdvice );
}

int __wrap_fclose( FILE* aStream )
{
    if( !g.armed || !known( aStream ) )
        return __real_fclose( aStream );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fclose" }, { "stream", nameOf( aStream ) } } );
        return static_cast<int>( replyInteger( "fclose", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_fflush( FILE* aStream )
{
    if( !g.armed || !known( aStream ) )
        return __real_fflush( aStream );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fflush" }, { "stream", nameOf( aStream ) } } );
        return static_cast<int>( replyInteger( "fflush", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_fsync( int aFd )
{
    if( !g.armed )
        return __real_fsync( aFd );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fsync" }, { "fd", aFd } } );
        return static_cast<int>( replyInteger( "fsync", INT32_MIN, INT32_MAX ) );
    } );
}

static int fakeStat( const char* aFunction, const char* aPath, struct stat* aBuf )
{
    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", aFunction }, { "path", hexOf( aPath, std::strlen( aPath ) ) } } );
        const STAT_REPLY value = replyStat( aFunction );
        *aBuf = value.buf;
        return value.result;
    } );
}

int __wrap_stat( const char* aPath, struct stat* aBuf )
{
    return g.armed ? fakeStat( "stat", aPath, aBuf ) : __real_stat( aPath, aBuf );
}

int __wrap_stat64( const char* aPath, struct stat* aBuf )
{
    return g.armed ? fakeStat( "stat", aPath, aBuf ) : __real_stat64( aPath, aBuf );
}

int __wrap_lstat( const char* aPath, struct stat* aBuf )
{
    return g.armed ? fakeStat( "lstat", aPath, aBuf ) : __real_lstat( aPath, aBuf );
}

int __wrap_lstat64( const char* aPath, struct stat* aBuf )
{
    return g.armed ? fakeStat( "lstat", aPath, aBuf ) : __real_lstat64( aPath, aBuf );
}

static int fakeFstat( int aFd, struct stat* aBuf )
{
    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fstat" }, { "fd", aFd } } );
        const STAT_REPLY value = replyStat( "fstat" );
        *aBuf = value.buf;
        return value.result;
    } );
}

int __wrap_fstat( int aFd, struct stat* aBuf )
{
    return g.armed ? fakeFstat( aFd, aBuf ) : __real_fstat( aFd, aBuf );
}

int __wrap_fstat64( int aFd, struct stat* aBuf )
{
    return g.armed ? fakeFstat( aFd, aBuf ) : __real_fstat64( aFd, aBuf );
}

int __wrap_chmod( const char* aPath, mode_t aMode )
{
    if( !g.armed )
        return __real_chmod( aPath, aMode );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "chmod" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "mode", static_cast<uint32_t>( aMode ) } } );
        return static_cast<int>( replyInteger( "chmod", INT32_MIN, INT32_MAX ) );
    } );
}

DIR* __wrap_opendir( const char* aName )
{
    if( !g.armed )
        return __real_opendir( aName );

    return guarded( static_cast<DIR*>( nullptr ), [&]() -> decltype( static_cast<DIR*>( nullptr ) ) {
        record( { { "fn", "opendir" }, { "name", hexOf( aName, std::strlen( aName ) ) } } );
        return static_cast<DIR*>( optionalHandle( "opendir", "DIR" ) );
    } );
}

static struct dirent64* fakeReaddir( DIR* aDir )
{
    return guarded( static_cast<struct dirent64*>( nullptr ), [&]() -> decltype( static_cast<struct dirent64*>( nullptr ) ) {
        record( { { "fn", "readdir" }, { "dir", nameOf( aDir ) } } );
        std::string name;

        if( !replyCText( "readdir", name ) )
            return nullptr;

        auto entry = std::make_unique<struct dirent64>();

        if( name.size() >= sizeof entry->d_name )
            refuse( "bad reply readdir" );

        std::memcpy( entry->d_name, name.c_str(), name.size() + 1 );
        g.dirents.push_back( std::move( entry ) );
        return g.dirents.back().get();
    } );
}

struct dirent64* __wrap_readdir64( DIR* aDir )
{
    return g.armed && known( aDir ) ? fakeReaddir( aDir ) : __real_readdir64( aDir );
}

struct dirent* __wrap_readdir( DIR* aDir )
{
    static_assert( sizeof( struct dirent ) == sizeof( struct dirent64 ) );
    return g.armed && known( aDir ) ? reinterpret_cast<struct dirent*>( fakeReaddir( aDir ) )
                                    : __real_readdir( aDir );
}

int __wrap_closedir( DIR* aDir )
{
    if( !g.armed || !known( aDir ) )
        return __real_closedir( aDir );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "closedir" }, { "dir", nameOf( aDir ) } } );
        return static_cast<int>( replyInteger( "closedir", INT32_MIN, INT32_MAX ) );
    } );
}

ssize_t __wrap_readlink( const char* aPath, char* aBuf, size_t aSize )
{
    if( !g.armed )
        return __real_readlink( aPath, aBuf, aSize );

    return guarded( static_cast<ssize_t>( -1 ), [&]() -> decltype( static_cast<ssize_t>( -1 ) ) {
        record( { { "fn", "readlink" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "bufsiz", aSize } } );
        const JSON value = reply( "readlink" );
        checkKeys( value, { "result", "bytes" }, "reply readlink" );
        const int64_t     result = readInteger( value.at( "result" ), "reply readlink", -1,
                                                static_cast<int64_t>( aSize ) );
        const std::string bytes = readHex( value.at( "bytes" ), "reply readlink" );

        if( bytes.size() > aSize || ( result > 0 && bytes.size() != static_cast<size_t>( result ) ) )
            refuse( "bad reply readlink" );

        std::memcpy( aBuf, bytes.data(), bytes.size() );
        return result;
    } );
}

int __wrap_fnmatch( const char* aPattern, const char* aString, int aFlags )
{
    if( !g.armed )
        return __real_fnmatch( aPattern, aString, aFlags );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fnmatch" },
                  { "pattern", hexOf( aPattern, std::strlen( aPattern ) ) },
                  { "string", hexOf( aString, std::strlen( aString ) ) },
                  { "flags", aFlags } } );
        return static_cast<int>( replyInteger( "fnmatch", INT32_MIN, INT32_MAX ) );
    } );
}

static int fakeOpen( const char* aPath, int aFlags )
{
    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "open" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "flags", aFlags } } );
        return static_cast<int>( replyInteger( "open", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_open( const char* aPath, int aFlags, ... )
{
    va_list args;
    va_start( args, aFlags );
    const mode_t mode = ( aFlags & ( O_CREAT | O_TMPFILE ) ) ? va_arg( args, mode_t ) : 0;
    va_end( args );
    return g.armed ? fakeOpen( aPath, aFlags ) : __real_open( aPath, aFlags, mode );
}

int __wrap_open64( const char* aPath, int aFlags, ... )
{
    va_list args;
    va_start( args, aFlags );
    const mode_t mode = ( aFlags & ( O_CREAT | O_TMPFILE ) ) ? va_arg( args, mode_t ) : 0;
    va_end( args );
    return g.armed ? fakeOpen( aPath, aFlags ) : __real_open64( aPath, aFlags, mode );
}

int __wrap_close( int aFd )
{
    if( !g.armed )
        return __real_close( aFd );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "close" }, { "fd", aFd } } );
        return static_cast<int>( replyInteger( "close", INT32_MIN, INT32_MAX ) );
    } );
}

static void* fakeMmap( void* aAddr, size_t aLength, int aProt, int aFlags, int aFd, off_t aOffset )
{
    return guarded( static_cast<void*>( MAP_FAILED ), [&]() -> decltype( static_cast<void*>( MAP_FAILED ) ) {
        record( { { "fn", "mmap" },
                  { "addr_null", aAddr == nullptr },
                  { "length", aLength },
                  { "prot", aProt },
                  { "flags", aFlags },
                  { "fd", aFd },
                  { "offset", aOffset } } );
        const JSON value = reply( "mmap" );

        if( value.is_null() )
            return MAP_FAILED;

        const std::string bytes = readHex( value, "reply mmap" );

        if( bytes.size() != aLength )
            refuse( "bad reply mmap" );

        g.buffers.push_back( std::make_unique<std::string>( bytes ) );
        void* data = g.buffers.back()->data();
        label( data, "map" );
        return data;
    } );
}

void* __wrap_mmap( void* aAddr, size_t aLength, int aProt, int aFlags, int aFd, off_t aOffset )
{
    return g.armed ? fakeMmap( aAddr, aLength, aProt, aFlags, aFd, aOffset )
                   : __real_mmap( aAddr, aLength, aProt, aFlags, aFd, aOffset );
}

void* __wrap_mmap64( void* aAddr, size_t aLength, int aProt, int aFlags, int aFd, off_t aOffset )
{
    return g.armed ? fakeMmap( aAddr, aLength, aProt, aFlags, aFd, aOffset )
                   : __real_mmap64( aAddr, aLength, aProt, aFlags, aFd, aOffset );
}

int __wrap_madvise( void* aAddr, size_t aLength, int aAdvice )
{
    if( !g.armed || !known( aAddr ) )
        return __real_madvise( aAddr, aLength, aAdvice );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "madvise" },
                  { "addr", nameOf( aAddr ) },
                  { "length", aLength },
                  { "advice", aAdvice } } );
        return static_cast<int>( replyInteger( "madvise", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_munmap( void* aAddr, size_t aLength )
{
    if( !g.armed || !known( aAddr ) )
        return __real_munmap( aAddr, aLength );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "munmap" }, { "addr", nameOf( aAddr ) }, { "length", aLength } } );
        return static_cast<int>( replyInteger( "munmap", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_access( const char* aPath, int aMode )
{
    if( !g.armed )
        return __real_access( aPath, aMode );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "access" },
                  { "path", hexOf( aPath, std::strlen( aPath ) ) },
                  { "mode", aMode } } );
        return static_cast<int>( replyInteger( "access", INT32_MIN, INT32_MAX ) );
    } );
}

int __wrap_fseek( FILE* aStream, long aOffset, int aWhence )
{
    if( !g.armed || !known( aStream ) )
        return __real_fseek( aStream, aOffset, aWhence );

    return guarded( -1, [&]() -> decltype( -1 ) {
        record( { { "fn", "fseek" },
                  { "stream", nameOf( aStream ) },
                  { "offset", aOffset },
                  { "whence", aWhence } } );
        return static_cast<int>( replyInteger( "fseek", INT32_MIN, INT32_MAX ) );
    } );
}

long __wrap_ftell( FILE* aStream )
{
    if( !g.armed || !known( aStream ) )
        return __real_ftell( aStream );

    return guarded( -1L, [&]() -> decltype( -1L ) {
        record( { { "fn", "ftell" }, { "stream", nameOf( aStream ) } } );
        // A length the fallback buffer can be resized to without exhausting the adapter.
        return static_cast<long>( replyInteger( "ftell", INT64_MIN, MAX_BYTES ) );
    } );
}

size_t __wrap_fread( void* aPtr, size_t aSize, size_t aCount, FILE* aStream )
{
    if( !g.armed || !known( aStream ) )
        return __real_fread( aPtr, aSize, aCount, aStream );

    return guarded( static_cast<size_t>( 0 ), [&]() -> decltype( static_cast<size_t>( 0 ) ) {
        record( { { "fn", "fread" },
                  { "stream", nameOf( aStream ) },
                  { "size", aSize },
                  { "count", aCount } } );
        const std::string bytes = readHex( reply( "fread" ), "reply fread" );

        if( bytes.size() > aSize * aCount )
            refuse( "bad reply fread" );

        std::memcpy( aPtr, bytes.data(), bytes.size() );
        return aSize == 0 ? 0 : bytes.size() / aSize;
    } );
}

// libsecret.
static JSON passwordCall( const char* aFunction, const SecretSchema* aSchema )
{
    return { { "fn", aFunction }, { "schema", schemaOf( aSchema ) } };
}

gboolean __wrap_secret_password_store_sync( const SecretSchema* aSchema, const gchar* aCollection,
                                            const gchar* aLabel, const gchar* aPassword,
                                            GCancellable* aCancellable, GError** aError, ... )
{
    if( !g.armed )
        unarmed( "secret_password_store_sync" );

    va_list args;
    va_start( args, aError );
    JSON call = passwordCall( "secret_password_store_sync", aSchema );
    call["collection"] = hexOf( aCollection, std::strlen( aCollection ) );
    call["label"] = hexOf( aLabel, std::strlen( aLabel ) );
    call["password"] = hexOf( aPassword, std::strlen( aPassword ) );
    call["cancellable_null"] = aCancellable == nullptr;
    call["error_null"] = aError == nullptr;
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    const JSON value = reply( "secret_password_store_sync" );
    checkKeys( value, { "result", "error" }, "reply secret_password_store_sync" );
    const bool result = readBool( value.at( "result" ), "reply secret_password_store_sync" );
    GError*    error = newError( value.at( "error" ), "reply secret_password_store_sync" );

    if( aError )
        *aError = error;

    return result;
}

void __wrap_secret_password_store( const SecretSchema* aSchema, const gchar* aCollection,
                                   const gchar* aLabel, const gchar* aPassword,
                                   GCancellable* aCancellable, GAsyncReadyCallback aCallback,
                                   gpointer aUserData, ... )
{
    if( !g.armed )
        unarmed( "secret_password_store" );

    va_list args;
    va_start( args, aUserData );
    JSON call = passwordCall( "secret_password_store", aSchema );
    call["collection"] = hexOf( aCollection, std::strlen( aCollection ) );
    call["label"] = hexOf( aLabel, std::strlen( aLabel ) );
    call["password"] = hexOf( aPassword, std::strlen( aPassword ) );
    call["cancellable_null"] = aCancellable == nullptr;
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    g.pending.push_back( { true, aCallback, aUserData } );
}

gboolean __wrap_secret_password_store_finish( GAsyncResult* aResult, GError** aError )
{
    if( !g.armed )
        unarmed( "secret_password_store_finish" );

    record( { { "fn", "secret_password_store_finish" },
              { "result", nameOf( aResult ) },
              { "error_null", aError == nullptr } } );
    return replyBool( "secret_password_store_finish" );
}

gchar* __wrap_secret_password_lookup_sync( const SecretSchema* aSchema,
                                           GCancellable* aCancellable, GError** aError, ... )
{
    if( !g.armed )
        unarmed( "secret_password_lookup_sync" );

    va_list args;
    va_start( args, aError );
    JSON call = passwordCall( "secret_password_lookup_sync", aSchema );
    call["cancellable_null"] = aCancellable == nullptr;
    call["error_null"] = aError == nullptr;
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    const JSON value = reply( "secret_password_lookup_sync" );
    checkKeys( value, { "secret", "error" }, "reply secret_password_lookup_sync" );
    gchar* secret = nullptr;

    if( !value.at( "secret" ).is_null() )
        secret = newGStr( readCText( value.at( "secret" ), "reply secret_password_lookup_sync" ) );

    GError* error = newError( value.at( "error" ), "reply secret_password_lookup_sync" );

    if( aError )
        *aError = error;

    return secret;
}

void __wrap_secret_password_lookup( const SecretSchema* aSchema, GCancellable* aCancellable,
                                    GAsyncReadyCallback aCallback, gpointer aUserData, ... )
{
    if( !g.armed )
        unarmed( "secret_password_lookup" );

    va_list args;
    va_start( args, aUserData );
    JSON call = passwordCall( "secret_password_lookup", aSchema );
    call["cancellable_null"] = aCancellable == nullptr;
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    g.pending.push_back( { false, aCallback, aUserData } );
}

gchar* __wrap_secret_password_lookup_finish( GAsyncResult* aResult, GError** aError )
{
    if( !g.armed )
        unarmed( "secret_password_lookup_finish" );

    record( { { "fn", "secret_password_lookup_finish" },
              { "result", nameOf( aResult ) },
              { "error_null", aError == nullptr } } );
    std::string bytes;

    if( !replyCText( "secret_password_lookup_finish", bytes ) )
        return nullptr;

    return newGStr( bytes );
}

GHashTable* __wrap_secret_attributes_build( const SecretSchema* aSchema, ... )
{
    if( !g.armed )
        unarmed( "secret_attributes_build" );

    va_list args;
    va_start( args, aSchema );
    JSON call = passwordCall( "secret_attributes_build", aSchema );
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    return static_cast<GHashTable*>( newHandle( "table" ) );
}

GList* __wrap_secret_service_search_sync( SecretService* aService, const SecretSchema* aSchema,
                                          GHashTable* aAttributes, SecretSearchFlags aFlags,
                                          GCancellable* aCancellable, GError** aError )
{
    if( !g.armed )
        unarmed( "secret_service_search_sync" );

    JSON call = passwordCall( "secret_service_search_sync", aSchema );
    call["service_null"] = aService == nullptr;
    call["attributes"] = nameOf( aAttributes );
    call["flags"] = static_cast<int>( aFlags );
    call["cancellable_null"] = aCancellable == nullptr;
    call["error_null"] = aError == nullptr;
    record( call );
    const JSON value = reply( "secret_service_search_sync" );
    checkKeys( value, { "items", "error" }, "reply secret_service_search_sync" );
    const JSON& items = value.at( "items" );

    if( !items.is_array() || items.size() > MAX_ENTRIES )
        refuse( "bad reply secret_service_search_sync" );

    GList* list = nullptr;

    for( const JSON& spec : items )
    {
        checkKeys( spec, { "locked", "secret" }, "reply secret_service_search_sync item" );
        auto item = std::make_unique<ITEM>();
        item->locked = readBool( spec.at( "locked" ), "item locked" );

        if( !spec.at( "secret" ).is_null() )
        {
            item->hasSecret = true;
            const JSON& text = spec.at( "secret" );

            if( !text.is_null() )
            {
                checkKeys( text, { "text" }, "item secret" );

                if( !text.at( "text" ).is_null() )
                {
                    item->hasText = true;
                    item->text = readCText( text.at( "text" ), "item text" );
                }
            }
        }

        void* handle = newHandle( "item" );
        g.items[handle] = *item;
        g.itemStore.push_back( std::move( item ) );
        list = g_list_append( list, handle );
    }

    if( list )
        label( list, "list" );

    GError* error = newError( value.at( "error" ), "reply secret_service_search_sync" );

    if( aError )
        *aError = error;

    return list;
}

gboolean __wrap_secret_item_get_locked( SecretItem* aItem )
{
    if( !g.armed )
        unarmed( "secret_item_get_locked" );

    record( { { "fn", "secret_item_get_locked" }, { "item", nameOf( aItem ) } } );
    return g.items.at( aItem ).locked;
}

SecretValue* __wrap_secret_item_get_secret( SecretItem* aItem )
{
    if( !g.armed )
        unarmed( "secret_item_get_secret" );

    record( { { "fn", "secret_item_get_secret" }, { "item", nameOf( aItem ) } } );
    const ITEM& item = g.items.at( aItem );

    if( !item.hasSecret )
        return nullptr;

    void* value = newHandle( "value" );
    g.values[value] = &item;
    return static_cast<SecretValue*>( value );
}

const gchar* __wrap_secret_value_get_text( SecretValue* aValue )
{
    if( !g.armed )
        unarmed( "secret_value_get_text" );

    record( { { "fn", "secret_value_get_text" }, { "value", nameOf( aValue ) } } );
    const ITEM* item = g.values.at( aValue );
    return item->hasText ? item->text.c_str() : nullptr;
}

void __wrap_secret_value_unref( gpointer aValue )
{
    if( !g.armed )
        unarmed( "secret_value_unref" );

    record( { { "fn", "secret_value_unref" }, { "value", nameOf( aValue ) } } );
}

gboolean __wrap_secret_password_clear_sync( const SecretSchema* aSchema,
                                            GCancellable* aCancellable, GError** aError, ... )
{
    if( !g.armed )
        unarmed( "secret_password_clear_sync" );

    va_list args;
    va_start( args, aError );
    JSON call = passwordCall( "secret_password_clear_sync", aSchema );
    call["cancellable_null"] = aCancellable == nullptr;
    call["error_null"] = aError == nullptr;
    call["attributes"] = attributesOf( args );
    va_end( args );
    record( call );
    const JSON value = reply( "secret_password_clear_sync" );
    checkKeys( value, { "result", "error" }, "reply secret_password_clear_sync" );
    const bool result = readBool( value.at( "result" ), "reply secret_password_clear_sync" );
    GError*    error = newError( value.at( "error" ), "reply secret_password_clear_sync" );

    if( aError )
        *aError = error;

    return result;
}

// poppler.
PopplerDocument* __wrap_poppler_document_new_from_file( const char* aUri, const char* aPassword,
                                                        GError** aError )
{
    if( !g.armed )
        unarmed( "poppler_document_new_from_file" );

    record( { { "fn", "poppler_document_new_from_file" },
              { "uri", hexOf( aUri, std::strlen( aUri ) ) },
              { "password_null", aPassword == nullptr },
              { "error_null", aError == nullptr } } );
    const JSON value = reply( "poppler_document_new_from_file" );
    checkKeys( value, { "doc", "error" }, "reply poppler_document_new_from_file" );
    PopplerDocument* doc = readBool( value.at( "doc" ), "reply poppler_document_new_from_file" )
                                   ? static_cast<PopplerDocument*>( newHandle( "doc" ) )
                                   : nullptr;
    GError* error = newError( value.at( "error" ), "reply poppler_document_new_from_file" );

    if( aError )
        *aError = error;

    return doc;
}

int __wrap_poppler_document_get_n_pages( PopplerDocument* aDocument )
{
    if( !g.armed )
        unarmed( "poppler_document_get_n_pages" );

    record( { { "fn", "poppler_document_get_n_pages" }, { "document", nameOf( aDocument ) } } );
    return static_cast<int>( replyInteger( "poppler_document_get_n_pages", INT32_MIN, INT32_MAX ) );
}

PopplerPage* __wrap_poppler_document_get_page( PopplerDocument* aDocument, int aIndex )
{
    if( !g.armed )
        unarmed( "poppler_document_get_page" );

    record( { { "fn", "poppler_document_get_page" },
              { "document", nameOf( aDocument ) },
              { "index", aIndex } } );
    const JSON value = reply( "poppler_document_get_page" );

    if( value.is_null() )
        return nullptr;

    checkKeys( value, { "width", "height" }, "reply poppler_document_get_page" );
    void* page = newHandle( "page" );
    g.pages[page] = { readBits( value.at( "width" ), "page width" ),
                      readBits( value.at( "height" ), "page height" ) };
    return static_cast<PopplerPage*>( page );
}

void __wrap_poppler_page_get_size( PopplerPage* aPage, double* aWidth, double* aHeight )
{
    if( !g.armed )
        unarmed( "poppler_page_get_size" );

    record( { { "fn", "poppler_page_get_size" }, { "page", nameOf( aPage ) } } );
    const PAGE& page = g.pages.at( aPage );

    // draw_page divides the context size by these when fitting: a zero divisor is undefined.
    if( g.inDrawPage && g.currentFit && ( page.width == 0.0 || page.height == 0.0 ) )
        refuse( "undefined: floating division by zero in draw_page" );
    *aWidth = page.width;
    *aHeight = page.height;
}

void __wrap_poppler_page_render( PopplerPage* aPage, cairo_t* aCairo )
{
    if( !g.armed )
        unarmed( "poppler_page_render" );

    record( { { "fn", "poppler_page_render" }, { "page", nameOf( aPage ) }, { "cairo", nameOf( aCairo ) } } );
}

// GTK printing.
GtkPrintOperation* __wrap_gtk_print_operation_new()
{
    if( !g.armed )
        unarmed( "gtk_print_operation_new" );

    record( { { "fn", "gtk_print_operation_new" } } );
    return static_cast<GtkPrintOperation*>( optionalHandle( "gtk_print_operation_new", "op" ) );
}

void __wrap_gtk_print_operation_set_use_full_page( GtkPrintOperation* aOp, gboolean aFullPage )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_use_full_page" );

    record( { { "fn", "gtk_print_operation_set_use_full_page" },
              { "op", nameOf( aOp ) },
              { "full_page", aFullPage != FALSE } } );
}

void __wrap_gtk_print_operation_set_unit( GtkPrintOperation* aOp, GtkUnit aUnit )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_unit" );

    record( { { "fn", "gtk_print_operation_set_unit" },
              { "op", nameOf( aOp ) },
              { "unit", static_cast<int>( aUnit ) } } );
}

void __wrap_gtk_print_operation_set_print_settings( GtkPrintOperation* aOp,
                                                    GtkPrintSettings*  aSettings )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_print_settings" );

    record( { { "fn", "gtk_print_operation_set_print_settings" },
              { "op", nameOf( aOp ) },
              { "settings", nameOf( aSettings ) } } );
    g.opSettings[aOp] = aSettings;
}

void __wrap_gtk_print_operation_set_job_name( GtkPrintOperation* aOp, const gchar* aJobName )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_job_name" );

    record( { { "fn", "gtk_print_operation_set_job_name" },
              { "op", nameOf( aOp ) },
              { "job_name", hexOf( aJobName, std::strlen( aJobName ) ) } } );
}

void __wrap_gtk_print_operation_set_current_page( GtkPrintOperation* aOp, gint aCurrentPage )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_current_page" );

    record( { { "fn", "gtk_print_operation_set_current_page" },
              { "op", nameOf( aOp ) },
              { "current_page", aCurrentPage } } );
}

void __wrap_gtk_print_operation_set_n_pages( GtkPrintOperation* aOp, gint aPages )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_set_n_pages" );

    record( { { "fn", "gtk_print_operation_set_n_pages" }, { "op", nameOf( aOp ) }, { "n_pages", aPages } } );
}

void __wrap_gtk_print_operation_cancel( GtkPrintOperation* aOp )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_cancel" );

    record( { { "fn", "gtk_print_operation_cancel" }, { "op", nameOf( aOp ) } } );
}

static GtkPrintSettings* newSettings( const SETTINGS& aEntries )
{
    void* settings = newHandle( "settings" );
    g.settings[settings] = aEntries;
    return static_cast<GtkPrintSettings*>( settings );
}

static SETTINGS readSettings( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_array() || aValue.size() > MAX_ENTRIES )
        refuse( "bad settings: " + aName );

    SETTINGS out;

    for( const JSON& entry : aValue )
    {
        if( !entry.is_array() || entry.size() != 2 )
            refuse( "bad settings: " + aName );

        out[readCText( entry[0], aName )] = readCText( entry[1], aName );
    }

    return out;
}

GtkPrintOperationResult __wrap_gtk_print_operation_run( GtkPrintOperation*      aOp,
                                                        GtkPrintOperationAction aAction,
                                                        GtkWindow* aParent, GError** aError )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_run" );

    record( { { "fn", "gtk_print_operation_run" },
              { "op", nameOf( aOp ) },
              { "action", static_cast<int>( aAction ) },
              { "parent", nameOf( aParent ) },
              { "error_null", aError == nullptr } } );
    const JSON value = reply( "gtk_print_operation_run" );
    checkKeys( value, { "context", "emit", "settings", "result", "error" },
               "reply gtk_print_operation_run" );
    const JSON& contextSpec = value.at( "context" );
    checkKeys( contextSpec, { "cairo", "width", "height" }, "reply gtk_print_operation_run context" );
    void*   context = newHandle( "context" );
    CONTEXT state;
    state.hasCairo = readBool( contextSpec.at( "cairo" ), "context cairo" );
    state.width = readBits( contextSpec.at( "width" ), "context width" );
    state.height = readBits( contextSpec.at( "height" ), "context height" );
    g.contexts[context] = state;
    const JSON& emits = value.at( "emit" );

    if( !emits.is_array() || emits.size() > MAX_ENTRIES )
        refuse( "bad reply gtk_print_operation_run" );

    for( const JSON& emit : emits )
    {
        const std::string signal = readText( member( emit, "signal" ), "emit signal" );
        int               page = 0;

        if( signal == "begin-print" )
        {
            checkKeys( emit, { "signal" }, "emit" );
        }
        else if( signal == "draw-page" || signal == "request-page-setup" )
        {
            checkKeys( emit, { "signal", "page" }, "emit" );
            page = static_cast<int>( readInteger( emit.at( "page" ), "emit page", INT32_MIN,
                                                  INT32_MAX ) );
        }
        else
        {
            refuse( "unknown signal" );
        }

        for( const CONNECTION& connection : g.connections )
        {
            if( connection.signal != signal )
                continue;

            auto* op = aOp;
            auto* ctx = static_cast<GtkPrintContext*>( context );

            if( signal == "begin-print" )
            {
                reinterpret_cast<void ( * )( GtkPrintOperation*, GtkPrintContext*, gpointer )>(
                        connection.handler )( op, ctx, connection.data );
            }
            else if( signal == "draw-page" )
            {
                g.inDrawPage = true;
                reinterpret_cast<void ( * )( GtkPrintOperation*, GtkPrintContext*, gint, gpointer )>(
                        connection.handler )( op, ctx, page, connection.data );
                g.inDrawPage = false;
            }
            else
            {
                void* setup = newHandle( "setup" );
                reinterpret_cast<void ( * )( GtkPrintOperation*, GtkPrintContext*, gint,
                                             GtkPageSetup*, gpointer )>( connection.handler )(
                        op, ctx, page, static_cast<GtkPageSetup*>( setup ), connection.data );
            }
        }
    }

    if( !value.at( "settings" ).is_null() )
    {
        const SETTINGS entries = readSettings( value.at( "settings" ), "run settings" );
        g.opSettings[aOp] = newSettings( entries );
    }

    const int result = static_cast<int>( readInteger( value.at( "result" ), "run result", INT32_MIN,
                                                      INT32_MAX ) );
    GError* error = newError( value.at( "error" ), "reply gtk_print_operation_run" );

    if( aError )
        *aError = error;

    return static_cast<GtkPrintOperationResult>( result );
}

GtkPrintSettings* __wrap_gtk_print_operation_get_print_settings( GtkPrintOperation* aOp )
{
    if( !g.armed )
        unarmed( "gtk_print_operation_get_print_settings" );

    record( { { "fn", "gtk_print_operation_get_print_settings" }, { "op", nameOf( aOp ) } } );
    auto it = g.opSettings.find( aOp );
    return it == g.opSettings.end()
                   ? nullptr
                   : static_cast<GtkPrintSettings*>( const_cast<void*>( it->second ) );
}

GtkPrintSettings* __wrap_gtk_print_settings_new()
{
    if( !g.armed )
        unarmed( "gtk_print_settings_new" );

    record( { { "fn", "gtk_print_settings_new" } } );
    return newSettings( {} );
}

GtkPrintSettings* __wrap_gtk_print_settings_copy( GtkPrintSettings* aOther )
{
    if( !g.armed )
        unarmed( "gtk_print_settings_copy" );

    record( { { "fn", "gtk_print_settings_copy" }, { "other", nameOf( aOther ) } } );
    return newSettings( g.settings.at( aOther ) );
}

const gchar* __wrap_gtk_print_settings_get( GtkPrintSettings* aSettings, const gchar* aKey )
{
    if( !g.armed )
        unarmed( "gtk_print_settings_get" );

    record( { { "fn", "gtk_print_settings_get" },
              { "settings", nameOf( aSettings ) },
              { "key", hexOf( aKey, std::strlen( aKey ) ) } } );
    SETTINGS& entries = g.settings.at( aSettings );
    auto      it = entries.find( aKey );
    return it == entries.end() ? nullptr : it->second.c_str();
}

void __wrap_gtk_print_settings_set( GtkPrintSettings* aSettings, const gchar* aKey,
                                    const gchar* aValue )
{
    if( !g.armed )
        unarmed( "gtk_print_settings_set" );

    record( { { "fn", "gtk_print_settings_set" },
              { "settings", nameOf( aSettings ) },
              { "key", hexOf( aKey, std::strlen( aKey ) ) },
              { "value", hexOf( aValue, std::strlen( aValue ) ) } } );
    g.settings.at( aSettings )[aKey] = aValue;
}

void __wrap_gtk_print_settings_unset( GtkPrintSettings* aSettings, const gchar* aKey )
{
    if( !g.armed )
        unarmed( "gtk_print_settings_unset" );

    record( { { "fn", "gtk_print_settings_unset" },
              { "settings", nameOf( aSettings ) },
              { "key", hexOf( aKey, std::strlen( aKey ) ) } } );
    g.settings.at( aSettings ).erase( aKey );
}

void __wrap_gtk_print_settings_set_print_pages( GtkPrintSettings* aSettings, GtkPrintPages aPages )
{
    if( !g.armed )
        unarmed( "gtk_print_settings_set_print_pages" );

    record( { { "fn", "gtk_print_settings_set_print_pages" },
              { "settings", nameOf( aSettings ) },
              { "pages", static_cast<int>( aPages ) } } );
}

cairo_t* __wrap_gtk_print_context_get_cairo_context( GtkPrintContext* aContext )
{
    if( !g.armed )
        unarmed( "gtk_print_context_get_cairo_context" );

    record( { { "fn", "gtk_print_context_get_cairo_context" }, { "context", nameOf( aContext ) } } );
    CONTEXT& context = g.contexts.at( aContext );

    if( !context.hasCairo )
        return nullptr;

    if( !context.cairo )
        context.cairo = newHandle( "cairo" );

    return static_cast<cairo_t*>( context.cairo );
}

double __wrap_gtk_print_context_get_width( GtkPrintContext* aContext )
{
    if( !g.armed )
        unarmed( "gtk_print_context_get_width" );

    record( { { "fn", "gtk_print_context_get_width" }, { "context", nameOf( aContext ) } } );
    return g.contexts.at( aContext ).width;
}

double __wrap_gtk_print_context_get_height( GtkPrintContext* aContext )
{
    if( !g.armed )
        unarmed( "gtk_print_context_get_height" );

    record( { { "fn", "gtk_print_context_get_height" }, { "context", nameOf( aContext ) } } );
    return g.contexts.at( aContext ).height;
}

void __wrap_gtk_page_setup_set_orientation( GtkPageSetup* aSetup, GtkPageOrientation aOrientation )
{
    if( !g.armed )
        unarmed( "gtk_page_setup_set_orientation" );

    record( { { "fn", "gtk_page_setup_set_orientation" },
              { "setup", nameOf( aSetup ) },
              { "orientation", static_cast<int>( aOrientation ) } } );
}

GtkWidget* __wrap_gtk_widget_get_toplevel( GtkWidget* aWidget )
{
    if( !g.armed )
        unarmed( "gtk_widget_get_toplevel" );

    record( { { "fn", "gtk_widget_get_toplevel" }, { "widget", nameOf( aWidget ) } } );
    return static_cast<GtkWidget*>( optionalHandle( "gtk_widget_get_toplevel", "widget" ) );
}

gboolean __wrap_gtk_widget_is_toplevel( GtkWidget* aWidget )
{
    if( !g.armed )
        unarmed( "gtk_widget_is_toplevel" );

    record( { { "fn", "gtk_widget_is_toplevel" }, { "widget", nameOf( aWidget ) } } );
    return replyBool( "gtk_widget_is_toplevel" );
}

// cairo.
void __wrap_cairo_save( cairo_t* aCr )
{
    if( !g.armed )
        unarmed( "cairo_save" );

    record( { { "fn", "cairo_save" }, { "cr", nameOf( aCr ) } } );
}

void __wrap_cairo_restore( cairo_t* aCr )
{
    if( !g.armed )
        unarmed( "cairo_restore" );

    record( { { "fn", "cairo_restore" }, { "cr", nameOf( aCr ) } } );
}

void __wrap_cairo_translate( cairo_t* aCr, double aX, double aY )
{
    if( !g.armed )
        unarmed( "cairo_translate" );

    record( { { "fn", "cairo_translate" }, { "cr", nameOf( aCr ) }, { "tx", bitsOf( aX ) }, { "ty", bitsOf( aY ) } } );
}

void __wrap_cairo_scale( cairo_t* aCr, double aX, double aY )
{
    if( !g.armed )
        unarmed( "cairo_scale" );

    record( { { "fn", "cairo_scale" }, { "cr", nameOf( aCr ) }, { "sx", bitsOf( aX ) }, { "sy", bitsOf( aY ) } } );
}

void __wrap_cairo_set_source_rgb( cairo_t* aCr, double aRed, double aGreen, double aBlue )
{
    if( !g.armed )
        unarmed( "cairo_set_source_rgb" );

    record( { { "fn", "cairo_set_source_rgb" },
              { "cr", nameOf( aCr ) },
              { "red", bitsOf( aRed ) },
              { "green", bitsOf( aGreen ) },
              { "blue", bitsOf( aBlue ) } } );
}

void __wrap_cairo_paint( cairo_t* aCr )
{
    if( !g.armed )
        unarmed( "cairo_paint" );

    record( { { "fn", "cairo_paint" }, { "cr", nameOf( aCr ) } } );
}
} // extern "C"


namespace
{
// ---- Case set-up --------------------------------------------------------------------------

// The reply queues a host script may supply, each a function the fakes answer.
const char* const REPLY_FUNCTIONS[] = {
    "GetHandle", "access", "chmod", "close", "closedir", "fclose", "fflush", "fileno",
    "fnmatch", "fopen", "fread", "fseek", "fstat", "fsync", "ftell", "g_drive_is_removable",
    "g_file_find_enclosing_mount", "g_file_get_basename", "g_file_get_parent", "g_file_get_path",
    "g_file_trash", "g_filename_to_uri", "g_get_home_dir", "g_get_user_cache_dir",
    "g_get_user_config_dir", "g_get_user_data_dir", "g_mount_get_drive",
    "g_proxy_resolver_get_default", "g_proxy_resolver_lookup", "gtk_print_operation_new",
    "gtk_print_operation_run", "gtk_widget_get_toplevel", "gtk_widget_is_toplevel", "lstat",
    "madvise", "mmap", "munmap", "open", "opendir", "poppler_document_get_n_pages",
    "poppler_document_get_page", "poppler_document_new_from_file", "posix_fadvise", "readdir",
    "readlink", "realpath", "secret_password_clear_sync", "secret_password_lookup_finish",
    "secret_password_lookup_sync", "secret_password_store_finish", "secret_password_store_sync",
    "secret_service_search_sync", "setpriority", "stat", "syscall", "wxGetActiveWindow",
    "wxGetTopLevelParent"
};


// The recording wx log target: the messages wxLogError and wxLogTrace deliver.
class RECORDING_LOG : public wxLog
{
protected:
    void DoLogRecord( wxLogLevel aLevel, const wxString& aMessage,
                      const wxLogRecordInfo& ) override
    {
        if( !g.armed )
            return;

        record( { { "fn", aLevel == wxLOG_Trace ? "wxLogTrace" : "wxLogError" },
                  { "message", unitsOf( aMessage ) } } );
    }
};


void recordAssert( const wxString& aFile, int aLine, const wxString& aFunction,
                   const wxString& aCondition, const wxString& aMessage )
{
    if( !g.armed )
        return;

    record( { { "fn", "wxOnAssert" },
              { "line", aLine },
              { "function", aFunction.ToStdString( wxConvUTF8 ) },
              { "condition", aCondition.ToStdString( wxConvUTF8 ) },
              { "message", aMessage.ToStdString( wxConvUTF8 ) } } );
}


void resetState()
{
    g.calls = JSON::array();
    g.replies.clear();
    g.env = JSON::object();
    g.setenvResult = true;
    g.trace = false;
    g.libc = MODE::ASCII;
    g.fileName = MODE::UTF8;
    g.nextHandle = 1;
    g.labels.clear();
    g.byLabel.clear();
    g.settings.clear();
    g.pages.clear();
    g.contexts.clear();
    g.items.clear();
    g.values.clear();
    g.lists.clear();
    g.pending.clear();
    g.connections.clear();
    g.opSettings.clear();
    g.dirents.clear();
    g.buffers.clear();
    g.refusal.clear();
    // Windows and items are kept alive: a reference may still point at one.
}


void readHost( const JSON& aHost )
{
    if( !aHost.is_object() )
        refuse( "expected host object" );

    for( const auto& [key, value] : aHost.items() )
    {
        if( key == "libc" )
        {
            g.libc = readMode( value, "libc" );
        }
        else if( key == "filename" )
        {
            g.fileName = readMode( value, "filename" );
        }
        else if( key == "env" )
        {
            if( !value.is_object() || value.size() > MAX_ENTRIES )
                refuse( "expected env object" );

            for( const auto& [name, setting] : value.items() )
            {
                if( !setting.is_null() )
                    readUnits( setting, "env" );
            }

            g.env = value;
        }
        else if( key == "setenv" )
        {
            g.setenvResult = readBool( value, "setenv" );
        }
        else if( key == "trace" )
        {
            g.trace = readBool( value, "trace" );
        }
        else if( key == "replies" )
        {
            if( !value.is_object() )
                refuse( "expected replies object" );

            for( const auto& [function, queue] : value.items() )
            {
                bool found = false;

                for( const char* name : REPLY_FUNCTIONS )
                    found = found || function == name;

                if( !found )
                    refuse( "unknown reply function: " + function );

                if( !queue.is_array() || queue.size() > MAX_REPLIES )
                    refuse( "expected reply array: " + function );

                g.replies[function] = std::deque<JSON>( queue.begin(), queue.end() );
            }
        }
        else
        {
            refuse( "unknown host key: " + key );
        }
    }
}


// Arms the fakes and installs the case's converters, log target and assertion handler for the
// lifetime of the object.
class ARMED
{
public:
    ARMED() :
            m_libc( g.libc ), m_fileName( g.fileName ), m_oldLibc( wxConvLibcPtr ),
            m_oldFileName( wxConvFileName ), m_oldLog( wxLog::SetActiveTarget( &m_log ) ),
            m_oldAssert( wxSetAssertHandler( recordAssert ) )
    {
        wxConvLibcPtr = &m_libc;
        wxConvFileName = &m_fileName;

        if( g.trace )
            wxLog::AddTraceMask( wxS( "KICAD_PRINT" ) );

        g.armed = true;
    }

    ~ARMED()
    {
        g.armed = false;
        wxLog::RemoveTraceMask( wxS( "KICAD_PRINT" ) );
        wxSetAssertHandler( m_oldAssert );
        wxLog::SetActiveTarget( m_oldLog );
        wxConvFileName = m_oldFileName;
        wxConvLibcPtr = m_oldLibc;
    }

    ARMED( const ARMED& ) = delete;
    ARMED& operator=( const ARMED& ) = delete;

private:
    SCRIPTED_CONV   m_libc;
    SCRIPTED_CONV   m_fileName;
    wxMBConv*       m_oldLibc;
    wxMBConv*       m_oldFileName;
    RECORDING_LOG   m_log;
    wxLog*          m_oldLog;
    wxAssertHandler_t m_oldAssert;
};


wxString argUnits( const JSON& aArgs, const char* aName )
{
    return readUnits( member( aArgs, aName ), aName );
}


JSON result( JSON aExtra = JSON::object() )
{
    if( !g.refusal.empty() )
        throw REFUSAL( g.refusal );

    aExtra["calls"] = g.calls;
    return aExtra;
}


// Runs aRun in a forked child (fresh copy of this process, fakes armed there) and returns its
// reply. Used for printing.cpp's s_lastSettings and for TerminateWithCode.
JSON inChild( const std::function<std::string()>& aRun, int* aStatus )
{
    int fds[2];

    if( pipe( fds ) != 0 )
        throw std::runtime_error( "pipe failed" );

    std::fflush( nullptr );
    const pid_t pid = fork();

    if( pid < 0 )
        throw std::runtime_error( "fork failed" );

    if( pid == 0 )
    {
        close( fds[0] );
        std::string text;

        try
        {
            text = "R" + aRun();
        }
        catch( const REFUSAL& refusal )
        {
            text = std::string( "I" ) + refusal.what();
        }
        catch( const std::exception& error )
        {
            text = std::string( "E" ) + error.what();
        }

        size_t written = 0;

        while( written < text.size() )
        {
            const ssize_t n = write( fds[1], text.data() + written, text.size() - written );

            if( n <= 0 )
                break;

            written += static_cast<size_t>( n );
        }

        _exit( 0 );
    }

    close( fds[1] );
    std::string text;
    char        buffer[4096];

    for( ;; )
    {
        const ssize_t n = read( fds[0], buffer, sizeof buffer );

        if( n <= 0 )
            break;

        text.append( buffer, static_cast<size_t>( n ) );
    }

    close( fds[0] );
    int status = 0;
    waitpid( pid, &status, 0 );

    if( aStatus )
    {
        *aStatus = status;
        return nullptr;
    }

    if( !WIFEXITED( status ) || WEXITSTATUS( status ) != 0 || text.empty() )
        throw std::runtime_error( "child failed" );

    if( text[0] == 'I' )
        refuse( text.substr( 1 ) );

    if( text[0] == 'E' )
        throw std::runtime_error( text.substr( 1 ) );

    return JSON::parse( text.substr( 1 ) );
}


// ---- Operations ---------------------------------------------------------------------------

JSON trivial( const JSON& aArgs )
{
    const std::string function = readText( member( aArgs, "function" ), "function" );
    namespace APP = KIPLATFORM::APP;
    namespace ENV = KIPLATFORM::ENV;
    namespace IO = KIPLATFORM::IO;
    static FAKE_WINDOW window;
    auto windowArg = [&]() -> wxWindow*
    {
        return readBool( member( aArgs, "window" ), "window" ) ? &window : nullptr;
    };

    // Each body runs with the fakes armed; none of them makes a call.
    if( function == "enable_dark_mode" )
    {
        checkKeys( aArgs, { "function", "flag" }, "args" );
        const bool flag = readBool( aArgs.at( "flag" ), "flag" );
        ARMED      armed;
        APP::EnableDarkMode( flag );
        return result( { { "returned", nullptr } } );
    }

    if( function == "attach_console" )
    {
        checkKeys( aArgs, { "function", "flag" }, "args" );
        const bool flag = readBool( aArgs.at( "flag" ), "flag" );
        ARMED      armed;
        return result( { { "returned", APP::AttachConsole( flag ) } } );
    }

    if( function == "is_operating_system_unsupported" || function == "unregister_application_restart"
        || function == "supports_shutdown_block_reason"
        || function == "force_timer_messages_to_be_created_if_necessary"
        || function == "get_app_user_model_id" || function == "get_command_line_str" )
    {
        checkKeys( aArgs, { "function" }, "args" );
        ARMED armed;
        JSON  value;

        if( function == "is_operating_system_unsupported" )
            value = APP::IsOperatingSystemUnsupported();
        else if( function == "unregister_application_restart" )
            value = APP::UnregisterApplicationRestart();
        else if( function == "supports_shutdown_block_reason" )
            value = APP::SupportsShutdownBlockReason();
        else if( function == "force_timer_messages_to_be_created_if_necessary" )
            APP::ForceTimerMessagesToBeCreatedIfNecessary();
        else if( function == "get_app_user_model_id" )
            value = unitsOf( ENV::GetAppUserModelId() );
        else
            value = unitsOf( ENV::GetCommandLineStr() );

        return result( { { "returned", value } } );
    }

    if( function == "register_application_restart" || function == "add_dynamic_library_search_path"
        || function == "is_network_path" || function == "verify_file_signature"
        || function == "add_to_recent_docs" )
    {
        checkKeys( aArgs, { "function", "string" }, "args" );
        const wxString text = argUnits( aArgs, "string" );
        ARMED          armed;
        JSON           value;

        if( function == "register_application_restart" )
            value = APP::RegisterApplicationRestart( text );
        else if( function == "add_dynamic_library_search_path" )
            APP::AddDynamicLibrarySearchPath( text );
        else if( function == "is_network_path" )
            value = ENV::IsNetworkPath( text );
        else if( function == "verify_file_signature" )
            value = ENV::VerifyFileSignature( text );
        else
            ENV::AddToRecentDocs( text );

        return result( { { "returned", value } } );
    }

    if( function == "remove_shutdown_block_reason" )
    {
        checkKeys( aArgs, { "function", "window" }, "args" );
        wxWindow* win = windowArg();
        ARMED     armed;
        APP::RemoveShutdownBlockReason( win );
        return result( { { "returned", nullptr } } );
    }

    if( function == "set_shutdown_block_reason" )
    {
        checkKeys( aArgs, { "function", "window", "string" }, "args" );
        wxWindow*      win = windowArg();
        const wxString text = argUnits( aArgs, "string" );
        ARMED          armed;
        APP::SetShutdownBlockReason( win, text );
        return result( { { "returned", nullptr } } );
    }

    if( function == "set_app_details_for_window" )
    {
        checkKeys( aArgs, { "function", "window", "string", "name" }, "args" );
        wxWindow*      win = windowArg();
        const wxString command = argUnits( aArgs, "string" );
        const wxString name = argUnits( aArgs, "name" );
        ARMED          armed;
        ENV::SetAppDetailsForWindow( win, command, name );
        return result( { { "returned", nullptr } } );
    }

    if( function == "capture_target_attributes" )
    {
        checkKeys( aArgs, { "function", "string" }, "args" );
        const wxString           text = argUnits( aArgs, "string" );
        ARMED                    armed;
        const IO::TARGET_ATTRS   attrs = IO::CaptureTargetAttributes( text );
        return result( { { "returned", { { "value", attrs.value }, { "captured", attrs.captured } } } } );
    }

    if( function == "apply_target_attributes" )
    {
        checkKeys( aArgs, { "function", "string", "value", "captured" }, "args" );
        const wxString   text = argUnits( aArgs, "string" );
        IO::TARGET_ATTRS attrs;
        attrs.value = static_cast<uint32_t>( readInteger( aArgs.at( "value" ), "value", 0, UINT32_MAX ) );
        attrs.captured = readBool( aArgs.at( "captured" ), "captured" );
        ARMED armed;
        return result( { { "returned", IO::ApplyTargetAttributes( text, attrs ) } } );
    }

    if( function == "long_path_adjustment" )
    {
        checkKeys( aArgs, { "function", "string" }, "args" );
        wxFileName name( argUnits( aArgs, "string" ) );
        const wxString before = name.GetFullPath();
        ARMED      armed;
        IO::LongPathAdjustment( name );
        return result( { { "returned", nullptr }, { "changed", name.GetFullPath() != before } } );
    }

    refuse( "unknown trivial function" );
}


JSON appInit( const JSON& aArgs )
{
    checkKeys( aArgs, {}, "args" );
    ARMED armed;
    return result( { { "returned", KIPLATFORM::APP::Init() } } );
}


JSON setCurrentThreadPriority( const JSON& aArgs )
{
    checkKeys( aArgs, { "raise" }, "args" );
    const bool raise = readBool( aArgs.at( "raise" ), "raise" );
    ARMED      armed;
    return result( { { "returned", KIPLATFORM::APP::SetCurrentThreadPriority( raise ) } } );
}


JSON terminateWithCode( const JSON& aArgs )
{
    checkKeys( aArgs, { "code" }, "args" );
    const int code = static_cast<int>( readInteger( aArgs.at( "code" ), "code", INT32_MIN, INT32_MAX ) );
    int       status = 0;
    inChild(
            [&]() -> std::string
            {
                ARMED armed;
                KIPLATFORM::APP::TerminateWithCode( code );
            },
            &status );

    if( !WIFEXITED( status ) )
        throw std::runtime_error( "child did not exit" );

    return { { "calls", JSON::array() }, { "exit_status", WEXITSTATUS( status ) } };
}


JSON envInit( const JSON& aArgs )
{
    checkKeys( aArgs, {}, "args" );
    ARMED armed;
    KIPLATFORM::ENV::Init();
    return result( { { "returned", nullptr } } );
}


JSON buildConfig( const JSON& aArgs )
{
    checkKeys( aArgs, {}, "args" );
#ifdef wxHAS_EGL
    const bool hasEgl = wxHAS_EGL;
#else
    const bool hasEgl = false;
#endif
    return { { "calls", JSON::array() },
             { "wx_at_least_3_3_2", static_cast<bool>( wxCHECK_VERSION( 3, 3, 2 ) ) },
             { "wx_has_egl", hasEgl },
             { "wx_use_glcanvas_egl", static_cast<bool>( wxUSE_GLCANVAS_EGL ) } };
}


JSON moveToTrash( const JSON& aArgs )
{
    checkKeys( aArgs, { "path", "error" }, "args" );
    const wxString path = argUnits( aArgs, "path" );
    wxString       error = argUnits( aArgs, "error" );
    ARMED          armed;
    const bool     value = KIPLATFORM::ENV::MoveToTrash( path, error );
    return result( { { "returned", value }, { "error", unitsOf( error ) } } );
}


JSON isRemovablePath( const JSON& aArgs )
{
    checkKeys( aArgs, { "path" }, "args" );
    const wxString path = argUnits( aArgs, "path" );
    ARMED          armed;
    return result( { { "returned", KIPLATFORM::ENV::IsRemovablePath( path ) } } );
}


JSON userPath( const JSON& aArgs, wxString ( *aFunction )() )
{
    checkKeys( aArgs, {}, "args" );
    ARMED          armed;
    const wxString value = aFunction();
    return result( { { "returned", unitsOf( value ) } } );
}


JSON proxyConfig( const JSON& aArgs )
{
    checkKeys( aArgs, { "url", "host", "username", "password" }, "args" );
    const wxString               url = argUnits( aArgs, "url" );
    KIPLATFORM::ENV::PROXY_CONFIG cfg;
    cfg.host = argUnits( aArgs, "host" );
    cfg.username = argUnits( aArgs, "username" );
    cfg.password = argUnits( aArgs, "password" );
    ARMED      armed;
    const bool value = KIPLATFORM::ENV::GetSystemProxyConfig( url, cfg );
    return result( { { "returned", value },
                     { "host", unitsOf( cfg.host ) },
                     { "username", unitsOf( cfg.username ) },
                     { "password", unitsOf( cfg.password ) } } );
}


JSON seqFOpen( const JSON& aArgs )
{
    checkKeys( aArgs, { "path", "mode" }, "args" );
    const wxString path = argUnits( aArgs, "path" );
    const wxString mode = argUnits( aArgs, "mode" );
    ARMED          armed;
    FILE*          fp = KIPLATFORM::IO::SeqFOpen( path, mode );
    return result( { { "returned", nameOf( fp ) } } );
}


JSON twoPaths( const JSON& aArgs, bool ( *aFunction )( const wxString&, const wxString& ),
               const char* aFirst, const char* aSecond )
{
    checkKeys( aArgs, { aFirst, aSecond }, "args" );
    const wxString first = argUnits( aArgs, aFirst );
    const wxString second = argUnits( aArgs, aSecond );
    ARMED          armed;
    return result( { { "returned", aFunction( first, second ) } } );
}


JSON onePath( const JSON& aArgs, bool ( *aFunction )( const wxString& ) )
{
    checkKeys( aArgs, { "path" }, "args" );
    const wxString path = argUnits( aArgs, "path" );
    ARMED          armed;
    return result( { { "returned", aFunction( path ) } } );
}


// TimestampDir's sum, simulated from the script: true when an addition or multiplication would
// overflow (undefined in the reference).
bool timestampOverflows( const JSON& aHost )
{
    if( !aHost.contains( "replies" ) )
        return false;

    const JSON& replies = aHost.at( "replies" );
    auto        queue = [&]( const char* aName ) -> std::deque<JSON>
    {
        if( !replies.contains( aName ) || !replies.at( aName ).is_array() )
            return {};

        return std::deque<JSON>( replies.at( aName ).begin(), replies.at( aName ).end() );
    };

    std::deque<JSON> dirs = queue( "opendir" ), names = queue( "readdir" ),
                     matches = queue( "fnmatch" ), stats = queue( "lstat" ),
                     links = queue( "readlink" );

    if( dirs.empty() || !dirs.front().is_boolean() || !dirs.front().get<bool>() )
        return false;

    long long timestamp = 0;
    auto      pop = []( std::deque<JSON>& aQueue ) -> JSON
    {
        if( aQueue.empty() )
            return nullptr;

        JSON value = aQueue.front();
        aQueue.pop_front();
        return value;
    };

    for( ;; )
    {
        const JSON name = pop( names );

        if( !name.is_string() )
            return false;

        const JSON match = pop( matches );

        if( !match.is_number_integer() )
            return false;

        if( match.get<long long>() != 0 )
            continue;

        const JSON stat = pop( stats );

        if( !stat.is_object() || !stat.contains( "result" ) || !stat.contains( "mode" )
            || !stat.contains( "mtime" ) || !stat.contains( "size" ) )
        {
            return false;
        }

        auto get = []( const JSON& aStat, const char* aKey ) -> long long
        {
            const JSON& value = aStat.at( aKey );
            return value.is_number_integer() ? value.get<long long>() : 0;
        };

        JSON entry = stat;

        if( get( stat, "result" ) == 0 )
        {
            if( ( get( stat, "mode" ) & S_IFMT ) == S_IFLNK )
            {
                const JSON link = pop( links );

                if( link.is_object() && link.contains( "result" ) && get( link, "result" ) > 0 )
                {
                    const JSON linked = pop( stats );

                    if( linked.is_object() && linked.contains( "result" ) && linked.contains( "mode" )
                        && linked.contains( "mtime" ) && linked.contains( "size" )
                        && get( linked, "result" ) == 0 )
                    {
                        entry = linked;
                    }
                }
            }

            if( ( get( entry, "mode" ) & S_IFMT ) == S_IFREG )
            {
                long long millis;

                if( __builtin_mul_overflow( get( entry, "mtime" ), 1000LL, &millis )
                    || __builtin_add_overflow( timestamp, millis, &timestamp )
                    || __builtin_add_overflow( timestamp, get( entry, "size" ), &timestamp ) )
                {
                    return true;
                }
            }
        }
        else
        {
            const std::string bytes = readHex( name, "reply readdir" );
            const int         hash = static_cast<int>( std::hash<std::string>{}( bytes ) );

            if( __builtin_add_overflow( timestamp, static_cast<long long>( hash ), &timestamp ) )
                return true;
        }
    }
}


JSON timestampDir( const JSON& aArgs, const JSON& aHost )
{
    checkKeys( aArgs, { "dir", "spec" }, "args" );
    const wxString dir = argUnits( aArgs, "dir" );
    const wxString spec = argUnits( aArgs, "spec" );

    if( timestampOverflows( aHost ) )
        refuse( "undefined: signed overflow in TimestampDir" );

    ARMED           armed;
    const long long value = KIPLATFORM::IO::TimestampDir( dir, spec );
    return result( { { "returned", value } } );
}


JSON flushToDisk( const JSON& aArgs )
{
    checkKeys( aArgs, { "null" }, "args" );
    const bool null = readBool( aArgs.at( "null" ), "null" );
    ARMED      armed;
    FILE*      fp = null ? nullptr : static_cast<FILE*>( newHandle( "FILE" ) );
    return result( { { "returned", KIPLATFORM::IO::FlushToDisk( fp ) } } );
}


// The private POSIX members of MAPPED_FILE, read through explicit instantiation (access
// checking does not apply to the names there).
template <typename TAG, typename TAG::TYPE MEMBER>
struct MEMBER_ACCESS
{
    friend typename TAG::TYPE Member( TAG ) { return MEMBER; }
};

struct IS_MAPPED_MEMBER
{
    using TYPE = bool KIPLATFORM::IO::MAPPED_FILE::*;
    friend TYPE Member( IS_MAPPED_MEMBER );
};

struct FALLBACK_MEMBER
{
    using TYPE = std::vector<uint8_t> KIPLATFORM::IO::MAPPED_FILE::*;
    friend TYPE Member( FALLBACK_MEMBER );
};

struct NATIVE_MEMBER
{
    using TYPE = wxPrintNativeDataBase* wxPrintData::*;
    friend TYPE Member( NATIVE_MEMBER );
};

template struct MEMBER_ACCESS<IS_MAPPED_MEMBER, &KIPLATFORM::IO::MAPPED_FILE::m_isMapped>;
template struct MEMBER_ACCESS<FALLBACK_MEMBER, &KIPLATFORM::IO::MAPPED_FILE::m_fallbackBuffer>;
template struct MEMBER_ACCESS<NATIVE_MEMBER, &wxPrintData::m_nativeData>;


JSON mappedFile( const JSON& aArgs )
{
    checkKeys( aArgs, { "name" }, "args" );
    const wxString name = argUnits( aArgs, "name" );
    ARMED          armed;
    JSON           out;

    try
    {
        auto           file = std::make_unique<KIPLATFORM::IO::MAPPED_FILE>( name );
        const uint8_t* data = file->Data();
        const bool     isMapped = ( *file ).*Member( IS_MAPPED_MEMBER() );
        const auto&    fallback = ( *file ).*Member( FALLBACK_MEMBER() );
        JSON           pointsTo = nullptr;

        if( data && known( data ) )
            pointsTo = nameOf( data );
        else if( data && data == fallback.data() )
            pointsTo = "fallback_buffer";

        out = { { "data", data ? JSON( hexOf( data, file->Size() ) ) : JSON( nullptr ) },
                { "size", file->Size() },
                { "is_mapped", isMapped },
                { "points_to", pointsTo },
                { "fallback", hexOf( fallback.data(), fallback.size() ) } };
        file.reset();
    }
    catch( const std::runtime_error& error )
    {
        out = { { "exception", hexOf( error.what(), std::strlen( error.what() ) ) } };
    }

    return result( out );
}


// Each completion invokes a pending callback in order with a new result handle.
void completePending( size_t aCount )
{
    for( size_t i = 0; i < aCount && i < g.pending.size(); ++i )
    {
        PENDING pending = g.pending[i];
        void*   handle = newHandle( "result" );
        pending.callback( nullptr, static_cast<GAsyncResult*>( handle ), pending.userData );
    }
}


JSON secretOp( const std::string& aOp, const JSON& aArgs )
{
    namespace SECRETS = KIPLATFORM::SECRETS;

    if( aOp == "secrets_store" )
    {
        checkKeys( aArgs, { "service", "key", "secret" }, "args" );
        const wxString service = argUnits( aArgs, "service" ), key = argUnits( aArgs, "key" ),
                       secret = argUnits( aArgs, "secret" );
        ARMED armed;
        return result( { { "returned", SECRETS::StoreSecret( service, key, secret ) } } );
    }

    if( aOp == "secrets_store_async" )
    {
        checkKeys( aArgs, { "service", "key", "secret", "done", "complete" }, "args" );
        const wxString service = argUnits( aArgs, "service" ), key = argUnits( aArgs, "key" ),
                       secret = argUnits( aArgs, "secret" );
        const bool hasDone = readBool( aArgs.at( "done" ), "done" );
        const bool complete = readBool( aArgs.at( "complete" ), "complete" );
        ARMED      armed;
        std::function<void( bool )> done;

        if( hasDone )
            done = []( bool aStored ) { record( { { "fn", "done" }, { "stored", aStored } } ); };

        SECRETS::StoreSecretAsync( service, key, secret, std::move( done ) );

        if( complete )
            completePending( 1 );

        return result( { { "returned", nullptr } } );
    }

    if( aOp == "secrets_get" || aOp == "secrets_get_no_prompt" )
    {
        checkKeys( aArgs, { "service", "key", "secret" }, "args" );
        const wxString service = argUnits( aArgs, "service" ), key = argUnits( aArgs, "key" );
        wxString       secret = argUnits( aArgs, "secret" );
        ARMED          armed;
        const bool     value = aOp == "secrets_get"
                                       ? SECRETS::GetSecret( service, key, secret )
                                       : SECRETS::GetSecretNoPrompt( service, key, secret );
        return result( { { "returned", value }, { "secret", unitsOf( secret ) } } );
    }

    if( aOp == "secrets_get_async" )
    {
        checkKeys( aArgs, { "service", "key", "done", "complete" }, "args" );
        const wxString service = argUnits( aArgs, "service" ), key = argUnits( aArgs, "key" );
        const bool     hasDone = readBool( aArgs.at( "done" ), "done" );
        const bool     complete = readBool( aArgs.at( "complete" ), "complete" );
        ARMED          armed;
        std::function<void( bool, const wxString& )> done;

        if( hasDone )
        {
            done = []( bool aFound, const wxString& aValue )
            { record( { { "fn", "done" }, { "found", aFound }, { "value", unitsOf( aValue ) } } ); };
        }

        SECRETS::GetSecretAsync( service, key, std::move( done ) );

        if( complete )
        {
            try
            {
                completePending( 1 );
            }
            catch( const std::bad_function_call& )
            {
                return result( { { "returned", nullptr }, { "exception", "bad_function_call" } } );
            }
        }

        return result( { { "returned", nullptr } } );
    }

    checkKeys( aArgs, { "service", "key" }, "args" );
    const wxString service = argUnits( aArgs, "service" ), key = argUnits( aArgs, "key" );
    ARMED          armed;
    return result( { { "returned", SECRETS::DeleteSecret( service, key ) } } );
}


JSON settingsJson( const void* aSettings )
{
    JSON out = JSON::array();

    for( const auto& [key, value] : g.settings.at( aSettings ) )
        out.push_back( JSON::array( { hexOf( key ), hexOf( value ) } ) );

    return out;
}


JSON printPdf( const JSON& aArgs )
{
    checkKeys( aArgs, { "calls" }, "args" );
    const JSON& calls = aArgs.at( "calls" );

    if( !calls.is_array() || calls.empty() || calls.size() > 8 )
        refuse( "expected 1 to 8 print calls" );

    struct CALL
    {
        std::string file;
        bool        hasPage;
        int         page;
        bool        hasFit;
        bool        fit;
    };

    std::vector<CALL> parsed;

    for( const JSON& call : calls )
    {
        checkKeys( call, { "file", "current_page", "fit" }, "print call" );
        CALL entry{};
        entry.file = readHex( call.at( "file" ), "file" );
        entry.hasPage = !call.at( "current_page" ).is_null();
        entry.page = entry.hasPage ? static_cast<int>( readInteger( call.at( "current_page" ),
                                                                    "current_page", INT32_MIN,
                                                                    INT32_MAX ) )
                                   : 0;
        entry.hasFit = !call.at( "fit" ).is_null();
        entry.fit = entry.hasFit && readBool( call.at( "fit" ), "fit" );

        if( entry.hasFit && !entry.hasPage )
            refuse( "the fit overload has no default page" );

        parsed.push_back( entry );
    }

    // printing.cpp's s_lastSettings starts null in the child.
    return inChild(
            [&]() -> std::string
            {
                namespace PRINTING = KIPLATFORM::PRINTING;
                ARMED armed;
                JSON  returned = JSON::array();

                for( const CALL& call : parsed )
                {
                    PRINTING::PRINT_RESULT value;
                    g.currentFit = !call.hasFit || call.fit;
                    g.inDrawPage = false;

                    if( call.hasFit )
                        value = PRINTING::PrintPDF( call.file, call.fit, call.page );
                    else if( call.hasPage )
                        value = PRINTING::PrintPDF( call.file, call.page );
                    else
                        value = PRINTING::PrintPDF( call.file );

                    returned.push_back( static_cast<int>( value ) );
                }

                JSON settings = JSON::array();

                for( const auto& [pointer, entries] : g.settings )
                {
                    (void) entries;
                    settings.push_back( JSON::array( { nameOf( pointer ), settingsJson( pointer ) } ) );
                }

                std::sort( settings.begin(), settings.end() );
                return result( { { "returned", returned }, { "settings", settings } } ).dump();
            },
            nullptr );
}


JSON resetPrintToFilePath( const JSON& aArgs )
{
    checkKeys( aArgs, { "filename", "native", "settings" }, "args" );
    const wxString    filename = argUnits( aArgs, "filename" );
    const std::string native = readText( aArgs.at( "native" ), "native" );

    if( native != "gtk" && native != "other" )
        refuse( "unknown native data" );

    const SETTINGS entries = readSettings( aArgs.at( "settings" ), "settings" );

    // wxURI::Unescape forms end - 2, before its buffer, for a one-byte "%" path (undefined).
    if( native == "gtk" && entries.count( "output-uri" ) )
    {
        const wxString uri = wxString::FromUTF8( entries.at( "output-uri" ).c_str() );
        wxString       path;

        if( !uri.StartsWith( wxS( "file://" ), &path ) && !uri.StartsWith( wxS( "file:" ), &path ) )
            path = uri;

        if( path == wxS( "%" ) )
            refuse( "undefined: wxURI::Unescape forms end - 2 before the buffer" );
    }

    wxPrintData    data;
    data.SetFilename( filename );
    void* config = nullptr;

    if( native == "gtk" )
    {
        wxPrintNativeDataBase*& slot = data.*Member( NATIVE_MEMBER() );
        delete slot;
        auto* gtk = new wxGtkPrintNativeData;
        slot = gtk;
        config = gtk->GetPrintConfig();
    }

    ARMED armed;

    if( config )
    {
        label( config, "settings" );
        g.settings[config] = entries;
    }

    KIPLATFORM::PRINTING::ResetPrintToFilePath( data );
    JSON out = { { "returned", nullptr }, { "filename", unitsOf( data.GetFilename() ) } };
    out["settings"] = config ? settingsJson( config ) : JSON( nullptr );
    return result( out );
}


JSON run( const JSON& aInput )
{
    checkKeys( aInput, { "op", "args", "host" }, "input" );
    const std::string op = readText( aInput.at( "op" ), "op" );
    const JSON&       args = aInput.at( "args" );
    const JSON&       host = aInput.at( "host" );
    resetState();
    readHost( host );
    namespace ENV = KIPLATFORM::ENV;
    namespace IO = KIPLATFORM::IO;

    if( op == "trivial" )
        return trivial( args );
    if( op == "app_init" )
        return appInit( args );
    if( op == "app_set_current_thread_priority" )
        return setCurrentThreadPriority( args );
    if( op == "app_terminate_with_code" )
        return terminateWithCode( args );
    if( op == "build_config" )
        return buildConfig( args );
    if( op == "env_init" )
        return envInit( args );
    if( op == "env_move_to_trash" )
        return moveToTrash( args );
    if( op == "env_is_removable_path" )
        return isRemovablePath( args );
    if( op == "env_get_documents_path" )
        return userPath( args, ENV::GetDocumentsPath );
    if( op == "env_get_user_config_path" )
        return userPath( args, ENV::GetUserConfigPath );
    if( op == "env_get_user_data_path" )
        return userPath( args, ENV::GetUserDataPath );
    if( op == "env_get_user_local_data_path" )
        return userPath( args, ENV::GetUserLocalDataPath );
    if( op == "env_get_user_cache_path" )
        return userPath( args, ENV::GetUserCachePath );
    if( op == "env_get_system_proxy_config" )
        return proxyConfig( args );
    if( op == "io_seq_f_open" )
        return seqFOpen( args );
    if( op == "io_duplicate_permissions" )
        return twoPaths( args, IO::DuplicatePermissions, "src", "dest" );
    if( op == "io_make_writeable" )
        return onePath( args, IO::MakeWriteable );
    if( op == "io_is_file_hidden" )
        return onePath( args, IO::IsFileHidden );
    if( op == "io_timestamp_dir" )
        return timestampDir( args, host );
    if( op == "io_flush_to_disk" )
        return flushToDisk( args );
    if( op == "io_mapped_file" )
        return mappedFile( args );
    if( op.rfind( "secrets_", 0 ) == 0
        && ( op == "secrets_store" || op == "secrets_store_async" || op == "secrets_get"
             || op == "secrets_get_async" || op == "secrets_get_no_prompt"
             || op == "secrets_delete" ) )
    {
        return secretOp( op, args );
    }
    if( op == "printing_print_pdf" )
        return printPdf( args );
    if( op == "printing_reset" )
        return resetPrintToFilePath( args );

    refuse( "unknown operation" );
}


} // namespace

namespace oracle
{
void RegisterKiplatformUnixUiP001( REGISTRY& aRegistry )
{
    aRegistry.Add( UNIX_UI_UNIT, "libs/kiplatform/os/unix/{app,environment,io,printing,secrets}.cpp",
                   run );
}
} // namespace oracle
