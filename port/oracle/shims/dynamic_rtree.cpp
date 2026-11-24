// SPDX-License-Identifier: GPL-3.0-or-later
// Real calls to the unmodified pinned DYNAMIC_RTREE template. Public generated
// corpus only; see the owning crate README for arithmetic guard qualification.
#include "registry.h"
#include <geometry/rtree/dynamic_rtree.h>
#include <array>
#include <cmath>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
struct ITEM
{
    int Tag;
};
[[noreturn]] void refuse( const std::string& aMessage )
{
    throw std::invalid_argument( aMessage );
}
void keys( const JSON& aValue, std::initializer_list<const char*> aKeys )
{
    if( !aValue.is_object() )
        refuse( "expected object" );
    for( const char* key : aKeys )
        if( !aValue.contains( key ) )
            refuse( "unexpected or missing field" );
    for( const auto& [key, value] : aValue.items() )
        if( std::none_of( aKeys.begin(), aKeys.end(),
                          [&]( const char* aKey )
                          {
                              return key == aKey;
                          } ) )
            refuse( "unexpected or missing field" );
}
const JSON& array( const JSON& aValue, const char* aName )
{
    if( !aValue.contains( aName ) || !aValue[aName].is_array() )
        refuse( std::string( "expected array: " ) + aName );
    return aValue[aName];
}
std::string stringValue( const JSON& aValue, const char* aName, const char* aMessage )
{
    if( !aValue.contains( aName ) || !aValue[aName].is_string() )
        refuse( aMessage );
    return aValue[aName].get<std::string>();
}
int intValue( const JSON& aValue, const char* aName )
{
    return oracle::Integer( aValue, aName, INT_MIN, INT_MAX );
}
template <class E, int D>
std::array<E, D> coords( const JSON& aValue, const char* aName )
{
    const JSON& values = aValue.at( aName );
    if( !values.is_array() || values.size() != D )
        refuse( "invalid coordinates" );
    std::array<E, D> result;
    for( int d = 0; d < D; ++d )
    {
        const JSON& v = values[d];
        if( !v.is_number_integer() )
            refuse( "invalid coordinates" );
        if( v.is_number_unsigned() && v.get<uint64_t>() > static_cast<uint64_t>( std::numeric_limits<E>::max() ) )
            refuse( "invalid coordinates" );
        const int64_t n = v.get<int64_t>();
        if( n < std::numeric_limits<E>::lowest() || n > std::numeric_limits<E>::max() )
            refuse( "invalid coordinates" );
        result[d] = static_cast<E>( n );
    }
    return result;
}
template <class TREE>
JSON collect( const TREE& aTree, int aLimit )
{
    JSON result = JSON::array();
    for( auto it = aTree.begin(); it != aTree.end() && ( aLimit < 0 || result.size() < static_cast<size_t>( aLimit ) );
         ++it )
        result.push_back( ( *it ) ? ( *it )->Tag : -1 );
    return result;
}
template <class CURSOR>
JSON cursorValue( const CURSOR& aCursor, const JSON& aArrowEqual )
{
    const CURSOR end;
    ITEM*        current = *aCursor;
    return { { "end", aCursor == end }, { "current", current ? current->Tag : -1 }, { "arrow_equal", aArrowEqual } };
}


