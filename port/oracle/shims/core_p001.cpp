// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.application.pure.p001. It calls the pinned
// libs/core base64.cpp, observable.cpp, profile.cpp and version_compare.cpp compiled
// into the reference `core` archive. Only validation, conversion, the scripted
// observable driver and serialization live here; no reference algorithm is copied.
//
// GetRunningMicroSecs reads CLOCK_MONOTONIC. The executables link with
// -Wl,--wrap=clock_gettime, so the static reference objects' clock_gettime calls
// reach __wrap_clock_gettime below. While a case has armed the script it records the
// clock id and call count and returns the case's timespec; otherwise it forwards to
// the real function. Shared libraries (libstdc++, wx) are not affected by --wrap.
#include "registry.h"
#include <core/base64.h>
#include <core/observable.h>
#include <core/profile.h>
#include <core/version_compare.h>
#include <algorithm>
#include <chrono>
#include <climits>
#include <cstdint>
#include <cstring>
#include <ctime>
#include <initializer_list>
#include <memory>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

static_assert( sizeof( int ) == 4 && sizeof( size_t ) == 8 && sizeof( time_t ) == 8
                       && sizeof( long ) == 8 && CHAR_BIT == 8,
               "core oracle cases require the pinned LP64 Linux profile" );
static_assert( std::is_same_v<LATENCY_PROBE::CLOCK::duration, std::chrono::nanoseconds>,
               "latency probe cases encode time points as int64 nanoseconds" );

extern "C" int __real_clock_gettime( clockid_t aId, struct timespec* aTime );

namespace
{
/// The scripted CLOCK source. Native harness state only; never portable code.
struct CLOCK_SCRIPT
{
    bool      armed = false;
    timespec  value{};
    int       calls = 0;
    clockid_t id = -1;
};

CLOCK_SCRIPT g_clock;
} // namespace

extern "C" int __wrap_clock_gettime( clockid_t aId, struct timespec* aTime )
{
    if( !g_clock.armed )
        return __real_clock_gettime( aId, aTime );

    g_clock.calls++;
    g_clock.id = aId;
    *aTime = g_clock.value;
    return 0;
}

namespace
{
using oracle::JSON;

constexpr size_t MAX_BYTES = size_t( 1 ) << 20;
constexpr size_t MAX_HEX_BYTES = size_t( 1 ) << 16;
constexpr size_t MAX_TEXT = size_t( 1 ) << 16;
constexpr size_t MAX_CHECKPOINTS = 256;

constexpr int    OBSERVABLES = 8;
constexpr int    LINKS = 16;
constexpr int    OBSERVERS = 8;
constexpr size_t MAX_STEPS = 2048;
constexpr int    MAX_DEPTH = 8;
constexpr size_t BUDGET = 100000;

[[noreturn]] void Fail( const std::string& aMessage )
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


std::string Text( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        Fail( std::string( "expected string: " ) + aName );

    return aInput.at( aName ).get<std::string>();
}


std::string BoundedText( const JSON& aInput, const char* aName )
{
    std::string value = Text( aInput, aName );

    if( value.size() > MAX_TEXT )
        Fail( std::string( "string too long: " ) + aName );

    return value;
}


int64_t Signed( const JSON& aInput, const char* aName, int64_t aMin, int64_t aMax )
{
    const auto fail = [&]() { Fail( std::string( "integer out of range: " ) + aName ); };

    if( !aInput.contains( aName ) )
        fail();

    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() )
        fail();

    if( value.is_number_unsigned() )
    {
        if( aMax < 0 || value.get<uint64_t>() > uint64_t( aMax ) )
            fail();
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        fail();

    return number;
}


uint64_t Unsigned( const JSON& aInput, const char* aName )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() || ( !value.is_number_unsigned() && value.get<int64_t>() < 0 ) )
        Fail( std::string( "integer out of range: " ) + aName );

    return value.get<uint64_t>();
}


bool Boolean( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_boolean() )
        Fail( std::string( "expected boolean: " ) + aName );

    return aInput.at( aName ).get<bool>();
}


int Nibble( char aChar )
{
    if( aChar >= '0' && aChar <= '9' )
        return aChar - '0';

    if( aChar >= 'a' && aChar <= 'f' )
        return aChar - 'a' + 10;

    return -1;
}


