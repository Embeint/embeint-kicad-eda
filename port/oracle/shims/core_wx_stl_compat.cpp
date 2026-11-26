// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.application.thin.replacement.p001: the four bodies of
// the pinned libs/core/wx_stl_compat.cpp, compiled into KiCad's own core library and linked
// here (std::less<wxPoint>::operator(), TowxString and the wxSize/wxPoint stream insertions),
// on the distribution wxWidgets 3.2 wchar_t build. Only validation, conversion, the caller's
// program around the stream (its buffer, an optional throwing num_put facet and an optional
// tied stream), process isolation and serialization live here; every result comes from the
// library bodies and the distribution libstdc++. The schema is in
// rust/kicad-wx-stl-compat-shim/README.md; the Rust adapter mirrors this file's validation
// order and messages.
#include "registry.h"
#include <core/wx_stl_compat.h>
#include <wx/gdicmn.h>
#include <wx/string.h>
#include <algorithm>
#include <cerrno>
#include <climits>
#include <clocale>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <exception>
#include <fcntl.h>
#include <functional>
#include <initializer_list>
#include <ios>
#include <iostream>
#include <locale>
#include <map>
#include <new>
#include <ostream>
#include <stdexcept>
#include <streambuf>
#include <string>
#include <string_view>
#include <sys/wait.h>
#include <type_traits>
#include <unistd.h>
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
constexpr int64_t MAX_SCRIPT_CALL = 64;
// One insertion logs at most 30 events (five sentries, each with two tied syncs, three
// buffer or facet events and one destructor sync); the trace refuses beyond this.
constexpr size_t  MAX_EVENTS = 64;

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


class SINK;

/// The trace of host calls, shared by the main buffer, the tied buffer and the facet. Each
/// event is [kind, n, detail], n being the bytes the main buffer held at the time.
struct EVENTS
{
    const SINK* main = nullptr;
    JSON        list = JSON::array();
    bool        overrun = false;

    void Log( const char* aKind, const JSON& aDetail );
};


/// The caller's stream buffer: no put area, so sputc and the default xsputn reach
/// overflow() byte by byte; it accepts up to a capacity, then returns eof(). sync() counts
/// its calls and returns -1 when configured to fail. The overflow and sync calls with the
/// scripted 0-based indexes throw std::runtime_error instead.
class SINK : public std::streambuf
{
public:
    SINK( size_t aCapacity, bool aSyncFails, int64_t aOverflowThrow, int64_t aSyncThrow,
          EVENTS& aEvents, const char* aSyncName ) :
            m_capacity( aCapacity ), m_syncFails( aSyncFails ), m_overflowThrow( aOverflowThrow ),
            m_syncThrow( aSyncThrow ), m_events( aEvents ), m_syncName( aSyncName )
    {}

    const std::string& Bytes() const { return m_bytes; }
    uint64_t           SyncCalls() const { return m_syncCalls; }

protected:
    int_type overflow( int_type aChar ) override
    {
        if( traits_type::eq_int_type( aChar, traits_type::eof() ) )
            return traits_type::not_eof( aChar );

        const uint64_t index = m_overflowCalls++;

        if( m_overflowThrow >= 0 && index == static_cast<uint64_t>( m_overflowThrow ) )
        {
            m_events.Log( "throw", "overflow" );
            throw std::runtime_error( "overflow" );
        }

        if( m_bytes.size() >= m_capacity )
        {
            m_events.Log( "eof", static_cast<int>( static_cast<unsigned char>( aChar ) ) );
            return traits_type::eof();
        }

        m_bytes.push_back( traits_type::to_char_type( aChar ) );
        return aChar;
    }

    int sync() override
    {
        const uint64_t index = m_syncCalls++;

        if( m_syncThrow >= 0 && index == static_cast<uint64_t>( m_syncThrow ) )
        {
            m_events.Log( "throw", m_syncName );
            throw std::runtime_error( m_syncName );
        }

        const int result = m_syncFails ? -1 : 0;
        m_events.Log( m_syncName, result );
        return result;
    }

private:
    size_t      m_capacity;
    bool        m_syncFails;
    int64_t     m_overflowThrow;
    int64_t     m_syncThrow;
    EVENTS&     m_events;
    const char* m_syncName;
    std::string m_bytes;
    uint64_t    m_overflowCalls = 0;
    uint64_t    m_syncCalls = 0;
};


