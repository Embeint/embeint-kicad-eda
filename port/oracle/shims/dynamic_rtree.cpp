// SPDX-License-Identifier: GPL-3.0-or-later
// Real calls to the unmodified pinned DYNAMIC_RTREE template. Public generated
// corpus only; see the owning crate README for arithmetic guard qualification.
#include "registry.h"
#include <geometry/rtree/dynamic_rtree.h>
#include <array>
#include <cmath>
#include <memory>
#include <string>
#include <vector>

namespace
{
using oracle::JSON;
struct ITEM { int tag; };
[[noreturn]] void Refuse( const std::string& aMessage ) { throw std::invalid_argument( aMessage ); }
void Keys( const JSON& aValue, std::initializer_list<const char*> aKeys )
{
    if( !aValue.is_object() ) Refuse( "expected object" );
    for( const char* key : aKeys ) if( !aValue.contains( key ) ) Refuse( "unexpected or missing field" );
    for( const auto& [key, value] : aValue.items() )
        if( std::none_of( aKeys.begin(), aKeys.end(), [&]( const char* k ){ return key == k; } ) )
            Refuse( "unexpected or missing field" );
}
const JSON& Array( const JSON& aValue, const char* aName )
{
    if( !aValue.contains( aName ) || !aValue[aName].is_array() ) Refuse( std::string( "expected array: " ) + aName );
    return aValue[aName];
}
std::string String( const JSON& aValue, const char* aName, const char* aMessage )
{
    if( !aValue.contains( aName ) || !aValue[aName].is_string() ) Refuse( aMessage );
    return aValue[aName].get<std::string>();
}
int Int( const JSON& aValue, const char* aName )
{ return oracle::Integer( aValue, aName, INT_MIN, INT_MAX ); }
template <class E, int D>
std::array<E,D> Coords( const JSON& aValue, const char* aName )
{
    const auto& array = aValue.at( aName );
    if( !array.is_array() || array.size() != D ) Refuse( "invalid coordinates" );
    std::array<E,D> result;
    for( int d = 0; d < D; ++d )
    {
        const auto& v = array[d];
        if( !v.is_number_integer() ) Refuse( "invalid coordinates" );
        if( v.is_number_unsigned() && v.get<uint64_t>() > static_cast<uint64_t>( std::numeric_limits<E>::max() ) ) Refuse( "invalid coordinates" );
        const int64_t n = v.get<int64_t>();
        if( n < std::numeric_limits<E>::lowest() || n > std::numeric_limits<E>::max() ) Refuse( "invalid coordinates" );
        result[d] = static_cast<E>( n );
    }
    return result;
}
template<class TREE>
JSON Collect( const TREE& aTree, int aLimit )
{
    JSON result = JSON::array();
    for( auto it = aTree.begin(); it != aTree.end() && ( aLimit < 0 || result.size() < static_cast<size_t>( aLimit ) ); ++it )
        result.push_back( ( *it )->tag );
    return result;
}
template<class E, int D, int F>
JSON RunTree( const JSON& aSteps )
{
    using TREE = KIRTREE::DYNAMIC_RTREE<ITEM*, E, D, F>;
    TREE tree;
    std::vector<ITEM> items( 65536 );
    for( int i = 0; i < 65536; ++i ) items[i].tag = i;
    auto item = [&]( const JSON& s ){ return &items[oracle::Integer( s, "data", 0, 65535 )]; };
    JSON outputs = JSON::array();
    for( const JSON& step : aSteps )
    {
        const std::string op = String( step, "op", "expected op string" );
        JSON value = nullptr;
        if( op == "insert" || op == "remove" )
        {
            Keys( step, { "op", "min", "max", "data" } );
            auto min = Coords<E,D>( step, "min" ), max = Coords<E,D>( step, "max" );
            ITEM* data = item( step );
            if( op == "insert" )
            {
                if( tree.size() >= 65536 ) Refuse( "resource budget exceeded" );
                tree.Insert( min.data(), max.data(), data );
            }
            else value = tree.Remove( min.data(), max.data(), data );
        }
        else if( op == "bulk" )
        {
            Keys( step, { "op", "entries" } );
            const auto& list = Array( step, "entries" );
            if( list.size() > 65536 ) Refuse( "resource budget exceeded" );
            std::vector<typename TREE::BULK_ENTRY> entries;
            for( const auto& v : list )
            {
                Keys( v, { "min", "max", "data" } );
                auto min = Coords<E,D>( v, "min" ), max = Coords<E,D>( v, "max" );
                typename TREE::BULK_ENTRY e{};
                std::copy( min.begin(), min.end(), e.min ); std::copy( max.begin(), max.end(), e.max ); e.data = item( v );
                entries.push_back( e );
            }
            // Exact floating cast domain, evaluated before BulkLoad discards the
            // previous index. Inverted bounds themselves are not globally refused.
            std::array<E,D> lo, hi; lo.fill( std::numeric_limits<E>::max() ); hi.fill( std::numeric_limits<E>::lowest() );
            for( const auto& e : entries ) for( int d=0; d<D; ++d ){ lo[d]=std::min(lo[d],e.min[d]); hi[d]=std::max(hi[d],e.max[d]); }
            for( const auto& e : entries ) for( int d=0; d<D; ++d )
            {
                double range = static_cast<double>(hi[d])-lo[d]; if(range<=0.0) range=1.0;
                double center = (static_cast<double>(e.min[d])+e.max[d])/2.0;
                double v=((center-lo[d])/range)*static_cast<double>(UINT32_MAX);
                if( !std::isfinite(v) || std::trunc(v)<0.0 || std::trunc(v)>=4294967296.0 ) Refuse( "undefined floating integer conversion" );
            }
            tree.BulkLoad( entries );
        }
        else if( op == "clear" ) { Keys( step, { "op" } ); tree.RemoveAll(); }
        else if( op == "iter" ) { Keys( step, { "op", "limit" } ); value=Collect(tree,Int(step,"limit")); }
        else if( op == "state" )
        {
            Keys( step, { "op" } ); auto a=tree.begin(), b=tree.begin(); ++b;
            value={{"all",Collect(tree,-1)},{"begin_equal_next",a==b},{"end",a==tree.end()}};
        }
        else if( op == "query" )
        {
            Keys( step, { "op", "min", "max", "stop" } );
            auto min=Coords<E,D>(step,"min"),max=Coords<E,D>(step,"max");int stop=Int(step,"stop");
            JSON hits=JSON::array(),lazy=JSON::array();
            auto visitor=[&]( ITEM* data ){hits.push_back(data->tag);return stop<=0 || hits.size()<static_cast<size_t>(stop);};
            int found=tree.Search(min.data(),max.data(),visitor);
            auto range=tree.Overlapping(min.data(),max.data());bool empty=range.empty();
            for( ITEM* data : range ) lazy.push_back(data->tag);
            value={{"found",found},{"hits",hits},{"lazy",lazy},{"empty",empty}};
        }
        else if( op == "nearest" )
        {
            Keys( step, { "op", "point", "k" } );auto point=Coords<E,D>(step,"point");int k=Int(step,"k");
            std::vector<std::pair<int64_t,ITEM*>> results={{99,&items[65535]}};
            tree.NearestNeighbors(point.data(),k,results);value=JSON::array();
            for( const auto& [distance,data] : results ) value.push_back({distance,data->tag});
        }
        else if( op == "move" )
        {
            Keys( step, { "op", "kind" } );std::string kind=String(step,"kind","expected move kind");
            if(kind=="self") { TREE& alias=tree;tree=std::move(alias); }
            else
            {
                std::unique_ptr<TREE> dest;
                if(kind=="construct") dest=std::make_unique<TREE>(std::move(tree));
                else if(kind=="assign")
                {
                    dest=std::make_unique<TREE>();std::array<E,D> zero{};
                    dest->Insert(zero.data(),zero.data(),&items[65535]);*dest=std::move(tree);
                }
                else Refuse("unknown move kind");
                value={{"size",tree.size()},{"empty",tree.empty()},{"memory",tree.MemoryUsage()}};tree=std::move(*dest);
            }
        }
        else Refuse( "unknown op" );
        outputs.push_back({{"value",value},{"size",tree.size()},{"empty",tree.empty()},{"memory",tree.MemoryUsage()}});
    }
    return outputs;
}
JSON Run( const JSON& aInput )
{
    Keys( aInput, { "config", "steps" } );
    const std::string config=String(aInput,"config","expected config string");const auto& steps=Array(aInput,"steps");
    if( steps.empty() || steps.size()>4096 ) Refuse( "steps outside 1..4096" );
    if(config=="i32.d2.m16")return RunTree<int,2,16>(steps);
    if(config=="i32.d2.m4")return RunTree<int,2,4>(steps);
    if(config=="i32.d2.m28")return RunTree<int,2,28>(steps);
    if(config=="i32.d3.m3")return RunTree<int,3,3>(steps);
    if(config=="i32.d3.m5")return RunTree<int,3,5>(steps);
    if(config=="i32.d1.m8")return RunTree<int,1,8>(steps);
    if(config=="i64.d2.m16")return RunTree<int64_t,2,16>(steps);
    if(config=="i64.d3.m8")return RunTree<int64_t,3,8>(steps);
    Refuse( "unknown config" );
}
}
namespace oracle
{
void RegisterDynamicRtree( REGISTRY& aRegistry )
{
    aRegistry.Add("libs.kimath.include.geometry.rtree.application.pure.p001","KIRTREE::DYNAMIC_RTREE",Run);
}
}
