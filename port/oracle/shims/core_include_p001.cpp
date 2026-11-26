// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for unit libs.core.include.core.application.pure.p001: the pinned
// libs/core/include/core header bodies (profile.h, union_find.h, filter_kruskal.h,
// minoptmax.h, spinlock.h, sync_queue.h, observable.h, arraydim.h, ignore.h,
// type_helpers.h), compiled here from the unmodified pinned headers. Only validation,
// conversion, scripting and serialization live here; no reference algorithm is copied.
//
// PROF_TIMER and LATENCY_PROBE read std::chrono::high_resolution_clock::now(), which
// libstdc++ defines as system_clock::now() in the runtime library. The executables
// link with -Wl,--wrap=_ZNSt6chrono3_V212system_clock3nowEv, so calls from these
// static objects reach the wrapper below. While a case has armed the script, each call
// returns the next scripted time point and is counted; otherwise it forwards to the
// real function.
#include "registry.h"
#include <core/arraydim.h>
#include <core/filter_kruskal.h>
#include <core/ignore.h>
#include <core/minoptmax.h>
#include <core/profile.h>
#include <core/spinlock.h>
#include <core/sync_queue.h>
#include <core/type_helpers.h>
#include <core/union_find.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <climits>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <memory>
#include <optional>
#include <random>
#include <span>
#include <sstream>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

static_assert( sizeof( int ) == 4 && sizeof( size_t ) == 8 && sizeof( long long ) == 8
                       && CHAR_BIT == 8,
               "core header oracle cases require the pinned LP64 Linux profile" );
static_assert( std::is_same_v<std::chrono::high_resolution_clock, std::chrono::system_clock>
                       && std::is_same_v<std::chrono::system_clock::duration,
                                         std::chrono::nanoseconds>,
               "profile cases script high_resolution_clock as int64 nanoseconds" );

using SYSTEM_TIME = std::chrono::system_clock::time_point;

extern "C" SYSTEM_TIME __real__ZNSt6chrono3_V212system_clock3nowEv() noexcept;

namespace
{
/// The scripted clock. Native harness state only; never portable code.
struct CHRONO_SCRIPT
{
    bool                 armed = false;
    std::vector<int64_t> values;
    size_t               calls = 0;
    bool                 exhausted = false;
};

CHRONO_SCRIPT g_chrono;
} // namespace

extern "C" SYSTEM_TIME __wrap__ZNSt6chrono3_V212system_clock3nowEv() noexcept
{
    if( !g_chrono.armed )
        return __real__ZNSt6chrono3_V212system_clock3nowEv();

    // now() is noexcept: an exhausted script is reported after the step instead.
    if( g_chrono.calls >= g_chrono.values.size() )
    {
        g_chrono.exhausted = true;
        return SYSTEM_TIME();
    }

    return SYSTEM_TIME( std::chrono::nanoseconds( g_chrono.values[g_chrono.calls++] ) );
}

namespace oracle
{
JSON RunCoreObservable( const JSON& aInput );
}

namespace
{
using oracle::JSON;

constexpr size_t  MAX_TEXT = size_t( 1 ) << 16;
constexpr size_t  MAX_STEPS = 4096;
constexpr size_t  MAX_CLOCK = 4096;
constexpr int64_t CLOCK_LIMIT = ( int64_t( 1 ) << 62 ) - 1;
constexpr int     TIMERS = 4;
constexpr int     PROBES = 2;
constexpr int     COUNTERS = 2;
constexpr int     MAX_STAGES = 1 << 16;
constexpr size_t  MAX_SET = 8192;
constexpr size_t  MAX_UF_STEPS = 100000;
constexpr size_t  MAX_EDGES = 65536;
constexpr size_t  MAX_LIST = 64;

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

    std::string value = aInput.at( aName ).get<std::string>();

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
    if( !aInput.contains( aName ) )
        Fail( std::string( "integer out of range: " ) + aName );

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


const JSON& Array( const JSON& aInput, const char* aName, size_t aMax )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_array() )
        Fail( std::string( "expected array: " ) + aName );

    if( aInput.at( aName ).size() > aMax )
        Fail( std::string( "array too long: " ) + aName );

    return aInput.at( aName );
}


std::string Bits( double aValue )
{
    uint64_t bits;
    std::memcpy( &bits, &aValue, sizeof( bits ) );

    static const char DIGITS[] = "0123456789abcdef";
    std::string       out( 16, '0' );

    for( int i = 15; i >= 0; --i )
    {
        out[i] = DIGITS[bits & 15];
        bits >>= 4;
    }

    return out;
}


JSON LengthError( const std::length_error& aError )
{
    return { { "exception", "length_error" }, { "what", aError.what() } };
}


// ---- private members -----------------------------------------------------------------

template <typename Tag, typename Tag::Type Member>
struct ACCESS
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};

struct UF_PARENT
{
    using Type = std::vector<std::atomic<uint32_t>> KI_UNION_FIND::*;
    friend Type Get( UF_PARENT );
};

struct COUNTER_COUNT
{
    using Type = std::atomic_ullong PROF_COUNTER::*;
    friend Type Get( COUNTER_COUNT );
};

