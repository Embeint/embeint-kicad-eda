// SPDX-License-Identifier: GPL-3.0-or-later
// Calls the pinned, unmodified dynamic_rtree.h. Standard explicit-instantiation
// access is used only to observe actual topology and validate nearest arithmetic.
#include "registry.h"
#include <geometry/rtree/dynamic_rtree.h>
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

template <class TAG, typename TAG::TYPE MEMBER>
struct ACCESS
{
    friend typename TAG::TYPE Get( TAG ) { return MEMBER; }
};
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wnon-template-friend"
template <class TREE>
struct ROOT_TAG
{
    using TYPE = typename TREE::NODE* TREE::*;
    friend TYPE                       Get( ROOT_TAG );
};
#pragma GCC diagnostic pop
#define CONFIG( E, D, M )                                                                                              \
    template struct ACCESS<ROOT_TAG<KIRTREE::DYNAMIC_RTREE<ITEM*, E, D, M>>,                                           \
                           &KIRTREE::DYNAMIC_RTREE<ITEM*, E, D, M>::m_root>;
CONFIG( int, 2, 4 )
CONFIG( int, 2, 8 )
CONFIG( int, 2, 16 )
CONFIG( int, 2, 28 )
CONFIG( int, 3, 5 )
CONFIG( int, 3, 16 )
CONFIG( int, 1, 8 )
CONFIG( int64_t, 2, 16 )
CONFIG( int64_t, 3, 8 )
CONFIG( int, 1, 3 )
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
    return { { "level", aNode->level }, { "children", children } };
}

// Guard only arithmetic actually evaluated by the best-first traversal. This is
// validation, not the result: the real NearestNeighbors is called afterwards.
// Widening each expression prevents executing undefined signed arithmetic. Equal
// priorities use the same native priority_queue, so pruned subtrees stay pruned.
template <class E, int D, int M>
void CheckNearest( KIRTREE::RTREE_NODE<ITEM*, E, D, M>* aRoot, const E* aPoint, int aK )
{
    using NODE = KIRTREE::RTREE_NODE<ITEM*, E, D, M>;
    if( !aRoot || aK <= 0 )
        return;
    auto distance = [&]( NODE* node, int slot )
    {
        int64_t dist = 0;
        for( int d = 0; d < D; ++d )
        {
            E        lo = node->bounds[d * 2][slot], hi = node->bounds[d * 2 + 1][slot];
            __int128 diff = 0;
            if( aPoint[d] < lo )
                diff = static_cast<__int128>( lo ) - aPoint[d];
            else if( aPoint[d] > hi )
                diff = static_cast<__int128>( aPoint[d] ) - hi;
            if( diff > INT64_MAX || diff < INT64_MIN )
                Refuse( "signed arithmetic overflow" );
            __int128 square = diff * diff;
            if( square > INT64_MAX || square + dist > INT64_MAX )
                Refuse( "signed arithmetic overflow" );
            dist += static_cast<int64_t>( square );
        }
        return dist;
    };
    using Q = std::pair<int64_t, std::pair<NODE*, int>>;
    auto cmp = []( const Q& a, const Q& b )
    {
        return a.first > b.first;
    };
    std::priority_queue<Q, std::vector<Q>, decltype( cmp )> pq( cmp );
    for( int i = 0; i < aRoot->count; ++i )
        pq.push( { distance( aRoot, i ), { aRoot, i } } );
    int found = 0;
    while( !pq.empty() && found < aK )
    {
        auto [dist, entry] = pq.top();
        pq.pop();
        NODE* node = entry.first;
        if( node->IsLeaf() )
            ++found;
        else
        {
            NODE* child = node->children[entry.second];
            for( int i = 0; i < child->count; ++i )
                pq.push( { distance( child, i ), { child, i } } );
        }
    }
}