std::vector<uint8_t> Hex( const JSON& aInput, const char* aName )
{
    const std::string invalid = std::string( "invalid hex: " ) + aName;

    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        Fail( invalid );

    const std::string digits = aInput.at( aName ).get<std::string>();

    if( digits.size() % 2 != 0 )
        Fail( invalid );

    if( digits.size() / 2 > MAX_HEX_BYTES )
        Fail( std::string( "byte array too large: " ) + aName );

    std::vector<uint8_t> bytes;
    bytes.reserve( digits.size() / 2 );

    for( size_t i = 0; i < digits.size(); i += 2 )
    {
        const int high = Nibble( digits[i] );
        const int low = Nibble( digits[i + 1] );

        if( high < 0 || low < 0 )
            Fail( invalid );

        bytes.push_back( static_cast<uint8_t>( ( high << 4 ) | low ) );
    }

    return bytes;
}


std::string ToHex( const std::vector<uint8_t>& aBytes )
{
    static const char DIGITS[] = "0123456789abcdef";
    std::string       out;
    out.reserve( aBytes.size() * 2 );

    for( uint8_t byte : aBytes )
    {
        out.push_back( DIGITS[byte >> 4] );
        out.push_back( DIGITS[byte & 15] );
    }

    return out;
}


double Bits( const JSON& aInput, const char* aName )
{
    const std::string invalid = std::string( "invalid bits: " ) + aName;

    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        Fail( invalid );

    const std::string digits = aInput.at( aName ).get<std::string>();

    if( digits.size() != 16 )
        Fail( invalid );

    uint64_t bits = 0;

    for( char c : digits )
    {
        const int nibble = Nibble( c );

        if( nibble < 0 )
            Fail( invalid );

        bits = ( bits << 4 ) | uint64_t( nibble );
    }

    double value;
    std::memcpy( &value, &bits, sizeof( value ) );
    return value;
}


JSON Base64( const JSON& aInput, bool aEncode )
{
    Fields( aInput, { "op", "input" }, { "output", "repeat" } );
    const std::vector<uint8_t> data = Hex( aInput, "input" );
    std::vector<uint8_t>       output;

    if( aInput.contains( "output" ) )
        output = Hex( aInput, "output" );

    const int64_t repeat = aInput.contains( "repeat" ) ? Signed( aInput, "repeat", 1, 65536 ) : 1;

    if( data.size() * size_t( repeat ) > MAX_BYTES )
        Fail( "input too large" );

    std::vector<uint8_t> input;
    input.reserve( data.size() * size_t( repeat ) );

    for( int64_t i = 0; i < repeat; ++i )
        input.insert( input.end(), data.begin(), data.end() );

    if( aEncode )
        base64::encode( input, output );
    else
        base64::decode( input, output );

    return { { "output", ToHex( output ) } };
}


/// The terminating domain of parseVersionString. A start position whose from_chars
/// matches nothing and that is not '.' never advances (the reference loops until
/// allocation fails). from_chars consumes an optional leading '-' and digits, so
/// every '-' is a start position: it must be followed by a digit, and every other
/// byte must be a digit or '.'.
void RequireTerminatingVersion( const std::string& aText )
{
    for( size_t i = 0; i < aText.size(); ++i )
    {
        const char c = aText[i];
        const bool digit = c >= '0' && c <= '9';
        const bool dash = c == '-' && i + 1 < aText.size() && aText[i + 1] >= '0'
                          && aText[i + 1] <= '9';

        if( !digit && !dash && c != '.' )
            Fail( "version string does not terminate" );
    }
}


JSON LatencyProbe( const JSON& aInput );
JSON Observable( const JSON& aInput );