template struct ACCESS<UF_PARENT, &KI_UNION_FIND::m_parent>;
template struct ACCESS<COUNTER_COUNT, &PROF_COUNTER::m_count>;


JSON Parents( const KI_UNION_FIND& aForest )
{
    JSON parents = JSON::array();

    for( const std::atomic<uint32_t>& parent : aForest.*Get( UF_PARENT{} ) )
        parents.push_back( parent.load() );

    return parents;
}


// ---- MINOPTMAX ------------------------------------------------------------------------

template <typename T>
JSON MinOptMaxState( const MINOPTMAX<T>& aValue, const std::vector<T>& aProbes,
                     const std::vector<T>& aDefaults )
{
    JSON matches = JSON::array();
    JSON pinned = JSON::array();

    for( T probe : aProbes )
        matches.push_back( aValue.Matches( probe ) );

    for( T fallback : aDefaults )
        pinned.push_back( aValue.PinnedOpt( fallback ) );

    return { { "min", aValue.Min() },
             { "max", aValue.Max() },
             { "opt", aValue.Opt() },
             { "pinned_opt", aValue.PinnedOpt() },
             { "has_min", aValue.HasMin() },
             { "has_max", aValue.HasMax() },
             { "has_opt", aValue.HasOpt() },
             { "is_null", aValue.IsNull() },
             { "matches", std::move( matches ) },
             { "pinned_opt_or", std::move( pinned ) } };
}


template <typename T>
JSON RunMinOptMax( const JSON& aInput )
{
    const int64_t low = std::numeric_limits<T>::min();
    const int64_t high = std::numeric_limits<T>::max();
    std::vector<T> probes;
    std::vector<T> defaults;

    for( const JSON& probe : Array( aInput, "probes", MAX_LIST ) )
        probes.push_back( T( Signed( { { "value", probe } }, "value", low, high ) ) );

    for( const JSON& fallback : Array( aInput, "defaults", MAX_LIST ) )
        defaults.push_back( T( Signed( { { "value", fallback } }, "value", low, high ) ) );

    std::vector<std::pair<std::string, T>> sets;

    for( const JSON& set : Array( aInput, "sets", MAX_LIST ) )
    {
        Fields( set, { "set", "value" } );
        const std::string which = Text( set, "set" );

        if( which != "min" && which != "max" && which != "opt" )
            Fail( "unknown setter" );

        sets.emplace_back( which, T( Signed( set, "value", low, high ) ) );
    }

    MINOPTMAX<T> value;
    JSON         states = JSON::array();
    states.push_back( MinOptMaxState( value, probes, defaults ) );

    for( const auto& [which, number] : sets )
    {
        if( which == "min" )
            value.SetMin( number );
        else if( which == "max" )
            value.SetMax( number );
        else
            value.SetOpt( number );

        states.push_back( MinOptMaxState( value, probes, defaults ) );
    }

    return { { "states", std::move( states ) } };
}


JSON MinOptMax( const JSON& aInput )
{
    Fields( aInput, { "op", "type", "sets", "probes", "defaults" } );
    const std::string type = Text( aInput, "type" );

    if( type == "int" )
        return RunMinOptMax<int>( aInput );

    if( type == "long long" )
        return RunMinOptMax<long long>( aInput );

    Fail( "unknown type" );
}


// ---- KI_UNION_FIND --------------------------------------------------------------------

/// The index a call would read: undefined unless the truncated value is below Size().
void RequireIndex( const KI_UNION_FIND& aForest, uint64_t aIndex )
{
    if( static_cast<uint32_t>( aIndex ) >= aForest.Size() )
        Fail( "index out of range" );
}


/// Counts the reference would try to allocate beyond the harness bound are refused;
/// counts above 2^32 - 1 throw before allocating and are compared.
void RequireCount( uint64_t aCount )
{
    if( aCount > MAX_SET && aCount <= std::numeric_limits<uint32_t>::max() )
        Fail( "count too large" );
}


