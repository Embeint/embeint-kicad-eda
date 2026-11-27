// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.application.thin.replacement.p001: the four bodies of
// the pinned libs/core/wx_stl_compat.cpp, compiled into KiCad's own core library and linked
// here (std::less<wxPoint>::operator(), TowxString and the wxSize/wxPoint stream insertions),
// on the distribution wxWidgets 3.2 wchar_t build. Only validation, conversion, the caller's
// program around the stream (its buffer, an optional throwing num_put facet and an optional
// tied stream), the call context (an insertion run from a destructor while an exception
// propagates, or on a thread that a host operation cancels), process isolation and
// serialization live here; every result comes from the library bodies and the
// distribution libstdc++. The schema is in
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
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <fcntl.h>
#include <functional>
#include <initializer_list>
#include <ios>
#include <iostream>
#include <locale>
#include <map>
#include <memory>
#include <new>
#include <ostream>
#include <pthread.h>
#include <stdexcept>
#include <streambuf>
#include <string>
#include <string_view>
#include <sys/mman.h>
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
static_assert( sizeof( long ) == 8, "num_put<char>::do_put( long ) values are 64-bit" );


/// The exit status of a child that received SIGABRT.
constexpr int WX_STL_COMPAT_ABORT_STATUS = 5;

extern "C"
{
/// The child's SIGABRT handler. It only ends the process (std::_Exit is signal-safe); the
/// oracle process builds the reply from the shared record after the child has ended.
[[noreturn]] static void WxStlCompatAbortExit( int )
{
    std::_Exit( WX_STL_COMPAT_ABORT_STATUS );
}
}


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
// The largest width the scripted facet sets: the stock do_put pads in alloca storage.
constexpr int64_t MAX_FACET_WIDTH = 4096;
constexpr int64_t MAX_DELEGATE = 2;
// One insertion logs at most 36 events (five sentries, each with two tied syncs, the
// buffer, facet and cancellation events and one destructor sync); the trace refuses beyond
// this.
constexpr size_t  MAX_EVENTS = 64;
// The largest serialized cancellation snapshot: the hexadecimal sink bytes (at most 2^20
// bytes and a few padded fields) with the trace and the fixed fields. A larger one is a
// harness failure.
constexpr size_t  MAX_SNAPSHOT = size_t( 6 ) << 20;

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


/// An object with every required field and no field outside the required and optional ones.
void KeysWith( const JSON& aValue, std::initializer_list<const char*> aRequired,
               std::initializer_list<const char*> aOptional, const char* aMessage )
{
    if( !aValue.is_object() )
        Fail( aMessage );

    for( const char* field : aRequired )
    {
        if( !aValue.contains( field ) )
            Fail( aMessage );
    }

    for( const auto& item : aValue.items() )
    {
        bool known = false;

        for( const char* field : aRequired )
            known = known || item.key() == field;

        for( const char* field : aOptional )
            known = known || item.key() == field;

        if( !known )
            Fail( aMessage );
    }
}