// Validate the actual BulkLoad floating conversions without touching the old
// tree. Negative fractions greater than -1 truncate to zero and remain valid.
template <class E, int D, class ENTRY>
void CheckBulk( const std::vector<ENTRY>& entries )
{
    if( entries.empty() )
        return;
    std::array<E, D> lo, hi;
    lo.fill( std::numeric_limits<E>::max() );
    hi.fill( std::numeric_limits<E>::lowest() );
    for( const auto& e : entries )
        for( int d = 0; d < D; ++d )
        {
            lo[d] = std::min( lo[d], e.min[d] );
            hi[d] = std::max( hi[d], e.max[d] );
        }
    for( const auto& e : entries )
        for( int d = 0; d < D; ++d )
        {
            double range = static_cast<double>( hi[d] ) - lo[d];
            if( range <= 0.0 )
                range = 1.0;
            double center = ( static_cast<double>( e.min[d] ) + e.max[d] ) / 2.0;
            double scaled = ( ( center - lo[d] ) / range ) * static_cast<double>( UINT32_MAX );
            if( !std::isfinite( scaled ) || scaled <= -1.0 || scaled >= 4294967296.0 )
                Refuse( "floating conversion outside reference domain" );
        }
}

template <class E, int D, int M>
JSON Program( const JSON& aSteps )
{
    using TREE = KIRTREE::DYNAMIC_RTREE<ITEM*, E, D, M>;
    std::array<ITEM, LIMIT> items;
    for( int i = 0; i < LIMIT; ++i )
        items[i].tag = i;
    std::array<std::unique_ptr<TREE>, 2> trees;
    for( auto& t : trees )
        t = std::make_unique<TREE>();
    JSON events = JSON::array();
    for( const JSON& s : aSteps )
    {
        if( !s.contains( "op" ) || !s["op"].is_string() )
            Refuse( "expected op string" );
        std::string op = s["op"];
        int         a = static_cast<int>( Number( s, "a", 0, 1 ) );
        TREE&       tree = *trees[a];
        JSON        event = JSON::object();
        if( op == "layout" )
        {
            Keys( s, { "op", "a" } );
            using NODE = typename TREE::NODE;
            using PAGE = typename KIRTREE::SLAB_ALLOCATOR<NODE>::PAGE;
            event = { { "node_size", sizeof( NODE ) },     { "node_align", alignof( NODE ) },
                      { "page_size", sizeof( PAGE ) },     { "page_align", alignof( PAGE ) },
                      { "pointer_size", sizeof( NODE* ) }, { "elem_size", sizeof( E ) },
                      { "min_nodes", TREE::MINNODES },     { "reinsert_count", TREE::REINSERT_COUNT } };
        }
        else if( op == "insert" || op == "remove" )
        {
            Keys( s, { "op", "a", "min", "max", "item" } );
            auto  min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            ITEM* item = Item( s["item"], items );
            if( op == "insert" )
            {
                if( tree.size() >= LIMIT )
                    Refuse( "resource limit" );
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
            CheckBulk<E, D>( batch );
            tree.BulkLoad( batch );
            // Observe the actual argument after the call, rather than echo input.
            JSON after = JSON::array();
            for( const auto& b : batch )
                after.push_back( { { "min", b.min }, { "max", b.max }, { "item", WireItem( b.data ) } } );
            event["entries"] = after;
        }
        else if( op == "state" )
        {
            Keys( s, { "op", "a" } );
            event = { { "size", tree.size() },
                      { "empty", tree.empty() },
                      { "memory", tree.MemoryUsage() },
                      { "root", Snapshot( tree.*Get( ROOT_TAG<TREE>{} ) ) } };
        }
        else if( op == "clear" )
        {
            Keys( s, { "op", "a" } );
            tree.RemoveAll();
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
            // Exercise operator-> as well as operator* on the cached value.
            if( *it.operator->() != *it )
                throw std::logic_error( "iterator arrow mismatch" );
        }
        else if( op == "overlap" )
        {
            Keys( s, { "op", "a", "min", "max", "take" } );
            auto min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            int  take = Number( s, "take", 0, LIMIT );
            auto range = tree.Overlapping( min.data(), max.data() );
            auto it = range.begin(), end = range.end(), same = it;
            ++same;
            bool equal = it == same;
            JSON result = JSON::array();
            for( int i = 0; i < take && it != end; ++i, ++it )
                result.push_back( WireItem( *it ) );
            event = { { "items", result },
                      { "empty", range.empty() },
                      { "at_end", it == end },
                      { "current", WireItem( *it ) },
                      { "equal_next", equal } };
        }
        else if( op == "search" )
        {
            Keys( s, { "op", "a", "min", "max", "stop", "seed" } );
            auto        min = Coords<E, D>( s, "min" ), max = Coords<E, D>( s, "max" );
            int         stop = Number( s, "stop", 0, LIMIT );
            const JSON& seed = s["seed"];
            if( !seed.is_array() || seed.size() > LIMIT )
                Refuse( "invalid seed" );
            JSON result = JSON::array();
            for( const JSON& x : seed )
                result.push_back( WireItem( Item( x, items ) ) );
            int  hits = 0;
            auto visitor = [&]( ITEM* item )
            {
                result.push_back( WireItem( item ) );
                return ++hits < stop;
            };
            int found = tree.Search( min.data(), max.data(), visitor );
            event = { { "items", result }, { "found", found } };
        }
        else if( op == "nearest" )
        {
            Keys( s, { "op", "a", "point", "k", "seed" } );
            auto        point = Coords<E, D>( s, "point" );
            int         k = Number( s, "k", INT_MIN, INT_MAX );
            const JSON& seed = s["seed"];
            if( !seed.is_array() || seed.size() > LIMIT )
                Refuse( "invalid seed" );
            std::vector<std::pair<int64_t, ITEM*>> out;
            for( const JSON& x : seed )
                out.push_back( { -7, Item( x, items ) } );
            CheckNearest( tree.*Get( ROOT_TAG<TREE>{} ), point.data(), k );
            tree.NearestNeighbors( point.data(), k, out );
            JSON result = JSON::array();
            for( const auto& [dist, item] : out )
                result.push_back( JSON::array( { dist, WireItem( item ) } ) );
            event["neighbors"] = result;
        }
        else if( op == "move_new" || op == "move_assign" )
        {
            Keys( s, { "op", "a", "src" } );
            int src = Number( s, "src", 0, 1 );
            if( a == src )
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

JSON Run( const JSON& aInput )
{
    Keys( aInput, { "config", "steps" } );
    if( !aInput["config"].is_string() )
        Refuse( "expected config string" );
    std::string c = aInput["config"];
    const JSON& steps = aInput["steps"];
    if( !steps.is_array() || steps.empty() || steps.size() > 4096 )
        Refuse( "steps outside 1..4096" );
    if( c == "i32.d2.m8" )
        return Program<int, 2, 8>( steps );
    if( c == "i32.d2.m4" )
        return Program<int, 2, 4>( steps );
    if( c == "i32.d2.m16" )
        return Program<int, 2, 16>( steps );
    if( c == "i32.d2.m28" )
        return Program<int, 2, 28>( steps );
    if( c == "i32.d3.m5" )
        return Program<int, 3, 5>( steps );
    if( c == "i32.d3.m16" )
        return Program<int, 3, 16>( steps );
    if( c == "i32.d1.m8" )
        return Program<int, 1, 8>( steps );
    if( c == "i64.d2.m16" )
        return Program<int64_t, 2, 16>( steps );
    if( c == "i64.d3.m8" )
        return Program<int64_t, 3, 8>( steps );
    if( c == "i32.d1.m3" )
        return Program<int, 1, 3>( steps );
    Refuse( "unknown config" );
}
} // namespace
namespace oracle
{
void RegisterDynamicRtree( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.geometry.rtree.application.pure.p001",
                   "libs/kimath/include/geometry/rtree/dynamic_rtree.h", Run );
}
} // namespace oracle