JSON UnionFind( const JSON& aInput )
{
    Fields( aInput, { "op", "count", "steps" } );
    const uint64_t count = Unsigned( aInput, "count" );
    const JSON&    steps = Array( aInput, "steps", MAX_UF_STEPS );
    RequireCount( count );

    for( const JSON& step : steps )
    {
        const std::string op = Text( step, "op" );

        if( op == "unite" || op == "connected" )
        {
            Fields( step, { "op", "a", "b" } );
            Unsigned( step, "a" );
            Unsigned( step, "b" );
        }
        else if( op == "find" || op == "find_compress" )
        {
            Fields( step, { "op", "x" } );
            Unsigned( step, "x" );
        }
        else if( op == "reset" )
        {
            Fields( step, { "op", "count" } );
            RequireCount( Unsigned( step, "count" ) );
        }
        else if( op == "size" || op == "component_count" )
        {
            Fields( step, { "op" } );
        }
        else
        {
            Fail( "unknown operation" );
        }
    }

    std::unique_ptr<KI_UNION_FIND> forest;

    try
    {
        forest = std::make_unique<KI_UNION_FIND>( count );
    }
    catch( const std::length_error& error )
    {
        return { { "constructed", LengthError( error ) } };
    }

    JSON results = JSON::array();

    for( const JSON& step : steps )
    {
        const std::string op = Text( step, "op" );

        if( op == "unite" )
        {
            RequireIndex( *forest, Unsigned( step, "a" ) );
            RequireIndex( *forest, Unsigned( step, "b" ) );
            results.push_back( forest->Unite( Unsigned( step, "a" ), Unsigned( step, "b" ) ) );
        }
        else if( op == "connected" )
        {
            RequireIndex( *forest, Unsigned( step, "a" ) );
            RequireIndex( *forest, Unsigned( step, "b" ) );
            results.push_back( forest->Connected( Unsigned( step, "a" ), Unsigned( step, "b" ) ) );
        }
        else if( op == "find" )
        {
            RequireIndex( *forest, Unsigned( step, "x" ) );
            results.push_back( forest->Find( Unsigned( step, "x" ) ) );
        }
        else if( op == "find_compress" )
        {
            RequireIndex( *forest, Unsigned( step, "x" ) );
            results.push_back( forest->FindCompress( Unsigned( step, "x" ) ) );
        }
        else if( op == "reset" )
        {
            try
            {
                forest->Reset( Unsigned( step, "count" ) );
                results.push_back( nullptr );
            }
            catch( const std::length_error& error )
            {
                results.push_back( LengthError( error ) );
            }
        }
        else if( op == "size" )
        {
            results.push_back( forest->Size() );
        }
        else
        {
            results.push_back( forest->ComponentCount() );
        }
    }

    return { { "constructed", true },
             { "results", std::move( results ) },
             { "components", forest->ComponentCount() },
             { "parents", Parents( *forest ) } };
}


// ---- KI_MST::FilterKruskal ------------------------------------------------------------

struct EDGE
{
    uint32_t u;
    uint32_t v;
    int64_t  weight;
    uint32_t id;
};


/// QA's randomEdges() and friends (qa/tests/common/test_filter_kruskal.cpp), drawn from
/// std::mt19937 after discarding `discard` outputs.
std::vector<EDGE> GenerateEdges( const JSON& aSpec, uint64_t aVertices,
                                 std::vector<std::pair<uint64_t, uint64_t>>& aSeeds )
{
    Fields( aSpec, { "seed", "discard", "count", "weight_range" },
            { "seeds", "islands", "weight_add", "weight_set" } );
    const uint64_t seed = Unsigned( aSpec, "seed" );
    const uint64_t discard = Unsigned( aSpec, "discard" );
    const uint64_t count = Unsigned( aSpec, "count" );
    const int64_t  range = Signed( aSpec, "weight_range", 1, UINT32_MAX );
    const uint64_t seedCount = aSpec.contains( "seeds" ) ? Unsigned( aSpec, "seeds" ) : 0;
    const uint64_t islands = aSpec.contains( "islands" ) ? Unsigned( aSpec, "islands" ) : 0;
    const int64_t  add = aSpec.contains( "weight_add" )
                                 ? Signed( aSpec, "weight_add", -( 1 << 30 ), 1 << 30 )
                                 : 0;
    const std::optional<int64_t> set =
            aSpec.contains( "weight_set" )
                    ? std::optional<int64_t>( Signed( aSpec, "weight_set", -( 1 << 30 ), 1 << 30 ) )
                    : std::nullopt;

    if( seed > UINT32_MAX || discard > ( uint64_t( 1 ) << 24 ) || count > MAX_EDGES
        || seedCount > MAX_EDGES )
    {
        Fail( "generator out of range" );
    }

    if( aVertices == 0 || ( islands != 0 && ( islands > aVertices || aVertices % islands != 0 ) ) )
        Fail( "generator needs vertices" );

    std::mt19937 rng( static_cast<uint32_t>( seed ) );
    rng.discard( discard );

    for( uint64_t ii = 0; ii < seedCount; ++ii )
    {
        const uint64_t a = rng() % aVertices;
        const uint64_t b = rng() % aVertices;
        aSeeds.emplace_back( a, b );
    }

    std::vector<EDGE> edges;

    for( uint64_t ii = 0; ii < count; ++ii )
    {
        EDGE edge;

        if( islands != 0 )
        {
            const uint64_t size = aVertices / islands;
            const uint64_t island = rng() % islands;
            edge.u = uint32_t( island * size + rng() % size );
            edge.v = uint32_t( island * size + rng() % size );
            edge.weight = int64_t( rng() % uint32_t( range ) );
        }
        else
        {
            edge.u = uint32_t( rng() % aVertices );
            edge.v = uint32_t( rng() % aVertices );
            edge.weight = int64_t( rng() % uint32_t( range ) );
        }

        edge.weight = set ? *set : edge.weight + add;
        edge.id = uint32_t( ii );
        edges.push_back( edge );
    }

    return edges;
}


