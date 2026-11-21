// SPDX-License-Identifier: GPL-3.0-or-later
#include "registry.h"
#include <math/util.h>

namespace oracle
{
static JSON Rescale( const JSON& aInput )
{
    if( aInput.size() != 3 )
        throw std::invalid_argument( "expected numerator, value and denominator" );

    const int numerator = Integer( aInput, "numerator", -1000000, 1000000 );
    const int value = Integer( aInput, "value", -1000000, 1000000 );
    const int denominator = Integer( aInput, "denominator", -1000000, 1000000 );

    // Bound the specialization's final narrowing conversion as well as its divisor.
    if( denominator == 0
        || std::abs( static_cast<int64_t>( numerator ) * value ) > 2000000000 )
    {
        throw std::invalid_argument( "zero divisor or product outside example domain" );
    }

    return rescale<int>( numerator, value, denominator );
}


void RegisterExample( REGISTRY& aRegistry )
{
    aRegistry.Add( "example.kimath-rescale", "libs/kimath/src/math/util.cpp:rescale<int>", Rescale );
}
} // namespace oracle