void EVENTS::Log( const char* aKind, const JSON& aDetail )
{
    if( list.size() >= MAX_EVENTS )
    {
        overrun = true;
        return;
    }

    list.push_back( JSON::array( { aKind, main ? main->Bytes().size() : 0, aDetail } ) );
}


/// A num_put<char> whose do_put( long ) call with the scripted 0-based index throws
/// std::runtime_error, before or after the runtime's own num_put<char>::do_put (an extern
/// template, so the distribution libstdc++'s body and numpunct cache run).
class NUM_PUT : public std::num_put<char>
{
public:
    NUM_PUT( int64_t aCall, bool aAfter, EVENTS& aEvents ) :
            m_call( aCall ), m_after( aAfter ), m_events( aEvents )
    {}

protected:
    iter_type do_put( iter_type aOut, std::ios_base& aIo, char aFill, long aValue ) const override
    {
        const uint64_t index = m_calls++;
        const bool     throws = index == static_cast<uint64_t>( m_call );
        m_events.Log( "do_put", static_cast<int64_t>( aValue ) );

        if( throws && !m_after )
        {
            m_events.Log( "throw", "num_put" );
            throw std::runtime_error( "num_put" );
        }

        iter_type out = std::num_put<char>::do_put( aOut, aIo, aFill, aValue );

        if( throws )
        {
            m_events.Log( "throw", "num_put" );
            throw std::runtime_error( "num_put" );
        }

        return out;
    }

private:
    int64_t          m_call;
    bool             m_after;
    EVENTS&          m_events;
    mutable uint64_t m_calls = 0;
};


/// A scripted call index: null (none) or an integer in [0, aMax].
int64_t CallIndex( const JSON& aValue, int64_t aMax, const char* aMessage )
{
    int64_t index = -1;

    if( !aValue.is_null() && !Integer( aValue, 0, aMax, index ) )
        Fail( aMessage );

    return index;
}


/// The reply for the state reached, from the child or its terminate handler.
std::function<JSON()>* g_terminateReport = nullptr;


[[noreturn]] void WriteAndExit( int aFd, const JSON& aReply )
{
    const std::string text = aReply.dump();
    size_t            done = 0;

    while( done < text.size() )
    {
        const ssize_t wrote = write( aFd, text.data() + done, text.size() - done );

        if( wrote < 0 && errno == EINTR )
            continue;

        if( wrote <= 0 )
            _exit( 4 );

        done += static_cast<size_t>( wrote );
    }

    _exit( 0 );
}


int g_replyFd = -1;


/// The reference process calls std::terminate when a sentry destructor throws (libstdc++ 14:
/// a throwing or failing sync() with badbit in exceptions()). The child reports the state
/// reached at that moment instead of the default handler's abort.
[[noreturn]] void ReportTermination()
{
    if( !g_terminateReport )
        _exit( 4 );

    WriteAndExit( g_replyFd, ( *g_terminateReport )() );
}