template <typename LESS>
JSON RunKruskal( std::vector<EDGE>& aEdges, KI_UNION_FIND& aForest, LESS aLess )
{
    JSON emitted = JSON::array();

    const auto endpoints = [&]( const EDGE& aEdge ) -> std::pair<size_t, size_t>
    {
        // Unite() and Connected() read both indices next: refuse before they would
        // read outside the parent array.
        if( aEdge.u >= aForest.Size() || aEdge.v >= aForest.Size() )
            Fail( "endpoint out of range" );

        return { aEdge.u, aEdge.v };
    };

    const size_t returned = KI_MST::FilterKruskal<EDGE>(
            std::span<EDGE>( aEdges ), aForest, aLess, endpoints,
            [&]( const EDGE& aEdge ) { emitted.push_back( aEdge.id ); } );

    JSON order = JSON::array();

    for( const EDGE& edge : aEdges )
        order.push_back( edge.id );

    return { { "emitted", std::move( emitted ) },
             { "returned", returned },
             { "order", std::move( order ) },
             { "components", aForest.ComponentCount() },
             { "parents", Parents( aForest ) } };
}


JSON FilterKruskal( const JSON& aInput )
{
    Fields( aInput, { "op", "vertices", "less" }, { "edges", "generate", "unite" } );
    const uint64_t    vertices = Unsigned( aInput, "vertices" );
    const std::string less = Text( aInput, "less" );

    if( vertices > MAX_SET )
        Fail( "count too large" );

    if( less != "weight" && less != "weight_id" && less != "weight_uv" )
        Fail( "unknown order" );

    if( aInput.contains( "edges" ) == aInput.contains( "generate" ) )
        Fail( "expected edges or generate" );

    std::vector<std::pair<uint64_t, uint64_t>> seeds;
    std::vector<EDGE>                          edges;

    if( aInput.contains( "edges" ) )
    {
        for( const JSON& item : Array( aInput, "edges", MAX_SET ) )
        {
            if( !item.is_array() || item.size() != 3 )
                Fail( "expected edge triple" );

            EDGE edge;
            edge.u = uint32_t( Signed( { { "u", item[0] } }, "u", 0, UINT32_MAX ) );
            edge.v = uint32_t( Signed( { { "v", item[1] } }, "v", 0, UINT32_MAX ) );
            edge.weight = Signed( { { "w", item[2] } }, "w", INT64_MIN, INT64_MAX );
            edge.id = uint32_t( edges.size() );
            edges.push_back( edge );
        }
    }
    else
    {
        edges = GenerateEdges( aInput.at( "generate" ), vertices, seeds );
    }

    if( aInput.contains( "unite" ) )
    {
        for( const JSON& pair : Array( aInput, "unite", MAX_SET ) )
        {
            if( !pair.is_array() || pair.size() != 2 )
                Fail( "expected index pair" );

            seeds.emplace_back( Unsigned( { { "a", pair[0] } }, "a" ),
                                Unsigned( { { "b", pair[1] } }, "b" ) );
        }
    }

    KI_UNION_FIND forest( vertices );

    for( const auto& [a, b] : seeds )
    {
        RequireIndex( forest, a );
        RequireIndex( forest, b );
        forest.Unite( a, b );
    }

    if( less == "weight" )
        return RunKruskal( edges, forest,
                           []( const EDGE& aA, const EDGE& aB ) { return aA.weight < aB.weight; } );

    if( less == "weight_id" )
        return RunKruskal( edges, forest,
                           []( const EDGE& aA, const EDGE& aB )
                           {
                               return std::tie( aA.weight, aA.id ) < std::tie( aB.weight, aB.id );
                           } );

    return RunKruskal( edges, forest,
                       []( const EDGE& aA, const EDGE& aB )
                       {
                           return std::tie( aA.weight, aA.u, aA.v )
                                  < std::tie( aB.weight, aB.u, aB.v );
                       } );
}


// ---- KISPINLOCK and SYNC_QUEUE ---------------------------------------------------------

JSON SpinLock( const JSON& aInput )
{
    Fields( aInput, { "op", "steps" } );
    const JSON& steps = Array( aInput, "steps", MAX_STEPS );

    for( const JSON& step : steps )
    {
        if( !step.is_string() )
            Fail( "unknown operation" );

        const std::string op = step.get<std::string>();

        if( op != "lock" && op != "try_lock" && op != "unlock" && op != "test" )
            Fail( "unknown operation" );
    }

    KISPINLOCK lock;
    JSON       results = JSON::array();

    for( const JSON& step : steps )
    {
        const std::string op = step.get<std::string>();

        if( op == "lock" )
        {
            // With one thread a held lock is never released: lock() never returns.
            if( lock.test() )
                Fail( "lock would spin forever" );

            lock.lock();
            results.push_back( nullptr );
        }
        else if( op == "try_lock" )
        {
            results.push_back( lock.try_lock() );
        }
        else if( op == "unlock" )
        {
            lock.unlock();
            results.push_back( nullptr );
        }
        else
        {
            results.push_back( lock.test() );
        }
    }

    return { { "results", std::move( results ) } };
}


