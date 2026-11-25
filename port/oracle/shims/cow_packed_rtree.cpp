// SPDX-License-Identifier: GPL-3.0-or-later
// Calls both pinned, unmodified owned headers. Explicit-instantiation access
// observes real node sharing, allocator state, and packed level layout.
#include "registry.h"
#include <geometry/rtree/dynamic_rtree_cow.h>
#include <geometry/rtree/packed_rtree.h>
#include <array>
#include <cmath>
#include <optional>
#include <string>

namespace
{
using oracle::JSON;
constexpr int LIMIT = 8192;
struct ITEM
{
    int tag;
};

[[noreturn]] void Refuse( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}

void Keys( const JSON& aValue, std::initializer_list<const char*> aRequired )
{
    if( !aValue.is_object() )
        Refuse( "expected object" );

    for( const char* key : aRequired )
        if( !aValue.contains( key ) )
            Refuse( "unexpected or missing field" );

    for( const auto& [key, value] : aValue.items() )
        if( std::none_of( aRequired.begin(), aRequired.end(),
                          [&]( const char* aKey )
                          {
                              return key == aKey;
                          } ) )
            Refuse( "unexpected or missing field" );
}

int64_t Number( const JSON& aValue, const char* aKey, int64_t aMin, int64_t aMax )
{
    if( !aValue.contains( aKey ) )
        Refuse( std::string( "integer out of range: " ) + aKey );
    const JSON& v = aValue.at( aKey );
    if( !v.is_number_integer() || ( v.is_number_unsigned() && v.get<uint64_t>() > static_cast<uint64_t>( INT64_MAX ) ) )
        Refuse( std::string( "integer out of range: " ) + aKey );
    int64_t n = v.get<int64_t>();
    if( n < aMin || n > aMax )
        Refuse( std::string( "integer out of range: " ) + aKey );
    return n;
}

template <class E, int D>
std::array<E, D> Coords( const JSON& aValue, const char* aKey )
{
    const JSON& a = aValue.at( aKey );
    if( !a.is_array() || a.size() != D )
        Refuse( "invalid coordinates" );
    std::array<E, D> out;
    for( int d = 0; d < D; ++d )
    {
        const JSON& v = a[d];
        if( !v.is_number_integer()
            || ( v.is_number_unsigned()
                 && v.get<uint64_t>() > static_cast<uint64_t>( std::numeric_limits<E>::max() ) ) )
            Refuse( "invalid coordinates" );
        int64_t n = v.get<int64_t>();
        if( n < std::numeric_limits<E>::lowest() || n > std::numeric_limits<E>::max() )
            Refuse( "invalid coordinates" );
        out[d] = static_cast<E>( n );
    }
    return out;
}

ITEM* Item( const JSON& aValue, std::array<ITEM, LIMIT>& aItems )
{
    if( aValue.is_null() )
        return nullptr;
    if( !aValue.is_number_integer() || ( aValue.is_number_unsigned() && aValue.get<uint64_t>() >= LIMIT ) )
        Refuse( "invalid item" );
    int64_t n = aValue.get<int64_t>();
    if( n < 0 || n >= LIMIT )
        Refuse( "invalid item" );
    return &aItems[n];
}
JSON WireItem( const ITEM* aItem )
{
    return aItem ? JSON( aItem->tag ) : JSON( nullptr );
}

template <class TAG, auto MEMBER>
struct ACCESS
{
    friend auto Get( TAG ) { return MEMBER; }
};
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wnon-template-friend"
template <class TREE>
struct ROOT_TAG
{
    friend auto Get( ROOT_TAG );
};
template <class TREE>
struct ALLOC_TAG
{
    friend auto Get( ALLOC_TAG );
};
template <class TREE>
struct CHAIN_TAG
{
    friend auto Get( CHAIN_TAG );
};
template <class TREE>
struct BOUNDS_TAG
{
    friend auto Get( BOUNDS_TAG );
};
template <class TREE>
struct COUNTS_TAG
{
    friend auto Get( COUNTS_TAG );
};
template <class TREE>
struct OFFSETS_TAG
{
    friend auto Get( OFFSETS_TAG );
};
#pragma GCC diagnostic pop
#define CONFIG( E, D, M )                                                                                              \
    template struct ACCESS<ROOT_TAG<KIRTREE::COW_RTREE<ITEM*, E, D, M>>, &KIRTREE::COW_RTREE<ITEM*, E, D, M>::m_root>; \
    template struct ACCESS<ALLOC_TAG<KIRTREE::COW_RTREE<ITEM*, E, D, M>>,                                              \
                           &KIRTREE::COW_RTREE<ITEM*, E, D, M>::m_allocator>;                                          \
    template struct ACCESS<CHAIN_TAG<KIRTREE::COW_RTREE<ITEM*, E, D, M>>,                                              \
                           &KIRTREE::COW_RTREE<ITEM*, E, D, M>::m_parentChain>;                                        \
    template struct ACCESS<BOUNDS_TAG<KIRTREE::PACKED_RTREE<ITEM*, E, D, M>>,                                          \
                           &KIRTREE::PACKED_RTREE<ITEM*, E, D, M>::m_bounds>;                                          \
    template struct ACCESS<COUNTS_TAG<KIRTREE::PACKED_RTREE<ITEM*, E, D, M>>,                                          \
                           &KIRTREE::PACKED_RTREE<ITEM*, E, D, M>::m_counts>;                                          \
    template struct ACCESS<OFFSETS_TAG<KIRTREE::PACKED_RTREE<ITEM*, E, D, M>>,                                         \
                           &KIRTREE::PACKED_RTREE<ITEM*, E, D, M>::m_levelOffsets>;
CONFIG( int, 2, 4 )
CONFIG( int, 2, 8 )
CONFIG( int, 2, 16 )
CONFIG( int, 2, 28 )
CONFIG( int, 3, 5 )
CONFIG( int, 3, 16 )
CONFIG( int, 1, 8 )
CONFIG( int, 1, 3 )
CONFIG( int64_t, 2, 16 )
CONFIG( int64_t, 3, 8 )
#undef CONFIG

template <class E, int D, int M>
JSON Snapshot( const KIRTREE::RTREE_NODE<ITEM*, E, D, M>* aNode )
{
    if( !aNode )
        return nullptr;
    JSON children = JSON::array();
    for( int i = 0; i < aNode->count; ++i )
    {
        E lo[D], hi[D];
        aNode->GetChildBounds( i, lo, hi );
        JSON child = { { "min", lo }, { "max", hi } };
        if( aNode->IsLeaf() )
        {
            E il[D], ih[D];
            aNode->GetInsertBounds( i, il, ih );
            child["insert_min"] = il;
            child["insert_max"] = ih;
            child["item"] = WireItem( aNode->data[i] );
        }
        else
            child["child"] = Snapshot( aNode->children[i] );
        children.push_back( child );
    }
    return { { "level", aNode->level }, { "refs", aNode->refcount.load() }, { "children", children } };
}

template <class E, int D, int M>
JSON Cow( const JSON& aSteps )
{
    using TREE = KIRTREE::COW_RTREE<ITEM*, E, D, M>;
    std::array<std::unique_ptr<TREE>, 4> trees;
    for( auto& t : trees )
        t = std::make_unique<TREE>();
    std::array<ITEM, LIMIT> items;
    for( int i = 0; i < LIMIT; ++i )
        items[i].tag = i;
    JSON events = JSON::array();
    for( const JSON& s : aSteps )
    {
        if( !s.contains( "op" ) || !s["op"].is_string() )
            Refuse( "expected op string" );
        std::string op = s["op"];
        int         a = Number( s, "a", 0, 3 );
        if( op == "new" )
        {
            Keys( s, { "op", "a" } );
            trees[a] = std::make_unique<TREE>();
            events.push_back( JSON::object() );
            continue;
        }
        if( !trees[a] )
            Refuse( "missing tree" );
        TREE& tree = *trees[a];
        JSON  event = JSON::object();
        if( op == "insert" || op == "remove" )
        {
            Keys( s, { "op", "a", "min", "max", "item" } );
            auto  min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            ITEM* item = Item( s["item"], items );
            if( op == "insert" )
            {
                if( tree.size() >= LIMIT )
                    Refuse( "resource limit" );
                if( !( tree.*Get( ALLOC_TAG<TREE>{} ) ) )
                    Refuse( "tree invariant violation" );
                tree.Insert( min.data(), max.data(), item );
            }
            else
                event["removed"] = tree.Remove( min.data(), max.data(), item );
        }
        else if( op == "bulk" )
        {
            Keys( s, { "op", "a", "entries" } );
            const JSON& es = s["entries"];
            if( !es.is_array() || es.size() > LIMIT )
                Refuse( "invalid entries" );
            std::vector<typename TREE::BULK_ENTRY> batch;
            for( const JSON& e : es )
            {
                Keys( e, { "min", "max", "item" } );
                auto                      min = Coords<E, D>( e, "min" ), max = Coords<E, D>( e, "max" );
                typename TREE::BULK_ENTRY b;
                std::copy( min.begin(), min.end(), b.min );
                std::copy( max.begin(), max.end(), b.max );
                b.data = Item( e["item"], items );
                batch.push_back( b );
            }
            if constexpr( D < 2 )
            {
                if( !batch.empty() )
                    Refuse( "invalid template parameters" );
            }
            // Do not execute an undefined floating-to-integer cast in the oracle.
            if( !batch.empty() )
            {
                if( !( tree.*Get( ALLOC_TAG<TREE>{} ) ) )
                    Refuse( "tree invariant violation" );
                E lo[D], hi[D];
                std::copy( batch[0].min, batch[0].min + D, lo );
                std::copy( batch[0].max, batch[0].max + D, hi );
                for( const auto& e : batch )
                    for( int d = 0; d < D; ++d )
                    {
                        lo[d] = std::min( lo[d], e.min[d] );
                        hi[d] = std::max( hi[d], e.max[d] );
                    }
                for( const auto& e : batch )
                    for( int d = 0; d < std::min( D, 2 ); ++d )
                    {
                        double range = static_cast<double>( hi[d] ) - lo[d];
                        double scale = range > 0 ? 65535.0 / range : 0.0;
                        double c = ( static_cast<double>( e.min[d] ) + e.max[d] ) * 0.5;
                        double x = ( c - lo[d] ) * scale;
                        if( !std::isfinite( x ) || x <= -1.0 || x >= 4294967296.0 )
                            Refuse( "floating conversion outside reference domain" );
                    }
            }
            tree.BulkLoad( batch );
            JSON after = JSON::array();
            for( const auto& b : batch )
                after.push_back( { { "min", b.min }, { "max", b.max }, { "item", WireItem( b.data ) } } );
            event["entries"] = after;
        }
        else if( op == "state" )
        {
            Keys( s, { "op", "a" } );
            JSON allocators = JSON::array();
            auto allocator = tree.*Get( ALLOC_TAG<TREE>{} );
            allocators.push_back( allocator ? JSON( allocator->MemoryUsage() ) : JSON( nullptr ) );
            for( auto chain = tree.*Get( CHAIN_TAG<TREE>{} ); chain; chain = chain->next )
                allocators.push_back( chain->allocator ? JSON( chain->allocator->MemoryUsage() ) : JSON( nullptr ) );
            event = { { "size", tree.size() },
                      { "empty", tree.empty() },
                      { "root", Snapshot( tree.*Get( ROOT_TAG<TREE>{} ) ) },
                      { "allocators", allocators } };
        }
        else if( op == "clear" || op == "destroy" )
        {
            Keys( s, { "op", "a" } );
            if( op == "clear" )
                tree.RemoveAll();
            else
                trees[a].reset();
        }
        else if( op == "iterate" )
        {
            Keys( s, { "op", "a", "skip", "take" } );
            int  skip = Number( s, "skip", 0, LIMIT ), take = Number( s, "take", 0, LIMIT );
            auto it = tree.begin(), end = tree.end(), same = it;
            ++same;
            bool equal = it == same;
            for( int i = 0; i < skip; ++i )
                ++it;
            JSON result = JSON::array();
            for( int i = 0; i < take && it != end; ++i, ++it )
                result.push_back( WireItem( *it ) );
            event = {
                { "items", result }, { "at_end", it == end }, { "current", WireItem( *it ) }, { "equal_next", equal }
            };
        }
        else if( op == "search" )
        {
            Keys( s, { "op", "a", "min", "max", "stop" } );
            auto min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            int  stop = Number( s, "stop", 0, LIMIT ), hits = 0;
            JSON result = JSON::array();
            auto visitor = [&]( ITEM* item )
            {
                result.push_back( WireItem( item ) );
                return ++hits < stop;
            };
            int found = tree.Search( min.data(), max.data(), visitor );
            event = { { "items", result }, { "found", found } };
        }
        else if( op == "clone" || op == "move_new" || op == "move_assign" )
        {
            Keys( s, { "op", "a", "src" } );
            int src = Number( s, "src", 0, 3 );
            if( !trees[src] )
                Refuse( "missing tree" );
            if( op == "clone" )
                trees[a] = std::make_unique<TREE>( trees[src]->Clone() );
            else if( a == src )
                tree = std::move( tree );
            else if( op == "move_new" )
                trees[a] = std::make_unique<TREE>( std::move( *trees[src] ) );
            else
                tree = std::move( *trees[src] );
        }
        else
            Refuse( "unknown op" );
        events.push_back( event );
    }
    return { { "events", events } };
}

template <class E, int D, int M>
JSON Packed( const JSON& aSteps )
{
    using TREE = KIRTREE::PACKED_RTREE<ITEM*, E, D, M>;
    std::array<TREE, 4>                   trees;
    std::array<typename TREE::Builder, 4> builders;
    std::array<size_t, 4>                 pending{};
    std::array<ITEM, LIMIT>               items;
    for( int i = 0; i < LIMIT; ++i )
        items[i].tag = i;
    JSON events = JSON::array();
    for( const JSON& s : aSteps )
    {
        if( !s.contains( "op" ) || !s["op"].is_string() )
            Refuse( "expected op string" );
        std::string op = s["op"];
        int         a = Number( s, "a", 0, 3 );
        TREE&       tree = trees[a];
        JSON        event = JSON::object();
        if( op == "reserve" )
        {
            Keys( s, { "op", "a", "count" } );
            builders[a].Reserve( Number( s, "count", 0, LIMIT ) );
        }
        else if( op == "add" )
        {
            Keys( s, { "op", "a", "min", "max", "item" } );
            auto  min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            ITEM* item = Item( s["item"], items );
            if( pending[a] >= LIMIT )
                Refuse( "resource limit" );
            builders[a].Add( min.data(), max.data(), item );
            pending[a]++;
        }
        else if( op == "build" )
        {
            Keys( s, { "op", "a" } );
            tree = builders[a].Build();
            pending[a] = 0;
        }
        else if( op == "state" )
        {
            Keys( s, { "op", "a" } );
            JSON bounds = JSON::array();
            for( const auto& row : tree.*Get( BOUNDS_TAG<TREE>{} ) )
                bounds.push_back( row );
            event = { { "size", tree.size() },
                      { "empty", tree.empty() },
                      { "memory", tree.MemoryUsage() },
                      { "bounds", bounds },
                      { "counts", tree.*Get( COUNTS_TAG<TREE>{} ) },
                      { "offsets", tree.*Get( OFFSETS_TAG<TREE>{} ) } };
        }
        else if( op == "iterate" )
        {
            Keys( s, { "op", "a", "skip", "take" } );
            int skip = Number( s, "skip", 0, LIMIT ), take = Number( s, "take", 0, LIMIT );
            if( static_cast<size_t>( skip ) > tree.size() )
                Refuse( "iterator outside data" );
            auto it = tree.begin(), end = tree.end();
            for( int i = 0; i < skip; ++i )
                ++it;
            JSON result = JSON::array();
            for( int i = 0; i < take && it != end; ++i )
            {
                auto old = it++;
                result.push_back( WireItem( *old ) );
                if( *old.operator->() != *old )
                    throw std::logic_error( "iterator arrow mismatch" );
            }
            event = { { "items", result },
                      { "at_end", it == end },
                      { "current", it == end ? JSON( nullptr ) : WireItem( *it ) } };
        }
        else if( op == "search" )
        {
            Keys( s, { "op", "a", "min", "max", "stop" } );
            auto min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            int  stop = Number( s, "stop", 0, LIMIT ), hits = 0;
            JSON result = JSON::array();
            auto visitor = [&]( ITEM* item )
            {
                result.push_back( WireItem( item ) );
                return ++hits < stop;
            };
            int found = tree.Search( min.data(), max.data(), visitor );
            event = { { "items", result }, { "found", found } };
        }
        else if( op == "move_new" || op == "move_assign" )
        {
            Keys( s, { "op", "a", "src" } );
            int src = Number( s, "src", 0, 3 );
            if( op == "move_new" && a != src )
            {
                TREE moved( std::move( trees[src] ) );
                tree = std::move( moved );
            }
            else
                tree = std::move( trees[src] );
        }
        else
            Refuse( "unknown op" );
        events.push_back( event );
    }
    return { { "events", events } };
}
JSON Run( const JSON& aInput )
{
    Keys( aInput, { "tree", "config", "steps" } );
    if( !aInput["config"].is_string() )
        Refuse( "expected config string" );
    if( !aInput["tree"].is_string() )
        Refuse( "expected tree string" );
    std::string c = aInput["config"], tree = aInput["tree"];
    const JSON& steps = aInput["steps"];
    if( !steps.is_array() || steps.empty() || steps.size() > 4096 )
        Refuse( "steps outside 1..4096" );
#define SELECT( NAME, E, D, M )                                                                                        \
    if( c == NAME )                                                                                                    \
    {                                                                                                                  \
        if( tree == "cow" )                                                                                            \
            return Cow<E, D, M>( steps );                                                                              \
        if( tree == "packed" )                                                                                         \
            return Packed<E, D, M>( steps );                                                                           \
        Refuse( "unknown tree" );                                                                                      \
    }
    SELECT( "i32.d2.m4", int, 2, 4 )
    SELECT( "i32.d2.m8", int, 2, 8 )
    SELECT( "i32.d2.m16", int, 2, 16 )
    SELECT( "i32.d2.m28", int, 2, 28 )
    SELECT( "i32.d3.m5", int, 3, 5 )
    SELECT( "i32.d3.m16", int, 3, 16 )
    SELECT( "i32.d1.m8", int, 1, 8 )
    SELECT( "i32.d1.m3", int, 1, 3 )
    SELECT( "i64.d2.m16", int64_t, 2, 16 )
    SELECT( "i64.d3.m8", int64_t, 3, 8 )
#undef SELECT
    Refuse( "unknown config" );
}
} // namespace
namespace oracle
{
void RegisterCowPackedRtree( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.geometry.rtree.application.pure.p002",
                   "libs/kimath/include/geometry/rtree/dynamic_rtree_cow.h", Run );
}
} // namespace oracle