JSON Run( const JSON& aInput )
{
    const std::string op = Text( aInput, "op" );

    if( op == "encode_length" || op == "decode_length" )
    {
        Fields( aInput, { "op", "len" } );
        const uint64_t len = Unsigned( aInput, "len" );
        return { { "value", op == "encode_length" ? base64::encode_length( len )
                                                  : base64::decode_length( len ) } };
    }

    if( op == "encode" )
        return Base64( aInput, true );

    if( op == "decode" )
        return Base64( aInput, false );

    if( op == "parse_version" )
    {
        Fields( aInput, { "op", "text" } );
        const std::string text = BoundedText( aInput, "text" );
        RequireTerminatingVersion( text );
        const std::vector<int> components = parseVersionString( text );
        return { { "components", components } };
    }

    if( op == "compare_versions" )
    {
        Fields( aInput, { "op", "a", "b" } );
        const std::string a = BoundedText( aInput, "a" );
        const std::string b = BoundedText( aInput, "b" );
        RequireTerminatingVersion( a );
        RequireTerminatingVersion( b );
        return { { "value", compareVersionStrings( a, b ) } };
    }

    if( op == "running_micro_secs" )
    {
        Fields( aInput, { "op", "tv_sec", "tv_nsec" } );
        const int64_t sec = Signed( aInput, "tv_sec", INT64_MIN, INT64_MAX );
        const int64_t nsec = Signed( aInput, "tv_nsec", INT64_MIN, INT64_MAX );

        if( nsec < 0 || nsec >= 1000000000 )
            Fail( "invalid timespec: tv_nsec" );

        // Keep the reference's signed int64 arithmetic defined.
        int64_t usecs = 0;

        if( __builtin_mul_overflow( sec, int64_t( 1000000 ), &usecs )
            || __builtin_add_overflow( usecs, nsec / 1000, &usecs ) )
        {
            Fail( "signed overflow" );
        }

        g_clock = CLOCK_SCRIPT{ true, timespec{ sec, nsec }, 0, -1 };
        const int64_t value = GetRunningMicroSecs();
        const CLOCK_SCRIPT seen = g_clock;
        g_clock = CLOCK_SCRIPT{};
        return { { "value", value }, { "clock_calls", seen.calls }, { "clock_id", seen.id } };
    }

    if( op == "latency_probe" )
        return LatencyProbe( aInput );

    if( op == "observable" )
        return Observable( aInput );

    Fail( "unknown operation" );
}


// ---- LATENCY_PROBE private state -------------------------------------------------------

template <typename Tag, typename Tag::Type Member>
struct ACCESS
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};

struct PROBE_START
{
    using Type = LATENCY_PROBE::TIME_POINT LATENCY_PROBE::*;
    friend Type Get( PROBE_START );
};

struct PROBE_CHECKPOINTS
{
    using Type = std::vector<LATENCY_PROBE::CHECKPOINT> LATENCY_PROBE::*;
    friend Type Get( PROBE_CHECKPOINTS );
};

template struct ACCESS<PROBE_START, &LATENCY_PROBE::m_start>;
template struct ACCESS<PROBE_CHECKPOINTS, &LATENCY_PROBE::m_checkpoints>;


JSON LatencyProbe( const JSON& aInput )
{
    Fields( aInput, { "op", "name", "start", "checkpoints" } );
    const std::string name = BoundedText( aInput, "name" );
    const int64_t     start = Signed( aInput, "start", INT64_MIN, INT64_MAX );

    if( !aInput.at( "checkpoints" ).is_array() )
        Fail( "expected array: checkpoints" );

    const JSON& list = aInput.at( "checkpoints" );

    if( list.size() > MAX_CHECKPOINTS )
        Fail( "too many checkpoints" );

    using TIME_POINT = LATENCY_PROBE::TIME_POINT;
    std::vector<LATENCY_PROBE::CHECKPOINT> checkpoints;

    for( const JSON& item : list )
    {
        Fields( item, { "name", "timestamp", "timer", "delta" } );
        LATENCY_PROBE::CHECKPOINT checkpoint;
        checkpoint.name = BoundedText( item, "name" );
        checkpoint.timestamp =
                TIME_POINT( std::chrono::nanoseconds( Signed( item, "timestamp", INT64_MIN, INT64_MAX ) ) );
        checkpoint.isTimer = Boolean( item, "timer" );
        checkpoint.timerDelta = Bits( item, "delta" );
        checkpoints.push_back( checkpoint );
    }

    // Keep delta_ms's time point subtraction (signed int64 nanoseconds) defined.
    int64_t prev = start;

    for( const LATENCY_PROBE::CHECKPOINT& checkpoint : checkpoints )
    {
        if( checkpoint.isTimer )
            continue;

        const int64_t stamp = checkpoint.timestamp.time_since_epoch().count();
        int64_t       difference = 0;

        if( __builtin_sub_overflow( stamp, prev, &difference )
            || __builtin_sub_overflow( stamp, start, &difference ) )
        {
            Fail( "time point difference overflows" );
        }

        prev = stamp;
    }

    // The upstream constructor (default aStages); state set through the private members.
    LATENCY_PROBE probe( name );
    probe.*Get( PROBE_START{} ) = TIME_POINT( std::chrono::nanoseconds( start ) );
    probe.*Get( PROBE_CHECKPOINTS{} ) = checkpoints;
    return { { "text", probe.to_string() } };
}