JSON SyncQueue( const JSON& aInput )
{
    Fields( aInput, { "op", "receiver", "steps" } );
    std::string receiver = Text( aInput, "receiver" );
    const JSON& steps = Array( aInput, "steps", MAX_STEPS );

    for( const JSON& step : steps )
    {
        const std::string op = Text( step, "op" );

        if( op == "push" || op == "move_push" )
        {
            Fields( step, { "op", "value" } );
            Text( step, "value" );
        }
        else if( op == "pop" || op == "empty" || op == "size" || op == "clear" )
        {
            Fields( step, { "op" } );
        }
        else
        {
            Fail( "unknown operation" );
        }
    }

    SYNC_QUEUE<std::string> queue;
    JSON                    results = JSON::array();

    for( const JSON& step : steps )
    {
        const std::string op = Text( step, "op" );

        if( op == "push" )
        {
            const std::string value = Text( step, "value" );
            queue.push( value );
            results.push_back( nullptr );
        }
        else if( op == "move_push" )
        {
            std::string value = Text( step, "value" );
            queue.move_push( std::move( value ) );
            results.push_back( nullptr );
        }
        else if( op == "pop" )
        {
            const bool popped = queue.pop( receiver );
            results.push_back( { { "popped", popped }, { "receiver", receiver } } );
        }
        else if( op == "empty" )
        {
            results.push_back( queue.empty() );
        }
        else if( op == "size" )
        {
            results.push_back( queue.size() );
        }
        else
        {
            queue.clear();
            results.push_back( nullptr );
        }
    }

    return { { "results", std::move( results ) } };
}


// ---- profile.h ------------------------------------------------------------------------

/// The duration types a script can name for SinceStart<DURATION> / SCOPED_PROF_TIMER.
template <typename DURATION>
JSON DurationJson( const DURATION& aDuration )
{
    if constexpr( std::is_floating_point_v<typename DURATION::rep> )
        return Bits( aDuration.count() );
    else
        return aDuration.count();
}


template <typename FUNC>
JSON WithDuration( const std::string& aName, FUNC&& aFunc )
{
    if( aName == "double_nano" )
        return aFunc( std::chrono::duration<double, std::nano>() );
    if( aName == "double_micro" )
        return aFunc( std::chrono::duration<double, std::micro>() );
    if( aName == "double_milli" )
        return aFunc( std::chrono::duration<double, std::milli>() );
    if( aName == "double_seconds" )
        return aFunc( std::chrono::duration<double>() );
    if( aName == "nanoseconds" )
        return aFunc( std::chrono::nanoseconds() );
    if( aName == "microseconds" )
        return aFunc( std::chrono::microseconds() );
    if( aName == "milliseconds" )
        return aFunc( std::chrono::milliseconds() );
    if( aName == "seconds" )
        return aFunc( std::chrono::seconds() );

    Fail( "unknown duration" );
}


bool IsDuration( const std::string& aName )
{
    for( const char* name : { "double_nano", "double_micro", "double_milli", "double_seconds",
                              "nanoseconds", "microseconds", "milliseconds", "seconds" } )
    {
        if( aName == name )
            return true;
    }

    return false;
}


bool SinceLast( const JSON& aStep )
{
    return aStep.contains( "since_last" ) ? Boolean( aStep, "since_last" ) : false;
}


/// A PROF_TIMER slot and whether its m_started has ever been written.
struct TIMER_SLOT
{
    std::unique_ptr<PROF_TIMER> timer;
    bool                        startedWritten = false;
};


class PROFILE_SCRIPT
{
public:
    JSON Run( const JSON& aSteps )
    {
        JSON results = JSON::array();

        for( const JSON& step : aSteps )
        {
            results.push_back( Step( step ) );

            if( g_chrono.exhausted )
                Fail( "clock script exhausted" );
        }

        return results;
    }

