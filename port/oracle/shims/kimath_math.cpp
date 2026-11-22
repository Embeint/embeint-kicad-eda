// SPDX-License-Identifier: GPL-3.0-or-later
//
// Oracle adapter for unit libs.kimath.include.math.application.pure.p001: the pinned
// math/matrix3x3.h and math/wide_int.h. Only validation, conversion, calls into the genuine
// KiCad header templates and serialization live here. The operation schema is documented in
// rust/kicad-math-shim/README.md; rust/kicad-math-shim validates every request identically.
//
// Binary64 values are exchanged as exactly 16 lowercase hexadecimal digits of their IEEE-754
// bits, so infinities, NaNs (sign and payload), signed zeros and subnormals are transported
// losslessly without non-finite JSON numbers.
#include "registry.h"
#include <math/matrix3x3.h>
#include <math/wide_int.h>
#include <algorithm>
#include <array>
#include <bit>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace oracle
{
static_assert( std::numeric_limits<double>::is_iec559 && sizeof( double ) == sizeof( uint64_t ),
               "bit transport requires IEEE-754 binary64" );
static_assert( sizeof( KI_INT128 ) == 16, "KI_INT128 must be the 128-bit __int128" );

using FIELDS = std::vector<const char*>;

struct OPERATION
{
    const char* name;
    FIELDS      fields;
};

// Each operation and its exact argument fields, in validation order.
static const std::vector<OPERATION>& Operations()
{
    static const std::vector<OPERATION> operations = {
        { "matrix_default", {} },
        { "matrix_from_vectors", { "a1", "a2", "a3" } },
        { "matrix_from_elements", { "elements" } },
        { "set_identity", { "matrix" } },
        { "set_translation", { "matrix", "translation" } },
        { "get_translation", { "matrix" } },
        { "set_rotation", { "matrix", "angle" } },
        { "set_scale", { "matrix", "scale" } },
        { "get_scale", { "matrix" } },
        { "determinant", { "matrix" } },
        { "inverse", { "matrix" } },
        { "transpose", { "matrix" } },
        { "compare", { "matrix", "other" } },
        { "mul_matrix", { "a", "b" } },
        { "mul_vector2", { "matrix", "vector" } },
        { "mul_vector3", { "matrix", "vector" } },
        { "mul_scalar", { "matrix", "scalar" } },
        { "scalar_mul", { "scalar", "matrix" } },
        { "stream", { "matrix" } },
        { "words_to_double", { "hi", "lo" } },
        { "to_double", { "value" } },
        { "cross_wide", { "a", "b" } },
        { "dot_wide", { "a", "b" } },
    };
    return operations;
}


static void Keys( const JSON& aValue, const FIELDS& aRequired )
{
    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            throw std::invalid_argument( "unexpected or missing field" );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        if( std::find_if( aRequired.begin(), aRequired.end(),
                          [&]( const char* aKey ) { return key == aKey; } )
            == aRequired.end() )
        {
            throw std::invalid_argument( "unexpected or missing field" );
        }
    }
}


static double Double( const JSON& aValue, const std::string& aName )
{
    const std::string error = "expected double bit string: " + aName;

    if( !aValue.is_string() )
        throw std::invalid_argument( error );

    const std::string& text = aValue.get_ref<const std::string&>();

    if( text.size() != 16 )
        throw std::invalid_argument( error );

    uint64_t bits = 0;

    for( char c : text )
    {
        int digit;

        if( c >= '0' && c <= '9' )
            digit = c - '0';
        else if( c >= 'a' && c <= 'f' )
            digit = c - 'a' + 10;
        else
            throw std::invalid_argument( error );

        bits = ( bits << 4 ) | static_cast<uint64_t>( digit );
    }

    return std::bit_cast<double>( bits );
}


static JSON Bits( double aValue )
{
    static const char hex[] = "0123456789abcdef";
    uint64_t          bits = std::bit_cast<uint64_t>( aValue );
    std::string       text( 16, '0' );

    for( int i = 15; i >= 0; --i, bits >>= 4 )
        text[i] = hex[bits & 0xf];

    return text;
}


