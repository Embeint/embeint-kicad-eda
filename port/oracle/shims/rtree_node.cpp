// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for libs.kimath.include.geometry.rtree.application.pure.p003: the
// pinned, unmodified rtree_node.h.  This file validates requests, keeps the inputs
// inside the reference's defined domain, calls the real templates and serialises
// state.  Pointers never reach the output: nodes are numbered by first allocation
// and items by their index in a local array.
#include "registry.h"
#include <geometry/rtree/rtree_node.h>
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <optional>
#include <string>
#include <type_traits>
#include <vector>

static_assert( sizeof( void* ) == 8 && sizeof( int ) == 4 && sizeof( long ) == 8,
               "rtree node layout cases require the pinned Linux x86-64 LP64 contract" );

namespace
{
using oracle::JSON;

constexpr int    MAX_STEPS = 4096;
constexpr size_t MAX_ALLOCATIONS = 4096;
constexpr int    ITEMS = 64;
constexpr int    ALLOCATORS = 4;

// Leaf DATATYPE: a pointer, as in every KiCad instantiation.
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
    {
        if( !aValue.contains( key ) )
            Refuse( "unexpected or missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        if( std::find_if( aRequired.begin(), aRequired.end(),
                          [&]( const char* aKey ) { return key == aKey; } )
            == aRequired.end() )
        {
            Refuse( "unexpected or missing field" );
        }
    }
}