    static void Validate( const JSON& aStep, bool aBase )
    {
        const std::string op = Text( aStep, "op" );
        const auto        timerFields = [&]( std::initializer_list<const char*> aExtra,
                                       std::initializer_list<const char*> aOptional )
        {
            std::vector<const char*> required{ "op" };

            if( !aBase )
                required.push_back( "timer" );

            required.insert( required.end(), aExtra.begin(), aExtra.end() );
            ValidateFields( aStep, required, aOptional );

            if( !aBase )
                Signed( aStep, "timer", 0, TIMERS - 1 );
        };

        if( op == "timer_start" || op == "timer_stop" || op == "timer_show"
            || op == "timer_to_string" || op == "timer_get_name" )
        {
            timerFields( {}, {} );
        }
        else if( op == "timer_msecs" )
        {
            timerFields( {}, { "since_last" } );
            SinceLast( aStep );
        }
        else if( op == "timer_since_start" )
        {
            timerFields( { "duration" }, { "since_last" } );
            SinceLast( aStep );

            if( !IsDuration( Text( aStep, "duration" ) ) )
                Fail( "unknown duration" );
        }
        else if( aBase )
        {
            Fail( "unknown operation" );
        }
        else if( op == "timer_new" )
        {
            Fields( aStep, { "op", "timer", "name" }, { "autostart" } );
            Signed( aStep, "timer", 0, TIMERS - 1 );
            Text( aStep, "name" );

            if( aStep.contains( "autostart" ) )
                Boolean( aStep, "autostart" );
        }
        else if( op == "timer_default" )
        {
            Fields( aStep, { "op", "timer" } );
            Signed( aStep, "timer", 0, TIMERS - 1 );
        }
        else if( op == "scoped" )
        {
            Fields( aStep, { "op", "duration", "base" } );

            if( !IsDuration( Text( aStep, "duration" ) ) )
                Fail( "unknown duration" );

            for( const JSON& step : Array( aStep, "base", MAX_STEPS ) )
                Validate( step, true );
        }
        else if( op == "probe_new" )
        {
            Fields( aStep, { "op", "probe", "name" }, { "stages" } );
            Signed( aStep, "probe", 0, PROBES - 1 );
            Text( aStep, "name" );

            if( aStep.contains( "stages" ) )
                Signed( aStep, "stages", INT_MIN, MAX_STAGES );
        }
        else if( op == "probe_reset" || op == "probe_to_string" )
        {
            Fields( aStep, { "op", "probe" } );
            Signed( aStep, "probe", 0, PROBES - 1 );
        }
        else if( op == "probe_checkpoint" )
        {
            Fields( aStep, { "op", "probe", "name" } );
            Signed( aStep, "probe", 0, PROBES - 1 );
            Text( aStep, "name" );
        }
        else if( op == "probe_add_timer" )
        {
            Fields( aStep, { "op", "probe", "timer" } );
            Signed( aStep, "probe", 0, PROBES - 1 );
            Signed( aStep, "timer", 0, TIMERS - 1 );
        }
        else if( op == "probe_delta_ms" )
        {
            Fields( aStep, { "op", "probe", "a", "b" } );
            Signed( aStep, "probe", 0, PROBES - 1 );
            Signed( aStep, "a", INT64_MIN, INT64_MAX );
            Signed( aStep, "b", INT64_MIN, INT64_MAX );
        }
        else if( op == "counter_new" )
        {
            Fields( aStep, { "op", "counter" }, { "name" } );
            Signed( aStep, "counter", 0, COUNTERS - 1 );

            if( aStep.contains( "name" ) )
                Text( aStep, "name" );
        }
        else if( op == "counter_inc" || op == "counter_count" || op == "counter_reset"
                 || op == "counter_show" )
        {
            Fields( aStep, { "op", "counter" } );
            Signed( aStep, "counter", 0, COUNTERS - 1 );
        }
        else if( op == "counter_set" )
        {
            Fields( aStep, { "op", "counter", "value" } );
            Signed( aStep, "counter", 0, COUNTERS - 1 );
            Unsigned( aStep, "value" );
        }
        else
        {
            Fail( "unknown operation" );
        }
    }

private:
    static void ValidateFields( const JSON& aStep, const std::vector<const char*>& aRequired,
                                std::initializer_list<const char*> aOptional )
    {
        if( !aStep.is_object() )
            Fail( "unexpected or missing field" );

        for( const char* key : aRequired )
        {
            if( !aStep.contains( key ) )
                Fail( "unexpected or missing field" );
        }

        for( const auto& [key, value] : aStep.items() )
        {
            auto same = [&]( const char* aName ) { return key == aName; };

            if( std::none_of( aRequired.begin(), aRequired.end(), same )
                && std::none_of( aOptional.begin(), aOptional.end(), same ) )
            {
                Fail( "unexpected or missing field" );
            }
        }
    }

    TIMER_SLOT& Timer( const JSON& aStep )
    {
        TIMER_SLOT& slot = m_timers[Signed( aStep, "timer", 0, TIMERS - 1 )];

        if( !slot.timer )
            Fail( "timer slot empty" );

        return slot;
    }

    LATENCY_PROBE& Probe( const JSON& aStep )
    {
        std::unique_ptr<LATENCY_PROBE>& slot = m_probes[Signed( aStep, "probe", 0, PROBES - 1 )];

        if( !slot )
            Fail( "probe slot empty" );

        return *slot;
    }

    PROF_COUNTER& Counter( const JSON& aStep )
    {
        std::unique_ptr<PROF_COUNTER>& slot =
                m_counters[Signed( aStep, "counter", 0, COUNTERS - 1 )];

        if( !slot )
            Fail( "counter slot empty" );

        return *slot;
    }

    /// The PROF_TIMER operations, on a slot's timer or a scoped timer's base.
    static JSON TimerStep( const JSON& aStep, PROF_TIMER& aTimer, bool& aStartedWritten )
    {
        const std::string op = Text( aStep, "op" );

        if( op == "timer_start" )
        {
            aTimer.Start();
            aStartedWritten = true;
            return nullptr;
        }

        if( op == "timer_stop" )
        {
            aTimer.Stop();
            return nullptr;
        }

        if( op == "timer_msecs" )
        {
            if( aStep.contains( "since_last" ) )
                return Bits( aTimer.msecs( SinceLast( aStep ) ) );

            return Bits( aTimer.msecs() );
        }

        if( op == "timer_since_start" )
        {
            return WithDuration( Text( aStep, "duration" ),
                                 [&]( auto aType ) -> JSON
                                 {
                                     using DURATION = decltype( aType );

                                     if( aStep.contains( "since_last" ) )
                                         return DurationJson( aTimer.SinceStart<DURATION>(
                                                 SinceLast( aStep ) ) );

                                     return DurationJson( aTimer.SinceStart<DURATION>() );
                                 } );
        }

        if( op == "timer_show" )
        {
            std::stringstream stream;
            aTimer.Show( stream );
            return stream.str();
        }

        if( op == "timer_to_string" )
        {
            // m_started is written only by Start(); reading it before is undefined.
            if( !aStartedWritten )
                Fail( "indeterminate m_started" );

            return aTimer.to_string();
        }

        return aTimer.GetName();
    }