template <size_t N>
static std::array<double, N> Doubles( const JSON& aValue, const std::string& aName,
                                      const std::string& aKind )
{
    if( !aValue.is_array() || aValue.size() != N )
        throw std::invalid_argument( "expected " + aKind + ": " + aName );

    std::array<double, N> result{};

    for( size_t i = 0; i < N; ++i )
        result[i] = Double( aValue[i], aName );

    return result;
}


static VECTOR2D Vector2( const JSON& aValue, const std::string& aName )
{
    const std::array<double, 2> v = Doubles<2>( aValue, aName, "vector" );
    return VECTOR2D( v[0], v[1] );
}


static VECTOR3D Vector3( const JSON& aValue, const std::string& aName )
{
    const std::array<double, 3> v = Doubles<3>( aValue, aName, "vector" );
    return VECTOR3D( v[0], v[1], v[2] );
}


static MATRIX3x3D Matrix( const JSON& aValue, const std::string& aName )
{
    const std::string error = "expected matrix: " + aName;

    if( !aValue.is_array() || aValue.size() != 3 )
        throw std::invalid_argument( error );

    std::array<std::array<double, 3>, 3> e{};

    for( size_t i = 0; i < 3; ++i )
    {
        if( !aValue[i].is_array() || aValue[i].size() != 3 )
            throw std::invalid_argument( error );

        e[i] = Doubles<3>( aValue[i], aName, "matrix" );
    }

    return MATRIX3x3D( e[0][0], e[0][1], e[0][2], e[1][0], e[1][1], e[1][2], e[2][0], e[2][1],
                       e[2][2] );
}


static JSON MatrixJSON( const MATRIX3x3D& aMatrix )
{
    JSON rows = JSON::array();

    for( int i = 0; i < 3; ++i )
    {
        JSON row = JSON::array();

        for( int j = 0; j < 3; ++j )
            row.push_back( Bits( aMatrix.m_data[i][j] ) );

        rows.push_back( std::move( row ) );
    }

    return rows;
}


static JSON Vector2JSON( const VECTOR2D& aVector )
{
    return JSON::array( { Bits( aVector.x ), Bits( aVector.y ) } );
}


static JSON Vector3JSON( const VECTOR3D& aVector )
{
    return JSON::array( { Bits( aVector.x ), Bits( aVector.y ), Bits( aVector.z ) } );
}


static int64_t Int64( const JSON& aValue, const std::string& aName )
{
    const std::string error = "integer out of range: " + aName;

    if( !aValue.is_number_integer() )
        throw std::invalid_argument( error );

    if( aValue.is_number_unsigned() )
    {
        const uint64_t number = aValue.get<uint64_t>();

        if( number > static_cast<uint64_t>( std::numeric_limits<int64_t>::max() ) )
            throw std::invalid_argument( error );

        return static_cast<int64_t>( number );
    }

    return aValue.get<int64_t>();
}


static uint64_t Uint64( const JSON& aValue, const std::string& aName )
{
    const std::string error = "integer out of range: " + aName;

    if( !aValue.is_number_integer() )
        throw std::invalid_argument( error );

    if( aValue.is_number_unsigned() )
        return aValue.get<uint64_t>();

    // A signed integer token is negative, or integer -0.
    const int64_t number = aValue.get<int64_t>();

    if( number < 0 )
        throw std::invalid_argument( error );

    return static_cast<uint64_t>( number );
}


static VECTOR2L Vector2L( const JSON& aValue, const std::string& aName )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "expected vector: " + aName );

    const int64_t x = Int64( aValue[0], aName );
    const int64_t y = Int64( aValue[1], aName );
    return VECTOR2L( x, y );
}


