// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned POISSON_DISK::ToroidalUnitTile (libs/kimath/src/geometry/poisson_disk.cpp) and
// the pinned Boost.Random distributions it instantiates. The adapter decodes inputs, applies the
// documented resource and undefined-domain refusals, calls the reference and encodes every
// sample as exact IEEE bits; it never computes a sample itself.
#include "registry.h"
#include <geometry/poisson_disk.h>
#include <boost/random/mersenne_twister.hpp>
#include <boost/random/uniform_int_distribution.hpp>
#include <boost/random/uniform_real_distribution.hpp>
#include <algorithm>
#include <bit>
#include <charconv>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace
{
using oracle::JSON;
constexpr const char* POISSON_UNIT = "libs.kimath.src.geometry.poisson_disk.application.pure.p001";

// Harness bounds on the caller's storage limits and on direct draws per request. They bound
// the adapter's memory and reply size; the port's own contract allows limits up to INT_MAX.
constexpr int      MAX_LIMIT = 4194304;
constexpr size_t   MAX_DRAWS = 65536;
constexpr size_t   MAX_SPECS = 64;
constexpr size_t   EDGE_SAMPLES = 8;


void checkKeys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );
    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );
    for( const char* field : aFields )
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
}


std::string readText( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );
    return aInput.at( aName ).get<std::string>();
}


// Exactly 16 lower-case hex digits: the IEEE binary64 bits.
double readBits( const JSON& aInput, const char* aName )
{
    const std::string text = readText( aInput, aName );
    uint64_t          value = 0;

    if( text.size() != 16
        || !std::all_of( text.begin(), text.end(),
                         []( char c ) { return ( c >= '0' && c <= '9' ) || ( c >= 'a' && c <= 'f' ); } ) )
        throw std::invalid_argument( std::string( "expected bits: " ) + aName );

    std::from_chars( text.data(), text.data() + text.size(), value, 16 );
    return std::bit_cast<double>( value );
}


// A canonical unsigned decimal string (no sign, no leading zero) within uint64_t.
uint64_t readDecimal( const JSON& aInput, const char* aName )
{
    const std::string text = readText( aInput, aName );
    uint64_t          value = 0;

    if( text.empty() || ( text.size() > 1 && text[0] == '0' )
        || !std::all_of( text.begin(), text.end(), []( char c ) { return c >= '0' && c <= '9'; } ) )
        throw std::invalid_argument( std::string( "expected decimal: " ) + aName );

    auto [end, ec] = std::from_chars( text.data(), text.data() + text.size(), value, 10 );

    if( ec != std::errc() || end != text.data() + text.size() )
        throw std::invalid_argument( std::string( "expected decimal: " ) + aName );

    return value;
}


uint32_t readSeed( const JSON& aInput )
{
    const JSON& value = aInput.at( "seed" );

    if( !value.is_number_unsigned() || value.get<uint64_t>() > UINT32_MAX )
        throw std::invalid_argument( "seed out of range" );

    return static_cast<uint32_t>( value.get<uint64_t>() );
}


std::string bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON point( const VECTOR2D& aValue )
{
    return JSON::array( { bits( aValue.x ), bits( aValue.y ) } );
}


// FNV-1a 64 over each sample's x then y bits, little-endian bytes, in result order.
std::string digest( const std::vector<VECTOR2D>& aSamples )
{
    uint64_t hash = 0xcbf29ce484222325ULL;

    for( const VECTOR2D& p : aSamples )
    {
        for( double coordinate : { p.x, p.y } )
        {
            uint64_t word = std::bit_cast<uint64_t>( coordinate );

            for( int i = 0; i < 8; ++i )
            {
                hash ^= ( word >> ( 8 * i ) ) & 0xff;
                hash *= 0x100000001b3ULL;
            }
        }
    }

    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << hash;
    return s.str();
}