    JSON Step( const JSON& aStep )
    {
        const std::string op = Text( aStep, "op" );

        if( op == "timer_new" )
        {
            TIMER_SLOT& slot = m_timers[Signed( aStep, "timer", 0, TIMERS - 1 )];
            const bool  autostart = aStep.contains( "autostart" ) ? Boolean( aStep, "autostart" )
                                                                  : true;
            slot.timer.reset();

            if( aStep.contains( "autostart" ) )
                slot.timer = std::make_unique<PROF_TIMER>( Text( aStep, "name" ), autostart );
            else
                slot.timer = std::make_unique<PROF_TIMER>( Text( aStep, "name" ) );

            slot.startedWritten = autostart;
            return nullptr;
        }

        if( op == "timer_default" )
        {
            TIMER_SLOT& slot = m_timers[Signed( aStep, "timer", 0, TIMERS - 1 )];
            slot.timer.reset();
            slot.timer = std::make_unique<PROF_TIMER>();
            slot.startedWritten = true;
            return nullptr;
        }

        if( op.rfind( "timer_", 0 ) == 0 )
        {
            TIMER_SLOT& slot = Timer( aStep );
            return TimerStep( aStep, *slot.timer, slot.startedWritten );
        }

        if( op == "scoped" )
        {
            return WithDuration( Text( aStep, "duration" ),
                                 [&]( auto aType ) -> JSON
                                 {
                                     using DURATION = decltype( aType );
                                     DURATION duration{};
                                     JSON     base = JSON::array();

                                     {
                                         SCOPED_PROF_TIMER<DURATION> scoped( duration );
                                         bool                        started = true;

                                         for( const JSON& step : aStep.at( "base" ) )
                                             base.push_back( TimerStep( step, scoped, started ) );
                                     }

                                     return { { "base", std::move( base ) },
                                              { "duration", DurationJson( duration ) } };
                                 } );
        }

        if( op == "probe_new" )
        {
            std::unique_ptr<LATENCY_PROBE>& slot =
                    m_probes[Signed( aStep, "probe", 0, PROBES - 1 )];
            slot.reset();

            try
            {
                if( aStep.contains( "stages" ) )
                {
                    slot = std::make_unique<LATENCY_PROBE>(
                            Text( aStep, "name" ),
                            int( Signed( aStep, "stages", INT_MIN, MAX_STAGES ) ) );
                }
                else
                {
                    slot = std::make_unique<LATENCY_PROBE>( Text( aStep, "name" ) );
                }
            }
            catch( const std::length_error& error )
            {
                return LengthError( error );
            }

            return nullptr;
        }

        if( op == "probe_reset" )
        {
            Probe( aStep ).Reset();
            return nullptr;
        }

        if( op == "probe_checkpoint" )
        {
            Probe( aStep ).Checkpoint( Text( aStep, "name" ) );
            return nullptr;
        }

        if( op == "probe_add_timer" )
        {
            LATENCY_PROBE& probe = Probe( aStep );
            probe.AddTimer( *Timer( aStep ).timer );
            return nullptr;
        }

        if( op == "probe_to_string" )
            return Probe( aStep ).to_string();

        if( op == "probe_delta_ms" )
        {
            LATENCY_PROBE& probe = Probe( aStep );
            const int64_t  a = Signed( aStep, "a", INT64_MIN, INT64_MAX );
            const int64_t  b = Signed( aStep, "b", INT64_MIN, INT64_MAX );
            int64_t        difference = 0;

            // a - b on signed int64 nanosecond durations.
            if( __builtin_sub_overflow( a, b, &difference ) )
                Fail( "time point difference overflows" );

            using TIME_POINT = LATENCY_PROBE::TIME_POINT;
            return Bits( probe.delta_ms( TIME_POINT( std::chrono::nanoseconds( a ) ),
                                         TIME_POINT( std::chrono::nanoseconds( b ) ) ) );
        }

        if( op == "counter_new" )
        {
            std::unique_ptr<PROF_COUNTER>& slot =
                    m_counters[Signed( aStep, "counter", 0, COUNTERS - 1 )];

            if( aStep.contains( "name" ) )
                slot = std::make_unique<PROF_COUNTER>( Text( aStep, "name" ) );
            else
                slot = std::make_unique<PROF_COUNTER>();

            return nullptr;
        }

        if( op == "counter_inc" )
            return Counter( aStep )++;

        if( op == "counter_count" )
            return Counter( aStep ).Count();

        if( op == "counter_reset" )
        {
            Counter( aStep ).Reset();
            return nullptr;
        }

        if( op == "counter_show" )
        {
            std::stringstream stream;
            Counter( aStep ).Show( stream );
            return stream.str();
        }

        ( Counter( aStep ).*Get( COUNTER_COUNT{} ) ).store( Unsigned( aStep, "value" ) );
        return nullptr;
    }