// A KI_INT128 as canonical decimal text: optional '-', no leading zeros, no "-0".
static KI_INT128 Int128( const JSON& aValue, const std::string& aName )
{
    const std::string error = "expected int128 string: " + aName;

    if( !aValue.is_string() )
        throw std::invalid_argument( error );

    const std::string& text = aValue.get_ref<const std::string&>();
    const bool         negative = !text.empty() && text[0] == '-';
    const std::string  digits = negative ? text.substr( 1 ) : text;

    if( digits.empty() || ( digits.size() > 1 && digits[0] == '0' ) || text == "-0"
        || !std::all_of( digits.begin(), digits.end(),
                         []( char c ) { return c >= '0' && c <= '9'; } ) )
    {
        throw std::invalid_argument( error );
    }

    const KI_UINT128 limit = ( KI_UINT128( 1 ) << 127 ) - ( negative ? 0 : 1 );
    KI_UINT128       magnitude = 0;

    for( char c : digits )
    {
        const KI_UINT128 digit = static_cast<KI_UINT128>( c - '0' );

        if( magnitude > ( limit - digit ) / 10 )
            throw std::invalid_argument( error );

        magnitude = magnitude * 10 + digit;
    }

    // Two's-complement conversion of the validated magnitude (C++20 defines it).
    return negative ? static_cast<KI_INT128>( KI_UINT128( 0 ) - magnitude )
                    : static_cast<KI_INT128>( magnitude );
}


static std::string Decimal( KI_INT128 aValue )
{
    const bool negative = aValue < 0;
    KI_UINT128 magnitude = negative ? KI_UINT128( 0 ) - static_cast<KI_UINT128>( aValue )
                                    : static_cast<KI_UINT128>( aValue );
    std::string text;

    do
    {
        text.push_back( static_cast<char>( '0' + static_cast<int>( magnitude % 10 ) ) );
        magnitude /= 10;
    } while( magnitude != 0 );

    if( negative )
        text.push_back( '-' );

    return std::string( text.rbegin(), text.rend() );
}


static JSON WideResult( KI_INT128 aValue )
{
    return { { "value", Decimal( aValue ) }, { "double", Bits( ToDouble( aValue ) ) } };
}


static JSON MatrixReply( const MATRIX3x3D& aMatrix )
{
    return { { "matrix", MatrixJSON( aMatrix ) } };
}