/// Runs aRun in a child process, which writes its reply to a pipe. Every insertion runs this
/// way so that a std::terminate inside it is observed, not fatal to the oracle.
JSON Isolated( const std::function<JSON()>& aRun )
{
    int pipeFds[2];

    if( pipe( pipeFds ) != 0 )
        throw std::runtime_error( "insertion pipe failed" );

    std::cout.flush();
    std::cerr.flush();
    std::fflush( nullptr );
    const pid_t pid = fork();

    if( pid < 0 )
        throw std::runtime_error( "insertion fork failed" );

    if( pid == 0 )
    {
        close( pipeFds[0] );
        const int null = open( "/dev/null", O_WRONLY );

        if( null >= 0 )
            dup2( null, STDOUT_FILENO );

        g_replyFd = pipeFds[1];
        std::set_terminate( ReportTermination );

        try
        {
            WriteAndExit( pipeFds[1], aRun() );
        }
        catch( ... )
        {
            _exit( 4 );
        }
    }

    close( pipeFds[1] );
    std::string text;
    char        chunk[4096];

    for( ;; )
    {
        const ssize_t got = read( pipeFds[0], chunk, sizeof( chunk ) );

        if( got < 0 && errno == EINTR )
            continue;

        if( got <= 0 )
            break;

        text.append( chunk, static_cast<size_t>( got ) );
    }

    close( pipeFds[0] );
    int status = 0;

    while( waitpid( pid, &status, 0 ) < 0 )
    {
        if( errno != EINTR )
            throw std::runtime_error( "insertion wait failed" );
    }

    if( !WIFEXITED( status ) || WEXITSTATUS( status ) != 0 || text.empty() )
        throw std::runtime_error( "insertion child failed" );

    JSON reply = JSON::parse( text );

    if( reply.contains( "_overrun" ) )
        Fail( "event limit exceeded" );

    return reply;
}