/// An optional boolean field; absent is false.
bool OptionalBool( const JSON& aValue, const char* aField, const char* aMessage )
{
    if( !aValue.contains( aField ) )
        return false;

    if( !aValue.at( aField ).is_boolean() )
        Fail( aMessage );

    return aValue.at( aField ).get<bool>();
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


/// What a child shares with the oracle process, so that an insertion that ends in an abort
/// is reported without any work in a signal handler. Each stream is constructed in its
/// slot, so that its ios_base and basic_ios members are read after the child has ended. The
/// host fields (sink bytes, sync counts, trace, setup results) are a snapshot the host takes
/// in ordinary context when it cancels the insertion's thread: an abort is only reached
/// from there through the runtime's destructor catch( ... ), which runs no host code, and a
/// host call after the snapshot marks it stale.
struct ABORT_RECORD
{
    alignas( std::ostream ) unsigned char main[sizeof( std::ostream )];
    alignas( std::ostream ) unsigned char tie[sizeof( std::ostream )];
    bool   cancelled = false;
    bool   stale = false;
    bool   oversized = false;
    size_t length = 0;
    /// The serialized reply at the cancellation; only its first `length` bytes are written.
    char   snapshot[MAX_SNAPSHOT];
};


/// The child's record and the snapshot its host takes on cancelling the thread.
ABORT_RECORD*          g_abortRecord = nullptr;
std::function<void()>* g_cancelSnapshot = nullptr;


/// Called at the start of every host operation: one after a cancellation snapshot makes
/// that snapshot stale.
void HostCall()
{
    if( g_abortRecord && g_abortRecord->cancelled )
        g_abortRecord->stale = true;
}


/// Cancels the calling thread at a scripted host call: pthread_testcancel() then throws the
/// forced unwind (abi::__forced_unwind) out of the host operation. Only an insertion run on
/// its own thread is scripted to reach this. The host fields are recorded first.
[[noreturn]] void ForceUnwind()
{
    if( g_cancelSnapshot )
        ( *g_cancelSnapshot )();

    pthread_setcancelstate( PTHREAD_CANCEL_ENABLE, nullptr );
    pthread_setcanceltype( PTHREAD_CANCEL_DEFERRED, nullptr );
    pthread_cancel( pthread_self() );
    pthread_testcancel();
    _exit( 4 );
}


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
/// scripted 0-based indexes cancel the thread, or throw std::runtime_error, instead.
class SINK : public std::streambuf
{
public:
    SINK( size_t aCapacity, bool aSyncFails, int64_t aOverflowThrow, int64_t aSyncThrow,
          int64_t aOverflowCancel, int64_t aSyncCancel, EVENTS& aEvents, const char* aSyncName ) :
            m_capacity( aCapacity ), m_syncFails( aSyncFails ), m_overflowThrow( aOverflowThrow ),
            m_syncThrow( aSyncThrow ), m_overflowCancel( aOverflowCancel ), m_syncCancel( aSyncCancel ),
            m_events( aEvents ), m_syncName( aSyncName )
    {}

    const std::string& Bytes() const { return m_bytes; }
    uint64_t           SyncCalls() const { return m_syncCalls; }

protected:
    int_type overflow( int_type aChar ) override
    {
        HostCall();

        if( traits_type::eq_int_type( aChar, traits_type::eof() ) )
            return traits_type::not_eof( aChar );

        const uint64_t index = m_overflowCalls++;

        if( m_overflowCancel >= 0 && index == static_cast<uint64_t>( m_overflowCancel ) )
        {
            m_events.Log( "cancel", "overflow" );
            ForceUnwind();
        }

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
        HostCall();
        const uint64_t index = m_syncCalls++;

        if( m_syncCancel >= 0 && index == static_cast<uint64_t>( m_syncCancel ) )
        {
            m_events.Log( "cancel", m_syncName );
            ForceUnwind();
        }

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
    int64_t     m_overflowCancel;
    int64_t     m_syncCancel;
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


/// Changes a scripted facet makes to its ios_base& (the stream being written), each when
/// given: width( w ), precision( p ), flags( f ).
struct IOS_CHANGE
{
    bool               setWidth = false;
    int64_t            width = 0;
    bool               setPrecision = false;
    int64_t            precision = 0;
    bool               setFlags = false;
    std::ios::fmtflags flags{};

    void Apply( std::ios_base& aIo ) const
    {
        if( setWidth )
            aIo.width( width );

        if( setPrecision )
            aIo.precision( precision );

        if( setFlags )
            aIo.flags( flags );
    }
};


enum class PUT_THROW
{
    BEFORE,
    AFTER,
    NONE
};


/// The facet script; see NUM_PUT.
struct PUT_SCRIPT
{
    bool       present = false;
    int64_t    call = -1;
    PUT_THROW  when = PUT_THROW::NONE;
    IOS_CHANGE before;
    IOS_CHANGE after;
    bool       setFill = false;
    char       fill = 0;
    bool       setValue = false;
    long       value = 0;
    int64_t    delegate = 1;
    int64_t    cancel = -1;
};


/// A num_put<char> whose do_put( long ) logs each call, cancels the thread at the scripted
/// cancellation call, makes the set_before changes to aIo, throws std::runtime_error at the
/// scripted call when that is before, delegates `delegate` times to the stock body (with the
/// scripted fill and value when given), makes the set_after changes, throws at the scripted
/// call when that is after, and returns the last iterator. The stock body is the classic
/// locale's num_put<char> object, reached through the runtime's use_facet and a virtual
/// put(): a qualified call of the inline num_put<char>::do_put would instantiate
/// _M_insert_int and its __use_cache<__numpunct_cache<char>> here, and that copy would
/// interpose the runtime's (whose numpunct twin ends the grouping at a NUL) for every
/// stream in the process. The adapter's object is checked to contain neither.
class NUM_PUT : public std::num_put<char>
{
public:
    NUM_PUT( const PUT_SCRIPT& aScript, EVENTS& aEvents ) :
            m_script( aScript ), m_events( aEvents ),
            m_stock( std::use_facet<std::num_put<char>>( std::locale::classic() ) )
    {}

protected:
    iter_type do_put( iter_type aOut, std::ios_base& aIo, char aFill, long aValue ) const override
    {
        HostCall();
        const uint64_t index = m_calls++;
        m_events.Log( "do_put", static_cast<int64_t>( aValue ) );

        if( m_script.cancel >= 0 && index == static_cast<uint64_t>( m_script.cancel ) )
        {
            m_events.Log( "cancel", "num_put" );
            ForceUnwind();
        }

        m_script.before.Apply( aIo );
        const bool throws = m_script.when != PUT_THROW::NONE && index == static_cast<uint64_t>( m_script.call );

        if( throws && m_script.when == PUT_THROW::BEFORE )
        {
            m_events.Log( "throw", "num_put" );
            throw std::runtime_error( "num_put" );
        }

        // The stock facet formats with aIo's flags, width and locale (its numpunct), as the
        // base body does.
        const char fill = m_script.setFill ? m_script.fill : aFill;
        const long value = m_script.setValue ? m_script.value : aValue;
        iter_type  out = aOut;

        for( int64_t i = 0; i < m_script.delegate; ++i )
            out = m_stock.put( out, aIo, fill, value );

        m_script.after.Apply( aIo );

        if( throws )
        {
            m_events.Log( "throw", "num_put" );
            throw std::runtime_error( "num_put" );
        }

        return out;
    }

private:
    PUT_SCRIPT                m_script;
    EVENTS&                   m_events;
    const std::num_put<char>& m_stock;
    mutable uint64_t          m_calls = 0;
};


/// set_before/set_after of a num_put script: {"width", "precision", "flags"}, each null or
/// a value.
IOS_CHANGE IosChange( const JSON& aPut, const char* aField )
{
    IOS_CHANGE change;

    if( !aPut.contains( aField ) )
        return change;

    const JSON& value = aPut.at( aField );
    Keys( value, { "width", "precision", "flags" }, "expected num_put" );

    if( !value.at( "width" ).is_null() )
    {
        if( !Integer( value.at( "width" ), INT64_MIN, MAX_FACET_WIDTH, change.width ) )
            Fail( "expected num_put" );

        change.setWidth = true;
    }

    if( !value.at( "precision" ).is_null() )
    {
        if( !Integer( value.at( "precision" ), INT64_MIN, INT64_MAX, change.precision ) )
            Fail( "expected num_put" );

        change.setPrecision = true;
    }

    if( !value.at( "flags" ).is_null() )
    {
        change.flags = Names<std::ios::fmtflags>( value.at( "flags" ), FMTFLAGS, "expected num_put" );
        change.setFlags = true;
    }

    return change;
}


/// A scripted call index: null (none) or an integer in [0, aMax].
int64_t CallIndex( const JSON& aValue, int64_t aMax, const char* aMessage )
{
    int64_t index = -1;

    if( !aValue.is_null() && !Integer( aValue, 0, aMax, index ) )
        Fail( aMessage );

    return index;
}


/// The reply for the state reached, from the child's terminate handler.
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


/// Gives private libstdc++ members, through an explicit instantiation (which may name
/// them), as member pointers: the stream fields read from an ended child's stream image.
template <typename TAG, typename TAG::TYPE MEMBER>
struct PRIVATE_MEMBER
{
    friend typename TAG::TYPE Member( TAG ) { return MEMBER; }
};

#define WX_STL_COMPAT_MEMBER( TAG, CLASS, TYPE_, NAME )                                     \
    struct TAG                                                                              \
    {                                                                                       \
        using TYPE = TYPE_ CLASS::*;                                                        \
        friend TYPE Member( TAG );                                                          \
    };                                                                                      \
    template struct PRIVATE_MEMBER<TAG, &CLASS::NAME>

WX_STL_COMPAT_MEMBER( IOS_STATE, std::ios_base, std::ios::iostate, _M_streambuf_state );
WX_STL_COMPAT_MEMBER( IOS_EXCEPTIONS, std::ios_base, std::ios::iostate, _M_exception );
WX_STL_COMPAT_MEMBER( IOS_FLAGS, std::ios_base, std::ios::fmtflags, _M_flags );
WX_STL_COMPAT_MEMBER( IOS_WIDTH, std::ios_base, std::streamsize, _M_width );
WX_STL_COMPAT_MEMBER( IOS_PRECISION, std::ios_base, std::streamsize, _M_precision );
WX_STL_COMPAT_MEMBER( IOS_FILL, std::basic_ios<char>, char, _M_fill );
WX_STL_COMPAT_MEMBER( IOS_FILL_INIT, std::basic_ios<char>, bool, _M_fill_init );
#undef WX_STL_COMPAT_MEMBER


/// Byte offsets of the stream fields within a complete std::ostream object, measured on a
/// probe of the same type (its virtual ios_base base has the same place in every one).
struct IMAGE_OFFSETS
{
    size_t state, exceptions, flags, width, precision, fill, fillInit;
};


const IMAGE_OFFSETS& ImageOffsets()
{
    static const IMAGE_OFFSETS offsets = []()
    {
        const std::ostream                probe( nullptr );
        const std::ios_base&              base = probe;
        const std::basic_ios<char>&       ios = probe;
        const uintptr_t                   start = reinterpret_cast<uintptr_t>( &probe );
        auto at = [&]( const void* aField ) { return static_cast<size_t>( reinterpret_cast<uintptr_t>( aField ) - start ); };

        return IMAGE_OFFSETS{ at( &( base.*Member( IOS_STATE{} ) ) ),
                              at( &( base.*Member( IOS_EXCEPTIONS{} ) ) ),
                              at( &( base.*Member( IOS_FLAGS{} ) ) ),
                              at( &( base.*Member( IOS_WIDTH{} ) ) ),
                              at( &( base.*Member( IOS_PRECISION{} ) ) ),
                              at( &( ios.*Member( IOS_FILL{} ) ) ),
                              at( &( ios.*Member( IOS_FILL_INIT{} ) ) ) };
    }();

    return offsets;
}


template <typename T>
T ImageValue( const unsigned char* aSlot, size_t aOffset )
{
    static_assert( std::is_trivially_copyable_v<T> );
    T value;
    std::memcpy( &value, aSlot + aOffset, sizeof( value ) );
    return value;
}


/// Replaces the stream fields of a reply with those of the ended child's stream image: the
/// state the runtime reached, including what its destructor did after the last host call.
void ImageFields( JSON& aReply, const unsigned char* aSlot, bool aMain )
{
    const IMAGE_OFFSETS& at = ImageOffsets();
    aReply["state"] = EmitNames( ImageValue<std::ios::iostate>( aSlot, at.state ), IOSTATE );
    aReply["exceptions"] = EmitNames( ImageValue<std::ios::iostate>( aSlot, at.exceptions ), IOSTATE );

    if( !aMain )
        return;

    // fill() would widen a fill that was never set; the adapter always sets it.
    if( !ImageValue<bool>( aSlot, at.fillInit ) )
        throw std::runtime_error( "aborted stream fill not initialised" );

    aReply["fill"] = static_cast<int>( static_cast<unsigned char>( ImageValue<char>( aSlot, at.fill ) ) );
    aReply["flags"] = EmitNames( ImageValue<std::ios::fmtflags>( aSlot, at.flags ), FMTFLAGS );
    aReply["precision"] = static_cast<int64_t>( ImageValue<std::streamsize>( aSlot, at.precision ) );
    aReply["width"] = static_cast<int64_t>( ImageValue<std::streamsize>( aSlot, at.width ) );
}


/// The shared record of one insertion: an anonymous shared mapping that the child inherits
/// and the oracle process reads after the child has ended. The snapshot pages are only
/// touched when written.
class SHARED_RECORD
{
public:
    SHARED_RECORD()
    {
        void* memory = mmap( nullptr, sizeof( ABORT_RECORD ), PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS,
                             -1, 0 );

        if( memory == MAP_FAILED )
            throw std::runtime_error( "insertion record mapping failed" );

        m_record = new( memory ) ABORT_RECORD;
    }

    ~SHARED_RECORD()
    {
        std::destroy_at( m_record );
        munmap( m_record, sizeof( ABORT_RECORD ) );
    }

    SHARED_RECORD( const SHARED_RECORD& ) = delete;
    SHARED_RECORD& operator=( const SHARED_RECORD& ) = delete;

    ABORT_RECORD& operator*() const { return *m_record; }
    ABORT_RECORD* operator->() const { return m_record; }

private:
    ABORT_RECORD* m_record = nullptr;
};


/// A std::ostream constructed in a slot of the shared record, destroyed with this holder.
struct SHARED_STREAM
{
    SHARED_STREAM( unsigned char* aSlot, std::streambuf* aBuffer ) :
            stream( *new( aSlot ) std::ostream( aBuffer ) )
    {}

    ~SHARED_STREAM() { std::destroy_at( &stream ); }

    SHARED_STREAM( const SHARED_STREAM& ) = delete;
    SHARED_STREAM& operator=( const SHARED_STREAM& ) = delete;

    std::ostream& stream;
};


/// The thread body for an insertion that a host operation may cancel.
void* RunInsertion( void* aBody )
{
    ( *static_cast<std::function<void()>*>( aBody ) )();
    return nullptr;
}


/// Runs the insertion in its destructor, while the exception thrown in its scope
/// propagates: std::uncaught_exceptions() is 1 throughout the insertion.
struct DURING_UNWINDING
{
    const std::function<void()>& body;

    ~DURING_UNWINDING()
    {
        if( std::uncaught_exceptions() != 1 )
            _exit( 4 );

        body();
    }
};


/// Reads a file descriptor to its end.
std::string ReadAll( int aFd )
{
    std::string text;
    char        chunk[4096];

    for( ;; )
    {
        const ssize_t got = read( aFd, chunk, sizeof( chunk ) );

        if( got < 0 && errno == EINTR )
            continue;

        if( got <= 0 )
            break;

        text.append( chunk, static_cast<size_t>( got ) );
    }

    return text;
}


/// Runs aRun in a child process, which writes its reply to a pipe. Every insertion runs this
/// way so that a std::terminate inside it is observed, not fatal to the oracle. A child that
/// receives SIGABRT only exits with WX_STL_COMPAT_ABORT_STATUS; if glibc reported a forced
/// unwind that was not rethrown, aAborted builds the reply here, outside any signal handler,
/// from what the ended child left in shared memory. The child's diagnostics are captured and
/// copied to this process's stderr.
JSON Isolated( const std::function<JSON()>& aRun, const std::function<JSON()>& aAborted )
{
    const int errorFd = memfd_create( "wx-stl-compat-stderr", MFD_CLOEXEC );

    if( errorFd < 0 )
        throw std::runtime_error( "insertion stderr capture failed" );

    int pipeFds[2];

    if( pipe( pipeFds ) != 0 )
    {
        close( errorFd );
        throw std::runtime_error( "insertion pipe failed" );
    }

    std::cout.flush();
    std::cerr.flush();
    std::fflush( nullptr );
    const pid_t pid = fork();

    if( pid < 0 )
    {
        close( pipeFds[0] );
        close( pipeFds[1] );
        close( errorFd );
        throw std::runtime_error( "insertion fork failed" );
    }

    if( pid == 0 )
    {
        close( pipeFds[0] );
        const int null = open( "/dev/null", O_WRONLY );

        if( null >= 0 )
            dup2( null, STDOUT_FILENO );

        if( dup2( errorFd, STDERR_FILENO ) < 0 )
            _exit( 4 );

        g_replyFd = pipeFds[1];
        std::set_terminate( ReportTermination );
        std::signal( SIGABRT, WxStlCompatAbortExit );

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
    const std::string text = ReadAll( pipeFds[0] );
    close( pipeFds[0] );
    int status = 0;

    while( waitpid( pid, &status, 0 ) < 0 )
    {
        if( errno != EINTR )
        {
            close( errorFd );
            throw std::runtime_error( "insertion wait failed" );
        }
    }

    // The child's writes moved the shared file offset to their end.
    const std::string errors = lseek( errorFd, 0, SEEK_SET ) == 0 ? ReadAll( errorFd ) : std::string();
    close( errorFd );
    std::cerr << errors;
    JSON reply;

    if( WIFEXITED( status ) && WEXITSTATUS( status ) == WX_STL_COMPAT_ABORT_STATUS )
    {
        // nptl/unwind.c unwind_cleanup: __libc_fatal, which writes this and aborts.
        if( !text.empty() || errors.find( "FATAL: exception not rethrown\n" ) == std::string::npos )
            throw std::runtime_error( "insertion child aborted without an unrethrown forced unwind" );

        reply = aAborted();
    }
    else
    {
        if( !WIFEXITED( status ) || WEXITSTATUS( status ) != 0 || text.empty() )
            throw std::runtime_error( "insertion child failed" );

        reply = JSON::parse( text );
    }

    if( reply.contains( "_overrun" ) )
        Fail( "event limit exceeded" );

    return reply;
}


template <typename VALUE>
JSON Insert( const JSON& aInput )
{
    KeysWith( aInput, { "op", "value", "stream" }, { "unwinding" }, "unexpected or missing field" );
    const auto [x, y] = Point( aInput.at( "value" ) );
    // Run the operator from a destructor while std::runtime_error( "outer" ) propagates.
    const bool  unwinding = OptionalBool( aInput, "unwinding", "expected unwinding" );
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

    // The host scripts: which overflow/sync/do_put call throws or cancels the thread, the
    // facet's changes, the tied stream and null buffers.
    int64_t            overflowThrow = -1;
    int64_t            syncThrow = -1;
    PUT_SCRIPT         put;
    bool               tied = false;
    std::ios::iostate  tieState{};
    std::ios::iostate  tieMask{};
    std::ios::fmtflags tieFlags{};
    bool               tieSyncFails = false;
    int64_t            tieSyncThrow = -1;
    bool               tieNullBuffer = false;
    bool               nullBuffer = false;
    bool               cancelled = false;
    int64_t            cancelOverflow = -1;
    int64_t            cancelSync = -1;
    int64_t            cancelTieSync = -1;

    if( scripted )
    {
        const JSON& host = s.at( "host" );
        KeysWith( host, { "overflow_throw", "sync_throw", "num_put", "tie" }, { "null_rdbuf", "cancel" },
                  "unexpected or missing host field" );
        overflowThrow = CallIndex( host.at( "overflow_throw" ), static_cast<int64_t>( MAX_SINK_CAPACITY ),
                                   "expected overflow_throw" );
        syncThrow = CallIndex( host.at( "sync_throw" ), MAX_SCRIPT_CALL, "expected sync_throw" );
        const JSON& putJson = host.at( "num_put" );

        if( !putJson.is_null() )
        {
            KeysWith( putJson, { "call", "throw" }, { "set_before", "set_after", "fill", "value", "delegate" },
                      "expected num_put" );
            put.present = true;

            if( !Integer( putJson.at( "call" ), 0, MAX_SCRIPT_CALL, put.call ) || !putJson.at( "throw" ).is_string() )
                Fail( "expected num_put" );

            const std::string& when = putJson.at( "throw" ).get_ref<const std::string&>();

            if( when == "before" )
                put.when = PUT_THROW::BEFORE;
            else if( when == "after" )
                put.when = PUT_THROW::AFTER;
            else if( when == "none" )
                put.when = PUT_THROW::NONE;
            else
                Fail( "expected num_put" );

            put.before = IosChange( putJson, "set_before" );
            put.after = IosChange( putJson, "set_after" );

            if( putJson.contains( "fill" ) )
            {
                put.fill = Byte( putJson.at( "fill" ), "expected num_put" );
                put.setFill = true;
            }

            if( putJson.contains( "value" ) )
            {
                int64_t value = 0;

                if( !Integer( putJson.at( "value" ), INT64_MIN, INT64_MAX, value ) )
                    Fail( "expected num_put" );

                put.value = static_cast<long>( value );
                put.setValue = true;
            }

            if( putJson.contains( "delegate" ) && !Integer( putJson.at( "delegate" ), 0, MAX_DELEGATE, put.delegate ) )
                Fail( "expected num_put" );
        }

        const JSON& tie = host.at( "tie" );

        if( !tie.is_null() )
        {
            KeysWith( tie, { "state", "exceptions", "flags", "sync_fails", "sync_throw" }, { "null_rdbuf" },
                      "unexpected or missing tie field" );
            tied = true;
            tieState = Names<std::ios::iostate>( tie.at( "state" ), IOSTATE, "expected iostate" );
            tieMask = Names<std::ios::iostate>( tie.at( "exceptions" ), IOSTATE, "expected iostate" );
            tieFlags = Names<std::ios::fmtflags>( tie.at( "flags" ), FMTFLAGS, "expected fmtflags" );

            if( !tie.at( "sync_fails" ).is_boolean() )
                Fail( "expected sync_fails" );

            tieSyncFails = tie.at( "sync_fails" ).get<bool>();
            tieSyncThrow = CallIndex( tie.at( "sync_throw" ), MAX_SCRIPT_CALL, "expected sync_throw" );
            tieNullBuffer = OptionalBool( tie, "null_rdbuf", "expected null_rdbuf" );
        }

        nullBuffer = OptionalBool( host, "null_rdbuf", "expected null_rdbuf" );

        if( host.contains( "cancel" ) )
        {
            const JSON& cancel = host.at( "cancel" );
            Keys( cancel, { "op", "call" }, "expected cancel" );

            if( !cancel.at( "op" ).is_string() )
                Fail( "expected cancel" );

            const std::string& op = cancel.at( "op" ).get_ref<const std::string&>();
            int64_t*           target = nullptr;

            if( op == "overflow" )
                target = &cancelOverflow;
            else if( op == "sync" )
                target = &cancelSync;
            else if( op == "num_put" )
                target = &put.cancel;
            else if( op == "tie_sync" )
                target = &cancelTieSync;
            else
                Fail( "expected cancel" );

            const int64_t maximum = op == "overflow" ? static_cast<int64_t>( MAX_SINK_CAPACITY ) : MAX_SCRIPT_CALL;

            if( !Integer( cancel.at( "call" ), 0, maximum, *target ) )
                Fail( "expected cancel" );

            cancelled = true;
        }
    }

    if( unwinding && cancelled )
        Fail( "unwinding excludes cancel" );

    if( !bounded && width > MAX_UNBOUNDED_WIDTH )
        Fail( "unbounded width exceeds limit" );

    const SHARED_RECORD record;

    return Isolated(
            [&]() -> JSON
            {
                g_abortRecord = &*record;
                EVENTS events;
                SINK   sink( bounded ? static_cast<size_t>( capacity ) : SIZE_MAX, syncFails, overflowThrow,
                             syncThrow, cancelOverflow, cancelSync, events, "sync" );
                events.main = &sink;

                // The tied stream: its flags, clear( state ), then exceptions( mask ), whose
                // throw leaves the mask set and is caught.
                SINK         tieSink( SIZE_MAX, tieSyncFails, -1, tieSyncThrow, -1, cancelTieSync, events,
                                      "tie_sync" );
                SHARED_STREAM tiedHolder( record->tie, tied && tieNullBuffer ? nullptr : &tieSink );
                std::ostream& tiedStream = tiedHolder.stream;
                bool          tieSetupFailure = false;

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

                SHARED_STREAM outHolder( record->main, nullBuffer ? nullptr : &sink );
                std::ostream& out = outHolder.stream;
                std::locale   locale( std::locale::classic(), new NUMPUNCT( separator, grouping ) );

                if( put.present )
                    locale = std::locale( locale, new NUM_PUT( put, events ) );

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

                bool outerPreserved = false;

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

                    if( unwinding )
                        reply["outer_preserved"] = outerPreserved;

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

                // Taken in the host operation that cancels the thread, before the forced
                // unwind: the reply an abort would give, whose stream fields the oracle
                // process then replaces with the stream images (ImageFields).
                std::function<void()> onCancel = [&]()
                {
                    const std::string text = report( "abort", nullptr, true ).dump();

                    if( text.size() > MAX_SNAPSHOT )
                    {
                        record->oversized = true;
                    }
                    else
                    {
                        std::memcpy( record->snapshot, text.data(), text.size() );
                        record->length = text.size();
                    }

                    record->cancelled = true;
                };
                g_cancelSnapshot = &onCancel;
                const VALUE value( x, y );
                const char* outcome = nullptr;
                std::string what;
                bool        same = true;

                // A forced unwind is caught by none of these: it ends the insertion's thread.
                std::function<void()> call = [&]()
                {
                    try
                    {
                        std::ostream& ( *insert )( std::ostream&, const VALUE& ) = &operator<<;
                        same = &insert( out, value ) == &out;
                        outcome = "returned";
                    }
                    catch( const std::ios_base::failure& )
                    {
                        outcome = "ios_failure";
                    }
                    catch( const std::runtime_error& error )
                    {
                        // Copied: what() of the propagated object itself.
                        what = error.what();
                        outcome = "runtime_error";
                    }
                    catch( const std::bad_alloc& )
                    {
                        outcome = "bad_alloc";
                    }
                };

                if( unwinding )
                {
                    try
                    {
                        DURING_UNWINDING guard{ call };
                        throw std::runtime_error( "outer" );
                    }
                    catch( const std::runtime_error& error )
                    {
                        outerPreserved = std::string_view( error.what() ) == "outer";
                    }
                }
                else if( cancelled )
                {
                    // Its own thread, so that only the insertion is cancelled; the streams
                    // stay on this one.
                    pthread_t thread;

                    if( pthread_create( &thread, nullptr, RunInsertion, &call ) != 0 )
                        throw std::runtime_error( "insertion thread failed" );

                    void* status = nullptr;

                    if( pthread_join( thread, &status ) != 0 )
                        throw std::runtime_error( "insertion join failed" );

                    if( status == PTHREAD_CANCELED )
                        outcome = "forced_unwind";
                }
                else
                {
                    call();
                }

                if( !outcome )
                    throw std::runtime_error( "insertion ended without an outcome" );

                return report( outcome, std::string_view( outcome ) == "runtime_error" ? what.c_str() : nullptr,
                               same );
            },
            // In this process, after the child has ended with an abort.
            [&]() -> JSON
            {
                if( !record->cancelled || record->stale || record->oversized )
                    throw std::runtime_error( "insertion aborted without a valid cancellation snapshot" );

                JSON reply = JSON::parse( std::string_view( record->snapshot, record->length ) );
                ImageFields( reply, record->main, true );

                if( reply.contains( "tie" ) )
                    ImageFields( reply["tie"], record->tie, false );

                return reply;
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