static JSON KimathMath( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        throw std::invalid_argument( "expected op string" );

    const std::string op = aInput.at( "op" ).get<std::string>();
    const auto&       operations = Operations();
    const auto        found = std::find_if( operations.begin(), operations.end(),
                                            [&]( const OPERATION& aOp ) { return op == aOp.name; } );

    if( found == operations.end() )
        throw std::invalid_argument( "unknown operation" );

    FIELDS required = { "op" };
    required.insert( required.end(), found->fields.begin(), found->fields.end() );
    Keys( aInput, required );

    auto arg = [&]( const char* aName ) -> const JSON& { return aInput.at( aName ); };

    if( op == "matrix_default" )
    {
        MATRIX3x3D m;
        return MatrixReply( m );
    }
    else if( op == "matrix_from_vectors" )
    {
        const VECTOR3D a1 = Vector3( arg( "a1" ), "a1" );
        const VECTOR3D a2 = Vector3( arg( "a2" ), "a2" );
        const VECTOR3D a3 = Vector3( arg( "a3" ), "a3" );
        return MatrixReply( MATRIX3x3D( a1, a2, a3 ) );
    }
    else if( op == "matrix_from_elements" )
    {
        return MatrixReply( Matrix( arg( "elements" ), "elements" ) );
    }
    else if( op == "set_identity" )
    {
        MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        m.SetIdentity();
        return MatrixReply( m );
    }
    else if( op == "set_translation" )
    {
        MATRIX3x3D     m = Matrix( arg( "matrix" ), "matrix" );
        const VECTOR2D translation = Vector2( arg( "translation" ), "translation" );
        m.SetTranslation( translation );
        return MatrixReply( m );
    }
    else if( op == "get_translation" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        return { { "vector", Vector2JSON( m.GetTranslation() ) } };
    }
    else if( op == "set_rotation" )
    {
        MATRIX3x3D   m = Matrix( arg( "matrix" ), "matrix" );
        const double angle = Double( arg( "angle" ), "angle" );
        m.SetRotation( angle );
        return MatrixReply( m );
    }
    else if( op == "set_scale" )
    {
        MATRIX3x3D     m = Matrix( arg( "matrix" ), "matrix" );
        const VECTOR2D scale = Vector2( arg( "scale" ), "scale" );
        m.SetScale( scale );
        return MatrixReply( m );
    }
    else if( op == "get_scale" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        return { { "vector", Vector2JSON( m.GetScale() ) } };
    }
    else if( op == "determinant" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        return { { "value", Bits( m.Determinant() ) } };
    }
    else if( op == "inverse" )
    {
        return MatrixReply( Matrix( arg( "matrix" ), "matrix" ).Inverse() );
    }
    else if( op == "transpose" )
    {
        return MatrixReply( Matrix( arg( "matrix" ), "matrix" ).Transpose() );
    }
    else if( op == "compare" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        const MATRIX3x3D other = Matrix( arg( "other" ), "other" );
        return { { "equal", m == other }, { "not_equal", m != other } };
    }
    else if( op == "mul_matrix" )
    {
        const MATRIX3x3D a = Matrix( arg( "a" ), "a" );
        const MATRIX3x3D b = Matrix( arg( "b" ), "b" );
        return MatrixReply( a * b );
    }
    else if( op == "mul_vector2" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        const VECTOR2D   v = Vector2( arg( "vector" ), "vector" );
        return { { "vector", Vector2JSON( m * v ) } };
    }
    else if( op == "mul_vector3" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        const VECTOR3D   v = Vector3( arg( "vector" ), "vector" );
        return { { "vector", Vector3JSON( m * v ) } };
    }
    else if( op == "mul_scalar" )
    {
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        const double     scalar = Double( arg( "scalar" ), "scalar" );
        return MatrixReply( m * scalar );
    }
    else if( op == "scalar_mul" )
    {
        const double     scalar = Double( arg( "scalar" ), "scalar" );
        const MATRIX3x3D m = Matrix( arg( "matrix" ), "matrix" );
        return MatrixReply( scalar * m );
    }
    else if( op == "stream" )
    {
        const MATRIX3x3D   m = Matrix( arg( "matrix" ), "matrix" );
        std::ostringstream stream;
        stream << m;
        return { { "text", stream.str() } };
    }
    else if( op == "words_to_double" )
    {
        const int64_t  hi = Int64( arg( "hi" ), "hi" );
        const uint64_t lo = Uint64( arg( "lo" ), "lo" );
        return { { "value", Bits( KIGEOM_WIDE::WordsToDouble( hi, lo ) ) } };
    }
    else if( op == "to_double" )
    {
        const KI_INT128 value = Int128( arg( "value" ), "value" );
        // The MSVC ToDouble body is WordsToDouble( _Word[1], _Word[0] ) on the high and low
        // words; std::_Signed128 is unavailable here, so pass the same two words directly.
        const double viaWords = KIGEOM_WIDE::WordsToDouble( static_cast<int64_t>( value >> 64 ),
                                                            static_cast<uint64_t>( value ) );
        return { { "value", Bits( ToDouble( value ) ) }, { "via_words", Bits( viaWords ) } };
    }
    else if( op == "cross_wide" )
    {
        const VECTOR2L a = Vector2L( arg( "a" ), "a" );
        const VECTOR2L b = Vector2L( arg( "b" ), "b" );
        return WideResult( CrossWide( a, b ) );
    }
    else if( op == "dot_wide" )
    {
        const VECTOR2L a = Vector2L( arg( "a" ), "a" );
        const VECTOR2L b = Vector2L( arg( "b" ), "b" );
        constexpr int64_t minimum = std::numeric_limits<int64_t>::min();

        // The only input whose exact result exceeds KI_INT128: signed overflow is undefined.
        if( a.x == minimum && a.y == minimum && b.x == minimum && b.y == minimum )
            throw std::invalid_argument( "DotWide overflows KI_INT128" );

        return WideResult( DotWide( a, b ) );
    }

    throw std::invalid_argument( "unknown operation" );
}


void RegisterKimathMath( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.math.application.pure.p001",
                   "libs/kimath/include/math/matrix3x3.h:MATRIX3x3;"
                   "libs/kimath/include/math/wide_int.h:KIGEOM_WIDE",
                   KimathMath );
}
} // namespace oracle