int64_t Signed( const JSON& aObject, const std::string& aName, int64_t aMin, int64_t aMax )
{
    const JSON& value = aObject.at( aName );

    if( !value.is_number_integer() )
        Refuse( "integer out of range: " + aName );

    if( value.is_number_unsigned() )
    {
        const uint64_t number = value.get<uint64_t>();

        if( aMax < 0 || number > static_cast<uint64_t>( aMax )
            || static_cast<int64_t>( number ) < aMin )
        {
            Refuse( "integer out of range: " + aName );
        }

        return static_cast<int64_t>( number );
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        Refuse( "integer out of range: " + aName );

    return number;
}


int Int( const JSON& aObject, const std::string& aName )
{
    return static_cast<int>( Signed( aObject, aName, std::numeric_limits<int>::min(),
                                     std::numeric_limits<int>::max() ) );
}


// A fixed-length array of integers in ELEMTYPE's range.
template <class E>
std::vector<E> Values( const JSON& aObject, const char* aName, size_t aLength )
{
    const JSON& array = aObject.at( aName );

    if( !array.is_array() || array.size() != aLength )
        Refuse( "invalid coordinates" );

    std::vector<E> values;

    for( const JSON& value : array )
    {
        bool ok = value.is_number_integer();

        if( ok && value.is_number_unsigned() )
            ok = value.get<uint64_t>() <= static_cast<uint64_t>( std::numeric_limits<E>::max() );
        else if( ok )
            ok = value.get<int64_t>() >= std::numeric_limits<E>::min()
                 && value.get<int64_t>() <= std::numeric_limits<E>::max();

        if( !ok )
            Refuse( "invalid coordinates" );

        values.push_back( static_cast<E>( value.is_number_unsigned() ? value.get<uint64_t>()
                                                                     : value.get<int64_t>() ) );
    }

    return values;
}


JSON HilbertXy( const JSON& aInput )
{
    Keys( aInput, { "kind", "order", "x", "y" } );
    const int      order = Int( aInput, "order" );
    const uint32_t x = static_cast<uint32_t>( Signed( aInput, "x", 0, UINT32_MAX ) );
    const uint32_t y = static_cast<uint32_t>( Signed( aInput, "y", 0, UINT32_MAX ) );

    // 1U << ( aOrder - 1 ) is defined only for orders 1..32.
    if( order < 1 || order > 32 )
        Refuse( "order outside reference domain" );

    return { { "index", KIRTREE::HilbertXY2D( order, x, y ) } };
}


template <int N>
uint64_t HilbertNd( int aOrder, const std::vector<uint32_t>& aCoords )
{
    uint32_t coords[N];

    // Values() already checked aCoords.size() == N.
    for( int i = 0; i < N; ++i )
        coords[i] = aCoords[i];

    return KIRTREE::HilbertND2D<N>( aOrder, coords );
}


JSON HilbertNdCase( const JSON& aInput )
{
    Keys( aInput, { "kind", "dims", "order", "coords" } );
    const int dims = static_cast<int>( Signed( aInput, "dims", 1, 1024 ) );
    const int order = Int( aInput, "order" );
    const std::vector<uint32_t> coords = Values<uint32_t>( aInput, "coords", dims );

    // HilbertXY2D needs 1..32.  The Morton fallback evaluates bits - 1, which
    // overflows only for INT_MIN, and shifts coordinates by up to aOrder - 1, but
    // only when its loop runs (min( aOrder, 64 / NUMDIMS ) >= 1).
    const bool mortonShifts = dims <= 64 && order >= 1;

    if( ( dims == 2 && ( order < 1 || order > 32 ) )
        || ( dims != 2
             && ( order == std::numeric_limits<int>::min() || ( mortonShifts && order > 32 ) ) ) )
    {
        Refuse( "order outside reference domain" );
    }

    uint64_t index = 0;

    switch( dims )
    {
    case 1: index = HilbertNd<1>( order, coords ); break;
    case 2: index = HilbertNd<2>( order, coords ); break;
    case 3: index = HilbertNd<3>( order, coords ); break;
    case 4: index = HilbertNd<4>( order, coords ); break;
    case 5: index = HilbertNd<5>( order, coords ); break;
    case 6: index = HilbertNd<6>( order, coords ); break;
    case 7: index = HilbertNd<7>( order, coords ); break;
    case 8: index = HilbertNd<8>( order, coords ); break;
    case 9: index = HilbertNd<9>( order, coords ); break;
    case 16: index = HilbertNd<16>( order, coords ); break;
    case 21: index = HilbertNd<21>( order, coords ); break;
    case 22: index = HilbertNd<22>( order, coords ); break;
    case 32: index = HilbertNd<32>( order, coords ); break;
    case 33: index = HilbertNd<33>( order, coords ); break;
    case 64: index = HilbertNd<64>( order, coords ); break;
    case 65: index = HilbertNd<65>( order, coords ); break;
    default: Refuse( "unsupported dimensions" );
    }

    return { { "index", index } };
}


template <class E, int FANOUT>
JSON MaskCase( const JSON& aInput, int aCount )
{
    const std::vector<E> min0 = Values<E>( aInput, "min0", FANOUT );
    const std::vector<E> max0 = Values<E>( aInput, "max0", FANOUT );
    const std::vector<E> min1 = Values<E>( aInput, "min1", FANOUT );
    const std::vector<E> max1 = Values<E>( aInput, "max1", FANOUT );
    const std::vector<E> qmin = Values<E>( aInput, "qmin", 2 );
    const std::vector<E> qmax = Values<E>( aInput, "qmax", 2 );

    // ( 1U << aCount ) - 1 is defined only for counts 0..31.
    if( aCount < 0 || aCount > 31 )
        Refuse( "count outside reference domain" );

    const E queryMin[2] = { qmin[0], qmin[1] };
    const E queryMax[2] = { qmax[0], qmax[1] };
    return { { "mask", KIRTREE::OverlapMask2D<E, FANOUT>( min0.data(), max0.data(), min1.data(),
                                                         max1.data(), aCount, queryMin,
                                                         queryMax ) } };
}


template <class E>
JSON MaskFanout( const JSON& aInput, int aFanout, int aCount )
{
    switch( aFanout )
    {
    case 4: return MaskCase<E, 4>( aInput, aCount );
    case 8: return MaskCase<E, 8>( aInput, aCount );
    case 12: return MaskCase<E, 12>( aInput, aCount );
    case 16: return MaskCase<E, 16>( aInput, aCount );
    case 20: return MaskCase<E, 20>( aInput, aCount );
    case 24: return MaskCase<E, 24>( aInput, aCount );
    case 28: return MaskCase<E, 28>( aInput, aCount );
    default: Refuse( "unsupported fanout" );
    }
}


JSON OverlapMaskCase( const JSON& aInput )
{
    Keys( aInput, { "kind", "elem", "fanout", "count", "min0", "max0", "min1", "max1", "qmin",
                    "qmax" } );
    const JSON& elem = aInput.at( "elem" );
    const int   fanout = static_cast<int>( Signed( aInput, "fanout", 0, 1024 ) );
    const int   count = Int( aInput, "count" );

    if( elem == "i32" )
        return MaskFanout<int>( aInput, fanout, count );
    else if( elem == "i64" )
        return MaskFanout<int64_t>( aInput, fanout, count );

    Refuse( "unknown element type" );
}


// Standard explicit-instantiation member access to the allocator's private state.
template <typename TAG, typename TAG::TYPE MEMBER>
struct ACCESS
{
    friend typename TAG::TYPE Get( TAG ) { return MEMBER; }
};

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wnon-template-friend"
template <class NODE>
struct PAGES_TAG
{
    using ALLOC = KIRTREE::SLAB_ALLOCATOR<NODE>;
    using TYPE = std::vector<std::unique_ptr<typename ALLOC::PAGE>> ALLOC::*;
    friend TYPE Get( PAGES_TAG );
};

template <class NODE>
struct FREE_TAG
{
    using ALLOC = KIRTREE::SLAB_ALLOCATOR<NODE>;
    using TYPE = std::vector<NODE*> ALLOC::*;
    friend TYPE Get( FREE_TAG );
};

template <class NODE>
struct FIND_TAG
{
    using ALLOC = KIRTREE::SLAB_ALLOCATOR<NODE>;
    using TYPE = typename ALLOC::PAGE* ( ALLOC::* )( const NODE*, size_t& ) const;
    friend TYPE Get( FIND_TAG );
};
#pragma GCC diagnostic pop

#define RTREE_CONFIG( E, D, M )                                                                  \
    template struct ACCESS<PAGES_TAG<KIRTREE::RTREE_NODE<ITEM*, E, D, M>>,                       \
                           &KIRTREE::SLAB_ALLOCATOR<KIRTREE::RTREE_NODE<ITEM*, E, D, M>>::m_pages>; \
    template struct ACCESS<FREE_TAG<KIRTREE::RTREE_NODE<ITEM*, E, D, M>>,                        \
                           &KIRTREE::SLAB_ALLOCATOR<                                             \
                                   KIRTREE::RTREE_NODE<ITEM*, E, D, M>>::m_freeList>;            \
    template struct ACCESS<FIND_TAG<KIRTREE::RTREE_NODE<ITEM*, E, D, M>>,                        \
                           &KIRTREE::SLAB_ALLOCATOR<                                             \
                                   KIRTREE::RTREE_NODE<ITEM*, E, D, M>>::findOwningPage>;

RTREE_CONFIG( int, 2, 16 )
RTREE_CONFIG( int, 2, 4 )
RTREE_CONFIG( int, 2, 28 )
RTREE_CONFIG( int, 3, 16 )
RTREE_CONFIG( int, 3, 5 )
RTREE_CONFIG( int, 3, 32 )
RTREE_CONFIG( int, 3, 33 )
RTREE_CONFIG( int, 1, 8 )
RTREE_CONFIG( int64_t, 2, 16 )
RTREE_CONFIG( int64_t, 3, 8 )
#undef RTREE_CONFIG


// Signed 64-bit overflow is undefined in the reference; these mirror its exact
// evaluation order to keep requests inside the defined domain before the call.
template <class E>
bool Extent( E aHi, E aLo, int64_t& aOut )
{
    return !__builtin_sub_overflow( static_cast<int64_t>( aHi ), static_cast<int64_t>( aLo ),
                                    &aOut );
}


template <class E, int D, int M>
class SLAB_PROGRAM
{
public:
    using NODE = KIRTREE::RTREE_NODE<ITEM*, E, D, M>;
    using ALLOC = KIRTREE::SLAB_ALLOCATOR<NODE>;
    using PAGE = typename ALLOC::PAGE;

    JSON Run( const JSON& aSteps )
    {
        JSON events = JSON::array();

        for( const JSON& step : aSteps )
        {
            if( !step.is_object() || !step.contains( "op" ) || !step.at( "op" ).is_string() )
                Refuse( "expected op string" );

            events.push_back( Step( step.at( "op" ).get<std::string>(), step ) );
        }

        return { { "events", events } };
    }

private:
    ALLOC& Allocator( int aId )
    {
        if( !m_allocators[aId] )
            Refuse( "allocator does not exist" );

        return *m_allocators[aId];
    }

    int AllocatorId( const JSON& aStep, const char* aName )
    {
        return static_cast<int>( Signed( aStep, aName, 0, ALLOCATORS - 1 ) );
    }

    NODE* Node( const JSON& aStep, const char* aName )
    {
        const size_t id = static_cast<size_t>( Signed( aStep, aName, 0, MAX_ALLOCATIONS - 1 ) );

        if( id >= m_seen.size() )
            Refuse( "node does not exist" );

        if( !m_seen[id] )
            Refuse( "node is destroyed" );

        return m_seen[id];
    }

    bool IsLive( const NODE* aNode )
    {
        for( const std::optional<ALLOC>& allocator : m_allocators )
        {
            if( allocator && allocator->Owns( aNode ) )
                return Used( *allocator, aNode );
        }

        return false;
    }

    // Move assignment destroys the target's pages.  Their nodes are retired from the
    // numbering, so a later page at a reused address is never mistaken for them.  A
    // live node elsewhere may keep such a pointer: retaining, copying or overwriting
    // its bits is defined, only a dereference is not, and nothing here dereferences
    // it.  Those slots are recorded while the pointers are still valid, so the stale
    // bits are never compared afterwards (a new page may reuse the address).
    void Retire( const ALLOC& aTarget )
    {
        std::vector<size_t> doomed;

        for( size_t i = 0; i < m_seen.size(); ++i )
        {
            if( m_seen[i] && aTarget.Owns( m_seen[i] ) )
                doomed.push_back( i );
        }

        for( size_t i = 0; i < m_seen.size(); ++i )
        {
            const NODE* node = m_seen[i];

            if( !node || aTarget.Owns( node ) || !IsLive( node ) )
                continue;

            for( int slot = 0; slot < M; ++slot )
            {
                if( m_stale[i][slot] >= 0 )
                    continue;

                const void* pointer = nullptr;
                std::memcpy( &pointer, &node->children[slot], sizeof( pointer ) );

                for( size_t id : doomed )
                {
                    if( pointer == static_cast<const void*>( m_seen[id] ) )
                        m_stale[i][slot] = static_cast<int>( id );
                }
            }
        }

        for( size_t id : doomed )
            m_seen[id] = nullptr;
    }

    // The number of a known, undestroyed node.
    size_t IndexOf( const NODE* aNode )
    {
        auto it = std::find( m_seen.begin(), m_seen.end(), const_cast<NODE*>( aNode ) );

        if( it == m_seen.end() )
            throw std::logic_error( "node not numbered" );

        return static_cast<size_t>( it - m_seen.begin() );
    }

    NODE* OptionalNode( const JSON& aStep, const char* aName )
    {
        return aStep.at( aName ).is_null() ? nullptr : Node( aStep, aName );
    }

    int Id( const NODE* aNode )
    {
        auto it = std::find( m_seen.begin(), m_seen.end(), const_cast<NODE*>( aNode ) );

        if( it != m_seen.end() )
            return static_cast<int>( it - m_seen.begin() );

        m_seen.push_back( const_cast<NODE*>( aNode ) );
        m_stale.emplace_back();
        m_stale.back().fill( -1 );
        return static_cast<int>( m_seen.size() - 1 );
    }

    // The page index and slot of aNode in aAllocator, through the private helper.
    std::optional<std::pair<size_t, size_t>> Find( const ALLOC& aAllocator, const NODE* aNode )
    {
        size_t      offset = 0;
        const PAGE* page = ( aAllocator.*Get( FIND_TAG<NODE>{} ) )( aNode, offset );

        if( !page )
            return std::nullopt;

        const auto& pages = aAllocator.*Get( PAGES_TAG<NODE>{} );

        for( size_t i = 0; i < pages.size(); ++i )
        {
            if( pages[i].get() == page )
                return std::make_pair( i, offset );
        }

        throw std::logic_error( "owning page not in page list" );
    }

    bool Used( const ALLOC& aAllocator, const NODE* aNode )
    {
        const auto location = Find( aAllocator, aNode );
        return location
               && ( aAllocator.*Get( PAGES_TAG<NODE>{} ) )[location->first]->used[location->second];
    }

    // A node may only be accessed while its slot is allocated (no use after free).
    NODE* Live( NODE* aNode )
    {
        for( const std::optional<ALLOC>& allocator : m_allocators )
        {
            if( allocator && allocator->Owns( aNode ) )
            {
                if( Used( *allocator, aNode ) )
                    return aNode;

                break;
            }
        }

        Refuse( "node is not live" );
    }

    int Slot( const JSON& aStep )
    {
        return Int( aStep, "slot" );
    }

    void SlotDomain( int aSlot )
    {
        if( aSlot < 0 || aSlot >= M )
            Refuse( "slot outside node" );
    }

    void CountDomain( const NODE* aNode )
    {
        if( aNode->count > M )
            Refuse( "count outside reference domain" );
    }

    static JSON Box( const E aMin[D], const E aMax[D] )
    {
        JSON min = JSON::array(), max = JSON::array();

        for( int d = 0; d < D; ++d )
        {
            min.push_back( aMin[d] );
            max.push_back( aMax[d] );
        }

        return { { "min", min }, { "max", max } };
    }

    struct QUERY
    {
        E min[D];
        E max[D];
    };

    static QUERY Query( const JSON& aStep )
    {
        const std::vector<E> min = Values<E>( aStep, "min", D );
        const std::vector<E> max = Values<E>( aStep, "max", D );
        QUERY query;

        for( int d = 0; d < D; ++d )
        {
            query.min[d] = min[d];
            query.max[d] = max[d];
        }

        return query;
    }

    static void Overflow()
    {
        Refuse( "arithmetic overflow outside reference domain" );
    }

    static void AreaDomain( const NODE* aNode, int aSlot )
    {
        int64_t area = 1, extent = 0;

        for( int d = 0; d < D; ++d )
        {
            if( !Extent( aNode->bounds[d * 2 + 1][aSlot], aNode->bounds[d * 2][aSlot], extent )
                || __builtin_mul_overflow( area, extent, &area ) )
            {
                Overflow();
            }
        }
    }

    static void PerimeterDomain( const NODE* aNode, int aSlot )
    {
        int64_t perimeter = 0, extent = 0, doubled = 0;

        for( int d = 0; d < D; ++d )
        {
            if( !Extent( aNode->bounds[d * 2 + 1][aSlot], aNode->bounds[d * 2][aSlot], extent )
                || __builtin_add_overflow( perimeter, extent, &perimeter ) )
            {
                Overflow();
            }
        }

        if( __builtin_mul_overflow( perimeter, int64_t( 2 ), &doubled ) )
            Overflow();
    }

    static void OverlapAreaDomain( const NODE* aNode, int aSlot, const QUERY& aQuery )
    {
        int64_t overlap = 1, extent = 0;

        for( int d = 0; d < D; ++d )
        {
            const E lo = std::max( aNode->bounds[d * 2][aSlot], aQuery.min[d] );
            const E hi = std::min( aNode->bounds[d * 2 + 1][aSlot], aQuery.max[d] );

            if( lo > hi )
                return;

            if( !Extent( hi, lo, extent ) || __builtin_mul_overflow( overlap, extent, &overlap ) )
                Overflow();
        }
    }

    static void EnlargementDomain( const NODE* aNode, int aSlot, const QUERY& aQuery )
    {
        AreaDomain( aNode, aSlot );
        int64_t original = aNode->ChildArea( aSlot ), enlarged = 1, extent = 0, difference = 0;

        for( int d = 0; d < D; ++d )
        {
            const E lo = std::min( aNode->bounds[d * 2][aSlot], aQuery.min[d] );
            const E hi = std::max( aNode->bounds[d * 2 + 1][aSlot], aQuery.max[d] );

            if( !Extent( hi, lo, extent ) || __builtin_mul_overflow( enlarged, extent, &enlarged ) )
                Overflow();
        }

        if( __builtin_sub_overflow( enlarged, original, &difference ) )
            Overflow();
    }

    JSON Snapshot( const NODE* aNode )
    {
        JSON bounds = JSON::array(), insert = JSON::array(), payload = JSON::array();

        for( int row = 0; row < D * 2; ++row )
        {
            JSON rowBounds = JSON::array(), rowInsert = JSON::array();

            for( int i = 0; i < M; ++i )
            {
                rowBounds.push_back( aNode->bounds[row][i] );
                rowInsert.push_back( aNode->insertBounds[row][i] );
            }

            bounds.push_back( rowBounds );
            insert.push_back( rowInsert );
        }

        for( int i = 0; i < M; ++i )
        {
            // Classify the union's object representation; no address is printed.
            const void* pointer = nullptr;
            std::memcpy( &pointer, &aNode->children[i], sizeof( pointer ) );

            const int destroyed = m_stale[IndexOf( aNode )][i];

            if( destroyed >= 0 )
            {
                // Retained bits of a node destroyed by a move assignment.
                payload.push_back( { { "destroyed", destroyed } } );
                continue;
            }

            int item = -1;

            for( int k = 0; k < ITEMS && item < 0; ++k )
            {
                if( pointer == static_cast<const void*>( &m_items[k] ) )
                    item = k;
            }

            if( !pointer )
            {
                payload.push_back( nullptr );
            }
            else if( item >= 0 )
            {
                payload.push_back( { { "item", item } } );
            }
            else
            {
                auto it = std::find( m_seen.begin(), m_seen.end(), pointer );

                if( it == m_seen.end() )
                    throw std::logic_error( "unknown pointer in node" );

                payload.push_back( { { "child", it - m_seen.begin() } } );
            }
        }

        return { { "count", aNode->count },     { "level", aNode->level },
                 { "bounds", bounds },          { "insert", insert },
                 { "payload", payload },        { "refcount", aNode->refcount.load() } };
    }

    JSON State( const ALLOC& aAllocator )
    {
        JSON pages = JSON::array(), free = JSON::array();

        for( const auto& page : aAllocator.*Get( PAGES_TAG<NODE>{} ) )
        {
            JSON words = JSON::array();

            for( size_t word = 0; word < ALLOC::NODES_PER_PAGE / 64; ++word )
            {
                uint64_t bits = 0;

                for( size_t bit = 0; bit < 64; ++bit )
                {
                    if( page->used[word * 64 + bit] )
                        bits |= uint64_t( 1 ) << bit;
                }

                words.push_back( bits );
            }

            pages.push_back( words );
        }

        const auto& freeList = aAllocator.*Get( FREE_TAG<NODE>{} );

        for( const NODE* node : freeList )
            free.push_back( Id( node ) );

        return { { "pages", pages },
                 { "free", free },
                 { "capacity", freeList.capacity() },
                 { "memory", aAllocator.MemoryUsage() } };
    }

    JSON Step( const std::string& aOp, const JSON& aStep )
    {
        if( aOp == "layout" )
        {
            Keys( aStep, { "op" } );
            return { { "node_size", sizeof( NODE ) },
                     { "node_align", alignof( NODE ) },
                     { "page_size", sizeof( PAGE ) },
                     { "page_align", alignof( PAGE ) },
                     { "nodes_per_page", ALLOC::NODES_PER_PAGE },
                     { "pointer_size", sizeof( NODE* ) },
                     { "min_nodes", NODE::MINNODES } };
        }

        if( aOp == "new" || aOp == "move_new" || aOp == "move_assign" )
        {
            if( aOp == "new" )
                Keys( aStep, { "op", "a" } );
            else
                Keys( aStep, { "op", "a", "src" } );

            const int id = AllocatorId( aStep, "a" );
            const int source = aOp == "new" ? 0 : AllocatorId( aStep, "src" );

            if( aOp == "move_assign" )
            {
                ALLOC& target = Allocator( id );
                ALLOC& from = Allocator( source );

                // The reference's own guard makes a self move a no-op.
                if( id != source )
                    Retire( target );

                target = std::move( from );
                return JSON::object();
            }

            if( m_allocators[id] )
                Refuse( "allocator exists" );

            if( aOp == "new" )
                m_allocators[id].emplace();
            else
                m_allocators[id].emplace( std::move( Allocator( source ) ) );

            return JSON::object();
        }

        if( aOp == "alloc" || aOp == "alloc_many" )
        {
            if( aOp == "alloc" )
                Keys( aStep, { "op", "a" } );
            else
                Keys( aStep, { "op", "a", "n" } );

            const int    id = AllocatorId( aStep, "a" );
            const size_t count = aOp == "alloc" ? 1 : Signed( aStep, "n", 1, 1024 );
            ALLOC&       allocator = Allocator( id );

            if( m_allocated + count > MAX_ALLOCATIONS )
                Refuse( "allocation budget exceeded" );

            JSON nodes = JSON::array();

            for( size_t i = 0; i < count; ++i )
            {
                const int id = Id( allocator.Allocate() );
                // The node is freshly constructed: every child pointer is null.
                m_stale[id].fill( -1 );
                nodes.push_back( id );
                ++m_allocated;
            }

            if( aOp == "alloc" )
                return { { "node", nodes[0] } };

            return { { "nodes", nodes } };
        }

        if( aOp == "free" || aOp == "owns" || aOp == "find" )
        {
            Keys( aStep, { "op", "a", "node" } );
            const int id = AllocatorId( aStep, "a" );
            NODE*     node = aOp == "free" ? OptionalNode( aStep, "node" ) : Node( aStep, "node" );
            ALLOC&    allocator = Allocator( id );

            if( aOp == "owns" )
                return { { "owns", allocator.Owns( node ) } };

            if( aOp == "find" )
            {
                const auto location = Find( allocator, node );

                if( !location )
                    return { { "page", nullptr }, { "offset", nullptr } };

                return { { "page", location->first }, { "offset", location->second } };
            }

            if( node )
            {
                // A foreign node would be destroyed and later re-constructed at an
                // address this allocator does not own; a freed one destroyed twice.
                if( !allocator.Owns( node ) )
                    Refuse( "node is not owned" );

                if( !Used( allocator, node ) )
                    Refuse( "node is not live" );
            }

            allocator.Free( node );
            return JSON::object();
        }

        if( aOp == "memory" || aOp == "state" )
        {
            Keys( aStep, { "op", "a" } );
            ALLOC& allocator = Allocator( AllocatorId( aStep, "a" ) );

            if( aOp == "memory" )
                return { { "memory", allocator.MemoryUsage() } };

            return State( allocator );
        }

        return NodeStep( aOp, aStep );
    }

    JSON NodeStep( const std::string& aOp, const JSON& aStep )
    {
        if( aOp == "set_count" || aOp == "set_level" || aOp == "ref_add" || aOp == "ref_sub"
            || aOp == "ref_store" )
        {
            Keys( aStep, { "op", "node", "value" } );
            NODE*     node = Node( aStep, "node" );
            const int value = Int( aStep, "value" );
            Live( node );

            if( aOp == "set_count" )
                node->count = value;
            else if( aOp == "set_level" )
                node->level = value;
            else if( aOp == "ref_add" )
                return { { "previous", node->refcount.fetch_add( value ) } };
            else if( aOp == "ref_sub" )
                return { { "previous", node->refcount.fetch_sub( value ) } };
            else
                node->refcount.store( value );

            return JSON::object();
        }

        if( aOp == "ref_load" || aOp == "predicates" || aOp == "snapshot" || aOp == "enclosing" )
        {
            Keys( aStep, { "op", "node" } );
            NODE* node = Live( Node( aStep, "node" ) );

            if( aOp == "ref_load" )
                return { { "value", node->refcount.load() } };

            if( aOp == "predicates" )
            {
                return { { "leaf", node->IsLeaf() },
                         { "internal", node->IsInternal() },
                         { "full", node->IsFull() },
                         { "underflow", node->IsUnderflow() } };
            }

            if( aOp == "snapshot" )
                return Snapshot( node );

            CountDomain( node );
            E min[D], max[D];
            node->ComputeEnclosingBounds( min, max );
            return Box( min, max );
        }

        if( aOp == "mask" )
        {
            Keys( aStep, { "op", "node", "min", "max" } );
            NODE*       node = Node( aStep, "node" );
            const QUERY query = Query( aStep );
            Live( node );

            if constexpr( D == 2 )
            {
                if( node->count < 0 || node->count > 31 )
                    Refuse( "count outside reference domain" );
            }
            else
            {
                // ChildOverlaps reads slot i < count; the shift 1U << i runs only for
                // an overlapping slot and is undefined from 32 upwards.
                CountDomain( node );

                for( int i = 32; i < node->count; ++i )
                {
                    if( node->ChildOverlaps( i, query.min, query.max ) )
                        Refuse( "count outside reference domain" );
                }
            }

            return { { "value", node->ChildOverlapMask( query.min, query.max ) } };
        }

        if( aOp == "set_bounds" || aOp == "set_insert" || aOp == "overlaps"
            || aOp == "overlap_area" || aOp == "enlargement" )
        {
            Keys( aStep, { "op", "node", "slot", "min", "max" } );
            NODE*       node = Node( aStep, "node" );
            const int   slot = Slot( aStep );
            const QUERY query = Query( aStep );
            Live( node );
            SlotDomain( slot );

            if( aOp == "set_bounds" )
                node->SetChildBounds( slot, query.min, query.max );
            else if( aOp == "set_insert" )
                node->SetInsertBounds( slot, query.min, query.max );
            else if( aOp == "overlaps" )
                return { { "value", node->ChildOverlaps( slot, query.min, query.max ) } };
            else if( aOp == "overlap_area" )
            {
                OverlapAreaDomain( node, slot, query );
                return { { "value", node->ChildOverlapArea( slot, query.min, query.max ) } };
            }
            else
            {
                EnlargementDomain( node, slot, query );
                return { { "value", node->ChildEnlargement( slot, query.min, query.max ) } };
            }

            return JSON::object();
        }

        if( aOp == "get_bounds" || aOp == "get_insert" || aOp == "area" || aOp == "perimeter"
            || aOp == "remove" )
        {
            Keys( aStep, { "op", "node", "slot" } );
            NODE*     node = Node( aStep, "node" );
            const int slot = Slot( aStep );
            Live( node );

            if( aOp == "remove" )
            {
                // RemoveChild's explicit precondition is assert( i < count ).  When
                // i == count - 1 it touches no array; otherwise slots i and
                // count - 1 are copied and must both lie inside the node.
                if( slot >= node->count )
                    Refuse( "slot outside node" );

                if( slot != node->count - 1 )
                {
                    if( slot < 0 )
                        Refuse( "slot outside node" );

                    CountDomain( node );

                    // The whole slot, including a retained stale pointer, is copied.
                    std::array<int, M>& stale = m_stale[IndexOf( node )];
                    stale[slot] = stale[node->count - 1];
                }

                node->RemoveChild( slot );
                return JSON::object();
            }

            SlotDomain( slot );

            if( aOp == "area" )
            {
                AreaDomain( node, slot );
                return { { "value", node->ChildArea( slot ) } };
            }

            if( aOp == "perimeter" )
            {
                PerimeterDomain( node, slot );
                return { { "value", node->ChildPerimeter( slot ) } };
            }

            E min[D], max[D];

            if( aOp == "get_bounds" )
                node->GetChildBounds( slot, min, max );
            else
                node->GetInsertBounds( slot, min, max );

            return Box( min, max );
        }

        if( aOp == "set_child" || aOp == "set_data" )
        {
            Keys( aStep, { "op", "node", "slot", aOp == "set_child" ? "child" : "item" } );
            NODE*     node = Node( aStep, "node" );
            const int slot = Slot( aStep );
            NODE*     child = nullptr;
            ITEM*     item = nullptr;

            if( aOp == "set_child" )
                child = OptionalNode( aStep, "child" );
            else if( !aStep.at( "item" ).is_null() )
                item = &m_items[Signed( aStep, "item", 0, ITEMS - 1 )];

            Live( node );
            SlotDomain( slot );

            if( aOp == "set_child" )
                node->children[slot] = child;
            else
                node->data[slot] = item;

            m_stale[IndexOf( node )][slot] = -1;

            return JSON::object();
        }

        Refuse( "unknown operation" );
    }

    std::array<ITEM, ITEMS>                         m_items{};
    std::array<std::optional<ALLOC>, ALLOCATORS>    m_allocators;
    std::vector<NODE*>                              m_seen;
    // Per node number and slot: the number of the destroyed node whose pointer the
    // slot retains, or -1.
    std::vector<std::array<int, M>>                 m_stale;
    size_t                                          m_allocated = 0;
};


template <class E, int D, int M>
JSON RunSlab( const JSON& aSteps )
{
    SLAB_PROGRAM<E, D, M> program;
    return program.Run( aSteps );
}


JSON SlabCase( const JSON& aInput )
{
    Keys( aInput, { "kind", "config", "steps" } );
    const JSON& config = aInput.at( "config" );
    const JSON& steps = aInput.at( "steps" );

    if( !config.is_string() )
        Refuse( "unknown config" );

    const std::string name = config.get<std::string>();

    if( name != "i32.d2.m16" && name != "i32.d2.m4" && name != "i32.d2.m28"
        && name != "i32.d3.m16" && name != "i32.d3.m5" && name != "i32.d3.m32"
        && name != "i32.d3.m33"
        && name != "i32.d1.m8" && name != "i64.d2.m16" && name != "i64.d3.m8" )
    {
        Refuse( "unknown config" );
    }

    if( !steps.is_array() )
        Refuse( "expected steps array" );

    if( steps.empty() || steps.size() > MAX_STEPS )
        Refuse( "steps outside 1..4096" );

    if( name == "i32.d2.m16" ) return RunSlab<int, 2, 16>( steps );
    if( name == "i32.d2.m4" ) return RunSlab<int, 2, 4>( steps );
    if( name == "i32.d2.m28" ) return RunSlab<int, 2, 28>( steps );
    if( name == "i32.d3.m16" ) return RunSlab<int, 3, 16>( steps );
    if( name == "i32.d3.m5" ) return RunSlab<int, 3, 5>( steps );
    if( name == "i32.d3.m32" ) return RunSlab<int, 3, 32>( steps );
    if( name == "i32.d3.m33" ) return RunSlab<int, 3, 33>( steps );
    if( name == "i32.d1.m8" ) return RunSlab<int, 1, 8>( steps );
    if( name == "i64.d2.m16" ) return RunSlab<int64_t, 2, 16>( steps );
    return RunSlab<int64_t, 3, 8>( steps );
}


JSON RtreeNode( const JSON& aInput )
{
    if( !aInput.contains( "kind" ) || !aInput.at( "kind" ).is_string() )
        Refuse( "expected kind string" );

    const std::string kind = aInput.at( "kind" ).get<std::string>();

    if( kind == "hilbert_xy" )
        return HilbertXy( aInput );
    if( kind == "hilbert_nd" )
        return HilbertNdCase( aInput );
    if( kind == "overlap_mask" )
        return OverlapMaskCase( aInput );
    if( kind == "slab" )
        return SlabCase( aInput );

    Refuse( "unknown kind" );
}
} // namespace


namespace oracle
{
void RegisterRtreeNode( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.geometry.rtree.application.pure.p003",
                   "KIRTREE::RTREE_NODE, SLAB_ALLOCATOR, HilbertND2D, OverlapMask2D",
                   RtreeNode );
}
} // namespace oracle
