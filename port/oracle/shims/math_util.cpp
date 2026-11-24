// SPDX-License-Identifier: GPL-3.0-or-later
// Calls the real pinned kimath util.cpp. No result arithmetic lives in this adapter.
#include "registry.h"
#include <math/util.h>
#include <algorithm>
#include <climits>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <string>

#if !defined( __SIZEOF_INT128__ ) || defined( _MSC_VER )
#error "math util oracle requires the Linux int128 branch"
#endif
static_assert( sizeof( int ) == 4 && sizeof( int64_t ) == 8 && CHAR_BIT == 8 );

namespace
{
using oracle::JSON;
constexpr const char* UNIT = "libs.kimath.src.math.util.application.ui.p001";

void Keys( const JSON& aInput, std::initializer_list<const char*> aFields )
{
    if( !aInput.is_object() )
        throw std::invalid_argument( "expected object" );

    if( aInput.size() != aFields.size() )
        throw std::invalid_argument( "unexpected or missing field" );

    for( const char* field : aFields )
    {
        if( !aInput.contains( field ) )
            throw std::invalid_argument( "unexpected or missing field" );
    }
}

std::string Text( const JSON& aInput, const char* aName )
{
    if( !aInput.contains( aName ) || !aInput.at( aName ).is_string() )
        throw std::invalid_argument( std::string( "expected string: " ) + aName );

    return aInput.at( aName ).get<std::string>();
}

int64_t Signed( const JSON& aInput, const char* aName, int64_t aMin, int64_t aMax )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer()
        || ( value.is_number_unsigned() && value.get<uint64_t>() > uint64_t( aMax ) ) )
    {
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );
    }

    const int64_t number = value.get<int64_t>();

    if( number < aMin || number > aMax )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );

    return number;
}

uint64_t Unsigned( const JSON& aInput, const char* aName )
{
    const JSON& value = aInput.at( aName );

    if( !value.is_number_integer() || ( !value.is_number_unsigned() && value.get<int64_t>() < 0 ) )
        throw std::invalid_argument( std::string( "integer out of range: " ) + aName );

    return value.get<uint64_t>();
}

JSON Run( const JSON& aInput )
{
    const std::string op = Text( aInput, "op" );

    if( op == "hypot_wide" )
    {
        Keys( aInput, { "op", "x", "y" } );
        const uint64_t x = Unsigned( aInput, "x" );
        const uint64_t y = Unsigned( aInput, "y" );
        return JSON{ { "value", RoundedHypotWide( x, y ) } };
    }

    if( op == "rescale_i32" || op == "rescale_i64_int128" )
    {
        Keys( aInput, { "op", "numerator", "value", "denominator" } );
        const bool narrow = op == "rescale_i32";
        const int64_t min = narrow ? std::numeric_limits<int>::min() : INT64_MIN;
        const int64_t max = narrow ? std::numeric_limits<int>::max() : INT64_MAX;
        const int64_t n = Signed( aInput, "numerator", min, max );
        const int64_t v = Signed( aInput, "value", min, max );
        const int64_t d = Signed( aInput, "denominator", min, max );

        // The products and rounding adjustment fit their native intermediate widths.
        // Division by zero is the only undefined arithmetic evaluation on this branch.
        if( d == 0 )
            throw std::invalid_argument( "zero denominator" );

        if( narrow )
            return JSON{ { "value", rescale<int>( int( n ), int( v ), int( d ) ) } };

        return JSON{ { "value", rescale<int64_t>( n, v, d ) } };
    }

    if( op == "log_debug" )
    {
        Keys( aInput, { "op", "format" } );
        (void) Text( aInput, "format" );
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogDebug
        // No wx side effect is simulated or claimed as native evidence.
        throw oracle::UNPORTED( "kimathLogDebug", UNIT );
    }

    if( op == "log_overflow" )
    {
        Keys( aInput, { "op", "bits", "type_name" } );
        const std::string bits = Text( aInput, "bits" );

        if( bits.size() != 16 || bits.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
            throw std::invalid_argument( "invalid hex: bits" );

        (void) Text( aInput, "type_name" );
        // UNPORTED(libs.kimath.src.math.util.application.ui.p001): kimathLogOverflow
        throw oracle::UNPORTED( "kimathLogOverflow", UNIT );
    }

    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterMathUtil( REGISTRY& aRegistry )
{
    aRegistry.Add( UNIT, "libs/kimath/src/math/util.cpp", Run );
}
} // namespace oracle