// ---- Observable driver ------------------------------------------------------------------

class DRIVER;

struct OBSERVER
{
    DRIVER* driver = nullptr;
    int     id = 0;

    void OnEvent( int aEvent );
};


/// The real OBSERVABLE<T> header template, with its protected base members exposed.
class TEST_OBSERVABLE : public UTIL::OBSERVABLE<OBSERVER>
{
public:
    TEST_OBSERVABLE() = default;
    TEST_OBSERVABLE( TEST_OBSERVABLE& aOther ) : UTIL::OBSERVABLE<OBSERVER>( aOther ) {}

    using IMPL_PTR = decltype( impl_ );

    const IMPL_PTR& Impl() const { return impl_; }
    void            Enter() { enter_iteration(); }
    void            Leave() { leave_iteration(); }
    void            Empty() { on_observers_empty(); }
};


struct LINK_TOKEN
{
    using Type = TEST_OBSERVABLE::IMPL_PTR UTIL::LINK::*;
    friend Type Get( LINK_TOKEN );
};

struct LINK_OBSERVER
{
    using Type = void* UTIL::LINK::*;
    friend Type Get( LINK_OBSERVER );
};

template struct ACCESS<LINK_TOKEN, &UTIL::LINK::token_>;
template struct ACCESS<LINK_OBSERVER, &UTIL::LINK::observer_>;


struct STEP
{
    std::string                                op;
    int                                        obs = -1;
    int                                        from = -1;
    int                                        link = -1;
    int                                        observer = -1; // -1 is nullptr
    int                                        ignore = -1;   // -1 is nullptr
    bool                                       guard = false;
    int                                        event = 0;
    std::vector<std::pair<int, std::vector<STEP>>> reactions;
};


class PARSER
{
public:
    std::vector<STEP> Steps( const JSON& aOwner, const char* aKey, int aDepth )
    {
        if( !aOwner.contains( aKey ) || !aOwner.at( aKey ).is_array() )
            Fail( "expected array: steps" );

        std::vector<STEP> steps;

        for( const JSON& step : aOwner.at( aKey ) )
            steps.push_back( Step( step, aDepth ) );

        return steps;
    }

private:
    static int Index( const JSON& aInput, const char* aName, int aCount )
    {
        return int( Signed( aInput, aName, 0, aCount - 1 ) );
    }

    static int Pointer( const JSON& aInput, const char* aName )
    {
        if( aInput.at( aName ).is_null() )
            return -1;

        return int( Signed( aInput, aName, 0, OBSERVERS - 1 ) );
    }

    static bool Guard( const JSON& aInput )
    {
        return aInput.contains( "guard" ) ? Boolean( aInput, "guard" ) : false;
    }

