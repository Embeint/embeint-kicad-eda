// SPDX-License-Identifier: GPL-3.0-or-later
// Compile the original translation unit in this library TU, unchanged, so its
// file-local helpers and SolutionCollector definition are available to test
// accessors. CMake/audit pin and verify both this TU and the included source.
#include <resistor_substitution_utils.cpp>
#include "resistors_access.h"

namespace
{
template<typename Tag, typename Tag::Type Member>
struct Access
{
    friend typename Tag::Type Get( Tag ) { return Member; }
};
struct UniqueTag
{
    using Type = int ( * )( const RESISTANCE& );
    friend Type Get( UniqueTag );
};
struct BetterTag
{
    using Type = bool ( * )( const RESISTANCE&, const RESISTANCE& );
    friend Type Get( BetterTag );
};
template struct Access<UniqueTag, &SolutionCollector::uniqueCount>;
template struct Access<BetterTag, &SolutionCollector::betterCandidate>;
} // namespace

namespace resistor_test
{
COLLECTED Collect( double target, std::vector<RESISTANCE>& candidates )
{
    SolutionCollector collector( target );
    size_t callbacks = 0;
    for( auto& candidate : candidates )
    {
        collector.Add2RLookupResults( { candidate, candidate }, []( double value ) { return value; },
                [&]( RESISTANCE& value ) { callbacks++; return value; } );
    }
    return { collector.GetBest(), callbacks };
}

int Unique( const RESISTANCE& value ) { return Get( UniqueTag{} )( value ); }
bool Better( const RESISTANCE& candidate, const RESISTANCE& best ) { return Get( BetterTag{} )( candidate, best ); }
std::string Embrace( const std::string& text, char symbol ) { return maybeEmbrace( text, symbol ); }
RESISTANCE Combine( const RESISTANCE& first, const RESISTANCE& second, bool parallel, bool simple )
{
    if( parallel ) return simple ? parallelResistanceSimple( first, second ) : parallelResistance( first, second );
    return simple ? serialResistanceSimple( first, second ) : serialResistance( first, second );
}
bool Less( const RESISTANCE& first, const RESISTANCE& second ) { return first < second; }
bool Less( const RESISTANCE& first, double second ) { return first < second; }
} // namespace resistor_test