template <class E, int D, int F>
JSON runTree( const JSON& aSteps )
{
    using TREE = KIRTREE::DYNAMIC_RTREE<ITEM*, E, D, F>;
    TREE                                      tree;
    typename TREE::Iterator                   allCursor;
    typename TREE::SearchIterator             queryCursor;
    std::optional<typename TREE::SearchRange> savedRange;
    std::vector<ITEM>                         items( 65536 );
    for( int i = 0; i < 65536; ++i )
        items[i].Tag = i;
    auto item = [&]( const JSON& aStep )
    {
        int id = oracle::Integer( aStep, "data", -1, 65535 );
        return id < 0 ? nullptr : &items[id];
    };
    JSON outputs = JSON::array();
    for( const JSON& step : aSteps )
    {
        const std::string op = stringValue( step, "op", "expected op string" );
        JSON              value = nullptr;
        if( op == "range_save" )
        {
            keys( step, { "op", "min", "max" } );
            std::array<E, D> min = coords<E, D>( step, "min" ), max = coords<E, D>( step, "max" );
            savedRange = tree.Overlapping( min.data(), max.data() );
        }
        else if( op == "range_begin" )
        {
            keys( step, { "op" } );
            if( !savedRange )
                refuse( "range not saved" );
            queryCursor = savedRange->begin();
            value = cursorValue( queryCursor, nullptr );
        }
        else if( op == "cursor_all" )
        {
            keys( step, { "op" } );
            allCursor = tree.begin();
            value = cursorValue( allCursor, allCursor.operator->() == &*allCursor );
        }
        else if( op == "cursor_query" )
        {
            keys( step, { "op", "min", "max" } );
            std::array<E, D> min = coords<E, D>( step, "min" ), max = coords<E, D>( step, "max" );
            queryCursor = tree.Overlapping( min.data(), max.data() ).begin();
            value = cursorValue( queryCursor, nullptr );
        }
        else if( op == "cursor_next" || op == "cursor_value" )
        {
            keys( step, { "op", "kind" } );
            const std::string kind = stringValue( step, "kind", "expected cursor kind" );
            if( kind == "all" )
            {
                if( op == "cursor_next" )
                    ++allCursor;
                value = cursorValue( allCursor, allCursor.operator->() == &*allCursor );
            }
            else if( kind == "query" )
            {
                if( op == "cursor_next" )
                    ++queryCursor;
                value = cursorValue( queryCursor, nullptr );
            }
            else
                refuse( "unknown cursor kind" );
        }
        else if( op == "insert" || op == "remove" )
        {
            keys( step, { "op", "min", "max", "data" } );
            std::array<E, D> min = coords<E, D>( step, "min" ), max = coords<E, D>( step, "max" );
            ITEM*            data = item( step );
            if( op == "insert" )
            {
                if( tree.size() >= 65536 )
                    refuse( "resource budget exceeded" );
                tree.Insert( min.data(), max.data(), data );
            }
            else
                value = tree.Remove( min.data(), max.data(), data );
        }
        else if( op == "bulk" )
        {
            keys( step, { "op", "entries" } );
            const JSON& list = array( step, "entries" );
            if( list.size() > 65536 )
                refuse( "resource budget exceeded" );
            std::vector<typename TREE::BULK_ENTRY> entries;
            for( const JSON& v : list )
            {
                keys( v, { "min", "max", "data" } );
                std::array<E, D>          min = coords<E, D>( v, "min" ), max = coords<E, D>( v, "max" );
                typename TREE::BULK_ENTRY e{};
                std::copy( min.begin(), min.end(), e.min );
                std::copy( max.begin(), max.end(), e.max );
                e.data = item( v );
                entries.push_back( e );
            }
            // Exact floating cast domain, evaluated before BulkLoad discards the
            // previous index. Inverted bounds themselves are not globally refused.
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
                    double v = ( ( center - lo[d] ) / range ) * static_cast<double>( UINT32_MAX );
                    if( !std::isfinite( v ) || std::trunc( v ) < 0.0 || std::trunc( v ) >= 4294967296.0 )
                        refuse( "undefined floating integer conversion" );
                }
            tree.BulkLoad( entries );
        }
        else if( op == "clear" )
        {
            keys( step, { "op" } );
            tree.RemoveAll();
        }
        else if( op == "iter" )
        {
            keys( step, { "op", "limit" } );
            value = collect( tree, intValue( step, "limit" ) );
        }
        else if( op == "state" )
        {
            keys( step, { "op" } );
            auto a = tree.begin(), b = tree.begin();
            ++b;
            value = { { "all", collect( tree, -1 ) }, { "begin_equal_next", a == b }, { "end", a == tree.end() } };
        }
        else if( op == "query" )
        {
            keys( step, { "op", "min", "max", "stop" } );
            std::array<E, D> min = coords<E, D>( step, "min" ), max = coords<E, D>( step, "max" );
            int              stop = intValue( step, "stop" );
            JSON             hits = JSON::array(), lazy = JSON::array();
            auto             visitor = [&]( ITEM* aData )
            {
                hits.push_back( aData ? aData->Tag : -1 );
                return stop <= 0 || hits.size() < static_cast<size_t>( stop );
            };
            int  found = tree.Search( min.data(), max.data(), visitor );
            auto range = tree.Overlapping( min.data(), max.data() );
            bool empty = range.empty();
            for( ITEM* data : range )
                lazy.push_back( data ? data->Tag : -1 );
            value = { { "found", found }, { "hits", hits }, { "lazy", lazy }, { "empty", empty } };
        }
        else if( op == "nearest" )
        {
            keys( step, { "op", "point", "k" } );
            std::array<E, D>                       point = coords<E, D>( step, "point" );
            int                                    k = intValue( step, "k" );
            std::vector<std::pair<int64_t, ITEM*>> results = { { 99, &items[65535] } };
            tree.NearestNeighbors( point.data(), k, results );
            value = JSON::array();
            for( const auto& [distance, data] : results )
                value.push_back( { distance, data ? data->Tag : -1 } );
        }
        else if( op == "move" )
        {
            keys( step, { "op", "kind" } );
            std::string kind = stringValue( step, "kind", "expected move kind" );
            if( kind == "self" )
            {
                TREE& alias = tree;
                tree = std::move( alias );
            }
            else
            {
                std::unique_ptr<TREE> dest;
                if( kind == "construct" )
                    dest = std::make_unique<TREE>( std::move( tree ) );
                else if( kind == "assign" )
                {
                    dest = std::make_unique<TREE>();
                    std::array<E, D> zero{};
                    dest->Insert( zero.data(), zero.data(), &items[65535] );
                    *dest = std::move( tree );
                }
                else
                    refuse( "unknown move kind" );
                value = { { "size", tree.size() }, { "empty", tree.empty() }, { "memory", tree.MemoryUsage() } };
                tree = std::move( *dest );
            }
        }
        else
            refuse( "unknown op" );
        outputs.push_back( { { "value", value },
                             { "size", tree.size() },
                             { "empty", tree.empty() },
                             { "memory", tree.MemoryUsage() } } );
    }
    return outputs;
}
JSON run( const JSON& aInput )
{
    keys( aInput, { "config", "steps" } );
    const std::string config = stringValue( aInput, "config", "expected config string" );
    const auto&       steps = array( aInput, "steps" );
    if( steps.empty() || steps.size() > 4096 )
        refuse( "steps outside 1..4096" );
    if( config == "i32.d2.m16" )
        return runTree<int, 2, 16>( steps );
    if( config == "i32.d2.m4" )
        return runTree<int, 2, 4>( steps );
    if( config == "i32.d2.m28" )
        return runTree<int, 2, 28>( steps );
    if( config == "i32.d3.m3" )
        return runTree<int, 3, 3>( steps );
    if( config == "i32.d3.m5" )
        return runTree<int, 3, 5>( steps );
    if( config == "i32.d1.m8" )
        return runTree<int, 1, 8>( steps );
    if( config == "i64.d2.m16" )
        return runTree<int64_t, 2, 16>( steps );
    if( config == "i64.d3.m8" )
        return runTree<int64_t, 3, 8>( steps );
    refuse( "unknown config" );
}
} // namespace
namespace oracle
{
void RegisterDynamicRtree( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.geometry.rtree.application.pure.p001", "KIRTREE::DYNAMIC_RTREE", run );
}
} // namespace oracle