    STEP Step( const JSON& aInput, int aDepth )
    {
        if( ++m_count > MAX_STEPS )
            Fail( "too many steps" );

        if( aDepth > MAX_DEPTH )
            Fail( "nesting too deep" );

        STEP step;
        step.op = Text( aInput, "op" );
        const std::string& op = step.op;

        if( op == "new" )
        {
            Fields( aInput, { "op", "obs" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );
        }
        else if( op == "copy" )
        {
            Fields( aInput, { "op", "obs", "from" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );
            step.from = Index( aInput, "from", OBSERVABLES );
        }
        else if( op == "drop" )
        {
            Fields( aInput, { "op", "obs" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );

            if( std::find( m_notifying.begin(), m_notifying.end(), step.obs ) != m_notifying.end() )
                Fail( "drop during own notification" );
        }
        else if( op == "subscribe" )
        {
            Fields( aInput, { "op", "obs", "observer", "link" }, { "guard" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );
            step.observer = Pointer( aInput, "observer" );
            step.link = Index( aInput, "link", LINKS );
            step.guard = Guard( aInput );
        }
        else if( op == "subscribe_unmanaged" || op == "unsubscribe" )
        {
            Fields( aInput, { "op", "obs", "observer" }, { "guard" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );
            step.observer = Pointer( aInput, "observer" );
            step.guard = Guard( aInput );
        }
        else if( op == "notify" || op == "notify_ignore" )
        {
            const bool ignoreVariant = op == "notify_ignore";

            if( ignoreVariant )
                Fields( aInput, { "op", "obs", "event", "ignore", "reactions" } );
            else
                Fields( aInput, { "op", "obs", "event", "reactions" } );

            step.obs = Index( aInput, "obs", OBSERVABLES );
            step.event = int( Signed( aInput, "event", -1000000, 1000000 ) );

            if( ignoreVariant )
                step.ignore = Pointer( aInput, "ignore" );

            if( !aInput.at( "reactions" ).is_array() )
                Fail( "expected array: reactions" );

            m_notifying.push_back( step.obs );

            for( const JSON& reaction : aInput.at( "reactions" ) )
            {
                Fields( reaction, { "observer", "steps" } );
                const int observer = int( Signed( reaction, "observer", 0, OBSERVERS - 1 ) );

                for( const auto& [existing, steps] : step.reactions )
                {
                    if( existing == observer )
                        Fail( "duplicate reaction" );
                }

                std::vector<STEP> steps = Steps( reaction, "steps", aDepth + 1 );
                step.reactions.emplace_back( observer, std::move( steps ) );
            }

            m_notifying.pop_back();
        }
        else if( op == "enter_iteration" || op == "leave_iteration" || op == "on_observers_empty" )
        {
            Fields( aInput, { "op", "obs" } );
            step.obs = Index( aInput, "obs", OBSERVABLES );

            if( aDepth > 0 )
                Fail( "base operation inside notification" );
        }
        else if( op == "link_new" || op == "link_reset" || op == "link_drop" )
        {
            Fields( aInput, { "op", "link" } );
            step.link = Index( aInput, "link", LINKS );
        }
        else if( op == "link_token" )
        {
            Fields( aInput, { "op", "link", "obs", "observer" } );
            step.link = Index( aInput, "link", LINKS );
            step.obs = Index( aInput, "obs", OBSERVABLES );
            step.observer = Pointer( aInput, "observer" );
        }
        else if( op == "link_move" || op == "link_assign" )
        {
            Fields( aInput, { "op", "link", "from" } );
            step.link = Index( aInput, "link", LINKS );
            step.from = Index( aInput, "from", LINKS );
        }
        else
        {
            Fail( "unknown operation" );
        }

        return step;
    }

    size_t           m_count = 0;
    std::vector<int> m_notifying;
};


class DRIVER
{
public:
    DRIVER()
    {
        for( int i = 0; i < OBSERVERS; ++i )
            m_observers[i] = OBSERVER{ this, i };
    }

    JSON Run( const std::vector<STEP>& aSteps )
    {
        JSON events = JSON::array();

        for( const STEP& step : aSteps )
        {
            Exec( step, 0 );
            events.push_back( { { "calls", std::exchange( m_calls, JSON::array() ) },
                                { "state", Snapshot() } } );
        }

        return { { "events", std::move( events ) } };
    }

    void Called( int aObserver, int aEvent )
    {
        Tick();
        m_calls.push_back( { { "depth", m_frames.back().depth },
                             { "event", aEvent },
                             { "observer", aObserver } } );

        const FRAME frame = m_frames.back();

        for( const auto& [observer, steps] : *frame.reactions )
        {
            if( observer == aObserver )
            {
                for( const STEP& step : steps )
                    Exec( step, frame.depth + 1 );
            }
        }
    }

private:
    struct FRAME
    {
        const std::vector<std::pair<int, std::vector<STEP>>>* reactions;
        int                                                    depth;
    };

    void Tick()
    {
        if( ++m_budget > BUDGET )
            Fail( "execution budget exceeded" );
    }

    OBSERVER* Ptr( int aObserver ) { return aObserver < 0 ? nullptr : &m_observers[aObserver]; }

    TEST_OBSERVABLE& Observable( int aObs )
    {
        if( !m_observables[aObs] )
            Fail( "observable slot empty" );

        return *m_observables[aObs];
    }

    void Vacant( int aObs )
    {
        if( m_observables[aObs] )
            Fail( "observable slot occupied" );
    }

    void LinkVacant( int aLink )
    {
        if( m_links[aLink] )
            Fail( "link slot occupied" );
    }

    UTIL::LINK& Link( int aLink )
    {
        if( !m_links[aLink] )
            Fail( "link slot empty" );

        return *m_links[aLink];
    }

    /// Adding to a list that an active Notify range-for is reading is undefined.
    bool AddsDuringIteration( TEST_OBSERVABLE& aObservable )
    {
        const void* impl = aObservable.Impl().get();
        return impl && std::find( m_active.begin(), m_active.end(), impl ) != m_active.end();
    }

    void Skipped( const char* aOp ) { m_calls.push_back( { { "skipped", aOp } } ); }

    void Exec( const STEP& aStep, int aDepth )
    {
        Tick();
        const std::string& op = aStep.op;

        if( op == "new" )
        {
            Vacant( aStep.obs );
            m_observables[aStep.obs] = std::make_unique<TEST_OBSERVABLE>();
        }
        else if( op == "copy" )
        {
            Vacant( aStep.obs );
            TEST_OBSERVABLE& source = Observable( aStep.from );
            m_observables[aStep.obs] = std::make_unique<TEST_OBSERVABLE>( source );
        }
        else if( op == "drop" )
        {
            Observable( aStep.obs );
            m_observables[aStep.obs].reset();
        }
        else if( op == "subscribe" || op == "subscribe_unmanaged" )
        {
            TEST_OBSERVABLE& observable = Observable( aStep.obs );

            if( op == "subscribe" )
                LinkVacant( aStep.link );

            if( AddsDuringIteration( observable ) )
            {
                if( aStep.guard )
                    return Skipped( op.c_str() );

                Fail( "subscribe during active iteration" );
            }

            if( op == "subscribe" )
                m_links[aStep.link].emplace( observable.Subscribe( Ptr( aStep.observer ) ) );
            else
                observable.SubscribeUnmanaged( Ptr( aStep.observer ) );
        }
        else if( op == "unsubscribe" )
        {
            TEST_OBSERVABLE& observable = Observable( aStep.obs );

            // OBSERVABLE_BASE::remove_observer dereferences a null impl_.
            if( !observable.Impl() )
            {
                if( aStep.guard )
                    return Skipped( "unsubscribe" );

                Fail( "unsubscribe without impl" );
            }

            observable.Unsubscribe( Ptr( aStep.observer ) );
        }
        else if( op == "notify" || op == "notify_ignore" )
        {
            TEST_OBSERVABLE& observable = Observable( aStep.obs );
            const void*      impl = observable.Impl().get();

            if( impl )
                m_active.push_back( impl );

            m_frames.push_back( FRAME{ &aStep.reactions, aDepth } );

            if( op == "notify" )
                observable.Notify( &OBSERVER::OnEvent, aStep.event );
            else
                observable.NotifyIgnore( &OBSERVER::OnEvent, Ptr( aStep.ignore ), aStep.event );

            m_frames.pop_back();

            if( impl )
                m_active.pop_back();
        }
        else if( op == "enter_iteration" )
        {
            Observable( aStep.obs ).Enter();
        }
        else if( op == "leave_iteration" )
        {
            Observable( aStep.obs ).Leave();
        }
        else if( op == "on_observers_empty" )
        {
            Observable( aStep.obs ).Empty();
        }
        else if( op == "link_new" )
        {
            LinkVacant( aStep.link );
            m_links[aStep.link].emplace();
        }
        else if( op == "link_token" )
        {
            LinkVacant( aStep.link );
            TEST_OBSERVABLE& observable = Observable( aStep.obs );
            m_links[aStep.link].emplace( observable.Impl(), Ptr( aStep.observer ) );
        }
        else if( op == "link_move" )
        {
            LinkVacant( aStep.link );
            UTIL::LINK& source = Link( aStep.from );
            m_links[aStep.link].emplace( std::move( source ) );
        }
        else if( op == "link_assign" )
        {
            UTIL::LINK& target = Link( aStep.link );
            UTIL::LINK& source = Link( aStep.from );
            target = std::move( source );
        }
        else if( op == "link_reset" )
        {
            Link( aStep.link ).reset();
        }
        else if( op == "link_drop" )
        {
            Link( aStep.link );
            m_links[aStep.link].reset();
        }
    }

    JSON ObserverJson( const void* aPointer )
    {
        if( !aPointer )
            return nullptr;

        for( int i = 0; i < OBSERVERS; ++i )
        {
            if( aPointer == &m_observers[i] )
                return i;
        }

        throw std::logic_error( "unknown observer pointer" );
    }

    JSON Snapshot()
    {
        std::vector<const TEST_OBSERVABLE::IMPL_PTR*> owners;
        std::vector<const void*>                      labels;

        auto label = [&]( const TEST_OBSERVABLE::IMPL_PTR& aPtr ) -> JSON
        {
            if( !aPtr )
                return nullptr;

            auto it = std::find( labels.begin(), labels.end(), aPtr.get() );

            if( it != labels.end() )
                return size_t( it - labels.begin() );

            labels.push_back( aPtr.get() );
            owners.push_back( &aPtr );
            return labels.size() - 1;
        };

        JSON observables = JSON::array();

        for( const auto& observable : m_observables )
        {
            if( !observable )
                observables.push_back( nullptr );
            else
                observables.push_back( { { "impl", label( observable->Impl() ) },
                                         { "size", observable->size() } } );
        }

        JSON links = JSON::array();

        for( const auto& link : m_links )
        {
            if( !link )
            {
                links.push_back( nullptr );
                continue;
            }

            const TEST_OBSERVABLE::IMPL_PTR& token = ( *link ).*Get( LINK_TOKEN{} );
            links.push_back( { { "impl", label( token ) },
                               { "linked", static_cast<bool>( *link ) },
                               { "observer", ObserverJson( ( *link ).*Get( LINK_OBSERVER{} ) ) } } );
        }

        JSON impls = JSON::array();

        for( const TEST_OBSERVABLE::IMPL_PTR* owner : owners )
        {
            const auto& impl = **owner;
            JSON        observers = JSON::array();

            for( const void* observer : impl.observers_ )
                observers.push_back( ObserverJson( observer ) );

            impls.push_back( { { "iteration_count", impl.iteration_count_ },
                               { "observers", std::move( observers ) },
                               { "shared", impl.is_shared() },
                               { "use_count", owner->use_count() } } );
        }

        return { { "impls", std::move( impls ) },
                 { "links", std::move( links ) },
                 { "observables", std::move( observables ) } };
    }

    OBSERVER                                       m_observers[OBSERVERS];
    std::unique_ptr<TEST_OBSERVABLE>               m_observables[OBSERVABLES];
    std::optional<UTIL::LINK>                      m_links[LINKS];
    std::vector<const void*>                       m_active;
    std::vector<FRAME>                             m_frames;
    JSON                                           m_calls = JSON::array();
    size_t                                         m_budget = 0;
};


void OBSERVER::OnEvent( int aEvent )
{
    driver->Called( id, aEvent );
}


JSON Observable( const JSON& aInput )
{
    Fields( aInput, { "op", "steps" } );
    PARSER            parser;
    std::vector<STEP> steps = parser.Steps( aInput, "steps", 0 );
    // Destruction order at scope exit: links, then observables, then observers.
    DRIVER driver;
    return driver.Run( steps );
}
} // namespace


namespace oracle
{
void RegisterCoreP001( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.core.application.pure.p001",
                   "libs/core/{base64,observable,profile,version_compare}.cpp", Run );
}
} // namespace oracle