JSON tile( const JSON& aInput )
{
    checkKeys( aInput, { "op", "min_dist", "seed", "max_grid_cells", "max_samples", "emit" } );
    const double      minDist = readBits( aInput, "min_dist" );
    const uint32_t    seed = readSeed( aInput );
    const int         maxCells = oracle::Integer( aInput, "max_grid_cells", 0, MAX_LIMIT );
    const int         maxSamples = oracle::Integer( aInput, "max_samples", 0, MAX_LIMIT );
    const std::string emit = readText( aInput, "emit" );

    if( emit != "samples" && emit != "digest" )
        throw std::invalid_argument( "unknown emit" );

    // Outside the early-return domain the reference converts ceil( sqrt( 2 ) / aMinDist ) to
    // int, which is undefined unless the value fits (never for NaN, which passes the guard).
    // The adapter evaluates the same expressions to refuse before the call; it does not
    // substitute any result.
    if( !( minDist <= 0.0 || minDist > 0.5 ) )
    {
        const double cellsPerSide = std::ceil( std::sqrt( 2.0 ) / minDist );

        if( !( cellsPerSide < 2147483648.0 ) )
            throw std::invalid_argument( "undefined grid size" );

        const int      gridN = std::max( 3, (int) cellsPerSide );
        const uint64_t cells = (uint64_t) gridN * (uint64_t) gridN;

        if( cells > (uint64_t) maxCells )
            throw std::invalid_argument( "grid limit" );
    }

    const std::vector<VECTOR2D> samples = POISSON_DISK::ToroidalUnitTile( minDist, seed );

    // The port stops at the limit; the reference finishes, and the adapter refuses the same
    // request from its count. Neither reports a partial tile.
    if( samples.size() > (size_t) maxSamples )
        throw std::invalid_argument( "sample limit" );

    JSON out = JSON::object();
    out["count"] = samples.size();

    if( emit == "samples" )
    {
        JSON list = JSON::array();

        for( const VECTOR2D& p : samples )
            list.push_back( point( p ) );

        out["samples"] = list;
    }
    else
    {
        JSON first = JSON::array();
        JSON last = JSON::array();
        size_t edge = std::min( EDGE_SAMPLES, samples.size() );

        for( size_t i = 0; i < edge; ++i )
            first.push_back( point( samples[i] ) );

        for( size_t i = samples.size() - edge; i < samples.size(); ++i )
            last.push_back( point( samples[i] ) );

        out["digest"] = digest( samples );
        out["first"] = first;
        out["last"] = last;
    }

    return out;
}


// Direct draws from one engine, in order, through the pinned Boost templates instantiated as in
// ToroidalUnitTile: mt19937::operator(), uniform_int_distribution<size_t> and
// uniform_real_distribution<double>.
JSON rng( const JSON& aInput )
{
    checkKeys( aInput, { "op", "seed", "draws" } );
    boost::random::mt19937 engine( readSeed( aInput ) );
    const JSON&            specs = aInput.at( "draws" );

    if( !specs.is_array() || specs.empty() || specs.size() > MAX_SPECS )
        throw std::invalid_argument( "draws out of range" );

    size_t total = 0;

    for( const JSON& spec : specs )
    {
        if( !spec.is_object() || !spec.contains( "kind" ) || !spec.contains( "count" ) )
            throw std::invalid_argument( "expected draw" );

        const std::string kind = readText( spec, "kind" );

        if( kind == "u32" )
            checkKeys( spec, { "kind", "count" } );
        else if( kind == "int" || kind == "real" )
            checkKeys( spec, { "kind", "count", "min", "max" } );
        else
            throw std::invalid_argument( "unknown draw" );

        total += oracle::Integer( spec, "count", 1, (int) MAX_DRAWS );

        if( kind == "int" )
        {
            // Sequenced reads, so a request with two malformed bounds names min.
            const uint64_t lo = readDecimal( spec, "min" );
            const uint64_t hi = readDecimal( spec, "max" );

            if( lo > hi )
                throw std::invalid_argument( "invalid int range" );
        }

        if( kind == "real" )
        {
            const double lo = readBits( spec, "min" );
            const double hi = readBits( spec, "max" );

            // min == max never terminates, NaN never compares and an infinite bound makes the
            // halving guard recurse without end; only finite min < max is a defined draw.
            if( !( std::isfinite( lo ) && std::isfinite( hi ) && lo < hi ) )
                throw std::invalid_argument( "invalid real range" );
        }
    }

    if( total > MAX_DRAWS )
        throw std::invalid_argument( "draws out of range" );

    JSON out = JSON::array();

    for( const JSON& spec : specs )
    {
        const std::string kind = readText( spec, "kind" );
        const int         count = oracle::Integer( spec, "count", 1, (int) MAX_DRAWS );
        JSON              values = JSON::array();

        if( kind == "u32" )
        {
            for( int i = 0; i < count; ++i )
                values.push_back( (uint64_t) engine() );
        }
        else if( kind == "int" )
        {
            boost::random::uniform_int_distribution<size_t> dist( readDecimal( spec, "min" ),
                                                                  readDecimal( spec, "max" ) );

            for( int i = 0; i < count; ++i )
                values.push_back( std::to_string( (uint64_t) dist( engine ) ) );
        }
        else
        {
            boost::random::uniform_real_distribution<double> dist( readBits( spec, "min" ),
                                                                   readBits( spec, "max" ) );

            for( int i = 0; i < count; ++i )
                values.push_back( bits( dist( engine ) ) );
        }

        out.push_back( values );
    }

    return out;
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );

    if( op == "tile" )
        return tile( aInput );
    if( op == "rng" )
        return rng( aInput );
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterPoissonDisk( REGISTRY& aRegistry )
{
    aRegistry.Add( POISSON_UNIT, "geometry/poisson_disk.cpp", run );
}
} // namespace oracle
