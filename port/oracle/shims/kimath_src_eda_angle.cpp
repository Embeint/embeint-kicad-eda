// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned out-of-line EDA_ANGLE bodies (libs/kimath/src/geometry/eda_angle.cpp).
// The adapter only decodes exact IEEE bit patterns and encodes the returned values.
#include "registry.h"
#include <geometry/eda_angle.h>
#include <bit>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace
{
using oracle::JSON;
constexpr const char* EDA_ANGLE_SOURCE_UNIT = "libs.kimath.src.geometry.eda_angle.application.pure.p001";


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


// Doubles travel as 16 lower-case hex digits of their IEEE bits, so NaN payloads,
// infinities and -0 reach the reference exactly.
double readNumber( const JSON& aInput, const char* aName )
{
    const std::string text = readText( aInput, aName );
    if( text.size() != 16 || text.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
        throw std::invalid_argument( std::string( "invalid hex: " ) + aName );
    return std::bit_cast<double>( std::stoull( text, nullptr, 16 ) );
}


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON run( const JSON& aInput )
{
    const std::string op = readText( aInput, "op" );
    checkKeys( aInput, { "op", "a" } );
    const EDA_ANGLE angle( readNumber( aInput, "a" ), DEGREES_T );

    if( op == "keep_upright" )
        return bits( angle.KeepUpright().AsDegrees() );
    if( op == "is_cardinal" )
        return angle.IsCardinal();
    if( op == "is_cardinal90" )
        return angle.IsCardinal90();
    throw std::invalid_argument( "unknown operation" );
}
} // namespace

namespace oracle
{
void RegisterEdaAngleSource( REGISTRY& aRegistry )
{
    aRegistry.Add( EDA_ANGLE_SOURCE_UNIT, "geometry/eda_angle.cpp", run );
}
} // namespace oracle