template <typename VALUE>
JSON Insert( const JSON& aInput )
{
    Keys( aInput, { "op", "value", "stream" }, "unexpected or missing field" );
    const auto [x, y] = Point( aInput.at( "value" ) );
    const JSON& s = aInput.at( "stream" );

    // Ten required fields and an optional "host".
    if( !s.is_object() )
        Fail( "unexpected or missing stream field" );

    const bool scripted = s.contains( "host" );

    if( s.size() != 10 + ( scripted ? 1 : 0 ) )
        Fail( "unexpected or missing stream field" );

    for( const char* field : { "state", "exceptions", "flags", "width", "precision", "fill",
                               "thousands_sep", "grouping", "capacity", "sync_fails" } )
    {
        if( !s.contains( field ) )
            Fail( "unexpected or missing stream field" );
    }

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

    // The host scripts: which overflow/sync/do_put call throws, and the tied stream.
    int64_t            overflowThrow = -1;
    int64_t            syncThrow = -1;
    int64_t            putCall = -1;
    bool               putAfter = false;
    bool               tied = false;
    std::ios::iostate  tieState{};
    std::ios::iostate  tieMask{};
    std::ios::fmtflags tieFlags{};
    bool               tieSyncFails = false;
    int64_t            tieSyncThrow = -1;

    if( scripted )
    {
        const JSON& host = s.at( "host" );
        Keys( host, { "overflow_throw", "sync_throw", "num_put", "tie" }, "unexpected or missing host field" );
        overflowThrow = CallIndex( host.at( "overflow_throw" ), static_cast<int64_t>( MAX_SINK_CAPACITY ),
                                   "expected overflow_throw" );
        syncThrow = CallIndex( host.at( "sync_throw" ), MAX_SCRIPT_CALL, "expected sync_throw" );
        const JSON& put = host.at( "num_put" );

        if( !put.is_null() )
        {
            Keys( put, { "call", "throw" }, "expected num_put" );

            if( !Integer( put.at( "call" ), 0, MAX_SCRIPT_CALL, putCall ) || !put.at( "throw" ).is_string() )
                Fail( "expected num_put" );

            const std::string& when = put.at( "throw" ).get_ref<const std::string&>();

            if( when != "before" && when != "after" )
                Fail( "expected num_put" );

            putAfter = when == "after";
        }

        const JSON& tie = host.at( "tie" );

        if( !tie.is_null() )
        {
            Keys( tie, { "state", "exceptions", "flags", "sync_fails", "sync_throw" },
                  "unexpected or missing tie field" );
            tied = true;
            tieState = Names<std::ios::iostate>( tie.at( "state" ), IOSTATE, "expected iostate" );
            tieMask = Names<std::ios::iostate>( tie.at( "exceptions" ), IOSTATE, "expected iostate" );
            tieFlags = Names<std::ios::fmtflags>( tie.at( "flags" ), FMTFLAGS, "expected fmtflags" );

            if( !tie.at( "sync_fails" ).is_boolean() )
                Fail( "expected sync_fails" );

            tieSyncFails = tie.at( "sync_fails" ).get<bool>();
            tieSyncThrow = CallIndex( tie.at( "sync_throw" ), MAX_SCRIPT_CALL, "expected sync_throw" );
        }
    }

    if( !bounded && width > MAX_UNBOUNDED_WIDTH )
        Fail( "unbounded width exceeds limit" );

    return Isolated(
            [&]() -> JSON
            {
                EVENTS events;
                SINK   sink( bounded ? static_cast<size_t>( capacity ) : SIZE_MAX, syncFails, overflowThrow,
                             syncThrow, events, "sync" );
                events.main = &sink;

                // The tied stream: its flags, clear( state ), then exceptions( mask ), whose
                // throw leaves the mask set and is caught.
                SINK         tieSink( SIZE_MAX, tieSyncFails, -1, tieSyncThrow, events, "tie_sync" );
                std::ostream tiedStream( &tieSink );
                bool         tieSetupFailure = false;

                if( tied )
                {
                    tiedStream.flags( tieFlags );
                    tiedStream.clear( tieState );

                    try
                    {
                        tiedStream.exceptions( tieMask );
                    }
                    catch( const std::ios_base::failure& )
                    {
                        tieSetupFailure = true;
                    }
                }

                std::ostream out( &sink );
                std::locale  locale( std::locale::classic(), new NUMPUNCT( separator, grouping ) );

                if( putCall >= 0 )
                    locale = std::locale( locale, new NUM_PUT( putCall, putAfter, events ) );

                out.imbue( locale );
                out.flags( flags );
                out.width( width );
                out.precision( precision );
                out.fill( fill );

                if( tied )
                    out.tie( &tiedStream );

                out.clear( state );
                bool setupFailure = false;

                try
                {
                    out.exceptions( mask );
                }
                catch( const std::ios_base::failure& )
                {
                    setupFailure = true;
                }

                auto report = [&]( const char* aOutcome, const char* aWhat, bool aSame ) -> JSON
                {
                    JSON reply{ { "bytes", Hex( sink.Bytes() ) },
                                { "exceptions", EmitNames( out.exceptions(), IOSTATE ) },
                                { "fill", static_cast<int>( static_cast<unsigned char>( out.fill() ) ) },
                                { "flags", EmitNames( out.flags(), FMTFLAGS ) },
                                { "outcome", aOutcome },
                                { "precision", static_cast<int64_t>( out.precision() ) },
                                { "same_stream", aSame },
                                { "state", EmitNames( out.rdstate(), IOSTATE ) },
                                { "sync_calls", sink.SyncCalls() },
                                { "width", static_cast<int64_t>( out.width() ) } };

                    if( setupFailure )
                        reply["setup_failure"] = true;

                    if( aWhat )
                        reply["what"] = aWhat;

                    if( scripted )
                    {
                        reply["events"] = events.list;

                        if( tied )
                        {
                            reply["tie"] = JSON{ { "exceptions", EmitNames( tiedStream.exceptions(), IOSTATE ) },
                                                 { "setup_failure", tieSetupFailure },
                                                 { "state", EmitNames( tiedStream.rdstate(), IOSTATE ) },
                                                 { "sync_calls", tieSink.SyncCalls() } };
                        }
                    }

                    if( events.overrun )
                        reply["_overrun"] = true;

                    return reply;
                };

                std::function<JSON()> onTerminate = [&]() { return report( "terminate", nullptr, true ); };
                g_terminateReport = &onTerminate;
                const VALUE value( x, y );

                try
                {
                    std::ostream& ( *insert )( std::ostream&, const VALUE& ) = &operator<<;
                    const bool same = &insert( out, value ) == &out;
                    return report( "returned", nullptr, same );
                }
                catch( const std::ios_base::failure& )
                {
                    return report( "ios_failure", nullptr, true );
                }
                catch( const std::runtime_error& error )
                {
                    // Copied: what() of the propagated object itself.
                    const std::string what = error.what();
                    return report( "runtime_error", what.c_str(), true );
                }
                catch( const std::bad_alloc& )
                {
                    return report( "bad_alloc", nullptr, true );
                }
            } );
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