    TIMER_SLOT                     m_timers[TIMERS];
    std::unique_ptr<LATENCY_PROBE> m_probes[PROBES];
    std::unique_ptr<PROF_COUNTER>  m_counters[COUNTERS];
};


JSON Profile( const JSON& aInput )
{
    Fields( aInput, { "op", "clock", "steps" } );
    std::vector<int64_t> values;

    for( const JSON& value : Array( aInput, "clock", MAX_CLOCK ) )
    {
        // Every difference the timers and probes take is between two scripted values
        // or the epoch; bounding the values keeps every such subtraction defined.
        values.push_back( Signed( { { "t", value } }, "t", -CLOCK_LIMIT, CLOCK_LIMIT ) );
    }

    const JSON& steps = Array( aInput, "steps", MAX_STEPS );

    for( const JSON& step : steps )
        PROFILE_SCRIPT::Validate( step, false );

    struct ARMED
    {
        ARMED( std::vector<int64_t> aValues )
        {
            g_chrono = CHRONO_SCRIPT{ true, std::move( aValues ), 0, false };
        }

        ~ARMED() { g_chrono = CHRONO_SCRIPT{}; }
    };

    ARMED          armed( std::move( values ) );
    PROFILE_SCRIPT script;
    JSON           results = script.Run( steps );
    return { { "results", std::move( results ) }, { "clock_calls", g_chrono.calls } };
}


// ---- small helpers --------------------------------------------------------------------

struct PAIR
{
    int    a;
    double b;
};

enum class E_I8 : int8_t
{
};
enum class E_U8 : uint8_t
{
};
enum class E_I16 : int16_t
{
};
enum class E_U32 : uint32_t
{
};
enum class E_I64 : int64_t
{
};
enum class E_U64 : uint64_t
{
};
enum E_INT : int
{
};


JSON Helpers( const JSON& aInput, const std::string& aOp )
{
    if( aOp == "array_dim" )
    {
        Fields( aInput, { "op", "kind" } );
        const std::string kind = Text( aInput, "kind" );
        static const char   chars1[1] = {};
        static const int    ints3[3] = {};
        static const double doubles16[16] = {};
        static const PAIR   pairs7[7] = {};
        static const char   chars1000[1000] = {};

        if( kind == "char[1]" )
            return { { "value", arrayDim( chars1 ) } };
        if( kind == "int[3]" )
            return { { "value", arrayDim( ints3 ) } };
        if( kind == "double[16]" )
            return { { "value", arrayDim( doubles16 ) } };
        if( kind == "pair[7]" )
            return { { "value", arrayDim( pairs7 ) } };
        if( kind == "char[1000]" )
            return { { "value", arrayDim( chars1000 ) } };

        Fail( "unknown kind" );
    }

    if( aOp == "to_underlying" )
    {
        Fields( aInput, { "op", "type", "value" } );
        const std::string type = Text( aInput, "type" );

        if( type == "int8" )
            return { { "value", to_underlying( static_cast<E_I8>(
                                        Signed( aInput, "value", INT8_MIN, INT8_MAX ) ) ) } };
        if( type == "uint8" )
            return { { "value", to_underlying( static_cast<E_U8>(
                                        Signed( aInput, "value", 0, UINT8_MAX ) ) ) } };
        if( type == "int16" )
            return { { "value", to_underlying( static_cast<E_I16>(
                                        Signed( aInput, "value", INT16_MIN, INT16_MAX ) ) ) } };
        if( type == "uint32" )
            return { { "value", to_underlying( static_cast<E_U32>(
                                        Signed( aInput, "value", 0, UINT32_MAX ) ) ) } };
        if( type == "int64" )
            return { { "value", to_underlying( static_cast<E_I64>(
                                        Signed( aInput, "value", INT64_MIN, INT64_MAX ) ) ) } };
        if( type == "uint64" )
            return { { "value", to_underlying( static_cast<E_U64>(
                                        Unsigned( aInput, "value" ) ) ) } };
        if( type == "int" )
            return { { "value", to_underlying( static_cast<E_INT>(
                                        Signed( aInput, "value", INT_MIN, INT_MAX ) ) ) } };

        Fail( "unknown type" );
    }

    Fields( aInput, { "op", "value" } );
    const std::string value = Text( aInput, "value" );
    ignore_unused( value );
    return { { "value", value } };
}


JSON Run( const JSON& aInput )
{
    const std::string op = Text( aInput, "op" );

    if( op == "minoptmax" )
        return MinOptMax( aInput );

    if( op == "union_find" )
        return UnionFind( aInput );

    if( op == "filter_kruskal" )
        return FilterKruskal( aInput );

    if( op == "spinlock" )
        return SpinLock( aInput );

    if( op == "sync_queue" )
        return SyncQueue( aInput );

    if( op == "profile" )
        return Profile( aInput );

    if( op == "observable" )
        return oracle::RunCoreObservable( aInput );

    if( op == "array_dim" || op == "to_underlying" || op == "ignore_unused" )
        return Helpers( aInput, op );

    Fail( "unknown operation" );
}
} // namespace


namespace oracle
{
void RegisterCoreIncludeP001( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.core.include.core.application.pure.p001",
                   "libs/core/include/core/*.h", Run );
}
} // namespace oracle
