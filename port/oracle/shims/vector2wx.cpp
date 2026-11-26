// SPDX-License-Identifier: GPL-3.0-or-later
// Genuine pinned libs/kimath/include/math/vector2wx.h conversions on the distribution
// wxWidgets wxPoint/wxSize. The adapter only decodes one 32-bit integer pair (or selects a
// default-constructed source), calls the pinned inline conversion and encodes the
// components: integers as JSON integers, doubles as exact IEEE bits. It never computes a
// reply itself.
#include "registry.h"
#include <math/vector2wx.h>
#include <bit>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <iomanip>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>
#include <type_traits>

namespace
{
using oracle::JSON;
constexpr const char* VECTOR2WX_UNIT = "libs.kimath.include.math.application.thin.replacement.p001";

// The Rust stand-ins (kicad_math::vector2wx::WxPoint/WxSize) model exactly this layout.
static_assert( std::is_same_v<decltype( wxPoint::x ), int> && std::is_same_v<decltype( wxPoint::y ), int> );
static_assert( std::is_same_v<decltype( wxSize::x ), int> && std::is_same_v<decltype( wxSize::y ), int> );
static_assert( sizeof( wxPoint ) == 2 * sizeof( int ) && offsetof( wxPoint, y ) == sizeof( int ) );
static_assert( sizeof( wxSize ) == 2 * sizeof( int ) && offsetof( wxSize, y ) == sizeof( int ) );
static_assert( std::is_same_v<VECTOR2I::coord_type, int32_t> && sizeof( int ) == sizeof( int32_t ) );
static_assert( std::is_same_v<decltype( ToVECTOR2D( wxPoint() ) ), VECTOR2D> );


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


JSON bits( double aValue )
{
    std::ostringstream s;
    s.imbue( std::locale::classic() );
    s << std::hex << std::setw( 16 ) << std::setfill( '0' ) << std::bit_cast<uint64_t>( aValue );
    return s.str();
}


JSON integer( int aX, int aY )
{
    return JSON{ { "x", aX }, { "y", aY } };
}


JSON vector2d( const VECTOR2D& aValue )
{
    return JSON{ { "x", bits( aValue.x ) }, { "y", bits( aValue.y ) } };
}


// `null` selects a default-constructed source; otherwise exactly two JSON integers in
// the 32-bit int range. Returns false for the default source.
bool readPair( const JSON& aInput, int& aX, int& aY )
{
    if( aInput.is_null() )
        return false;
    if( !aInput.is_array() || aInput.size() != 2 )
        throw std::invalid_argument( "expected point" );
    int values[2];
    for( int i = 0; i < 2; ++i )
    {
        if( !aInput[i].is_number_integer()
            || ( aInput[i].is_number_unsigned() && aInput[i].get<uint64_t>() > INT_MAX )
            || aInput[i].get<int64_t>() < INT_MIN || aInput[i].get<int64_t>() > INT_MAX )
            throw std::invalid_argument( "component out of range" );
        values[i] = aInput[i].get<int>();
    }
    aX = values[0];
    aY = values[1];
    return true;
}


JSON run( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        throw std::invalid_argument( "expected string: op" );
    const std::string op = aInput.at( "op" ).get<std::string>();

    if( op != "to_vector2i_size" && op != "to_vector2i_point" && op != "to_vector2d_point"
        && op != "to_vector2d_size" && op != "to_wx_point" && op != "to_wx_size" )
        throw std::invalid_argument( "unknown operation" );

    checkKeys( aInput, { "op", "value" } );
    int        x = 0, y = 0;
    const bool given = readPair( aInput.at( "value" ), x, y );
    // Default sources use the types' own default constructors, never ( 0, 0 ) literals.
    const wxPoint  point = given ? wxPoint( x, y ) : wxPoint();
    const wxSize   size = given ? wxSize( x, y ) : wxSize();
    const VECTOR2I vector = given ? VECTOR2I( x, y ) : VECTOR2I();

    if( op == "to_vector2i_size" )
    {
        const VECTOR2I v = ToVECTOR2I( size );
        return integer( v.x, v.y );
    }
    if( op == "to_vector2i_point" )
    {
        const VECTOR2I v = ToVECTOR2I( point );
        return integer( v.x, v.y );
    }
    if( op == "to_vector2d_point" )
        return vector2d( ToVECTOR2D( point ) );
    if( op == "to_vector2d_size" )
        return vector2d( ToVECTOR2D( size ) );
    if( op == "to_wx_point" )
    {
        const wxPoint p = ToWxPoint( vector );
        return integer( p.x, p.y );
    }
    const wxSize s = ToWxSize( vector );
    return integer( s.x, s.y );
}
} // namespace

namespace oracle
{
void RegisterVector2Wx( REGISTRY& aRegistry )
{
    aRegistry.Add( VECTOR2WX_UNIT, "math/vector2wx.h", run );
}
} // namespace oracle
