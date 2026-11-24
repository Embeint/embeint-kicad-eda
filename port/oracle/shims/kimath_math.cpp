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
//
// MATRIX3x3 is instantiated with T = double (the application type) and T = int (the pinned
// qa/tests/libs/kimath/math/test_matrix3x3.cpp). For T = int, signed overflow and converting a
// NaN or out-of-range double to int are undefined behaviour, so those inputs are refused
// before the genuine template is called; the checks recompute only the preconditions, in
// the same order as the Rust port, and never supply a result.
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
static_assert( sizeof( int ) == 4, "MATRIX3x3<int> cases assume a 32-bit int" );

using FIELDS = std::vector<const char*>;

struct OPERATION
{
    const char* name;
    FIELDS      fields;
    FIELDS      optional;
};

// Each operation, its exact argument fields in validation order, and optional modifiers.
static const std::vector<OPERATION>& Operations()
{
    static const FIELDS matrix = { "type" };
    static const FIELDS scalar = { "type", "scalar_type" };
    static const std::vector<OPERATION> operations = {
        { "matrix_default", {}, matrix },
        { "matrix_from_vectors", { "a1", "a2", "a3" }, matrix },
        { "matrix_from_elements", { "elements" }, matrix },
        { "set_identity", { "matrix" }, matrix },
        { "set_translation", { "matrix", "translation" }, matrix },
        { "get_translation", { "matrix" }, matrix },
        { "set_rotation", { "matrix", "angle" }, matrix },
        { "set_scale", { "matrix", "scale" }, matrix },
        { "get_scale", { "matrix" }, matrix },
        { "determinant", { "matrix" }, matrix },
        { "inverse", { "matrix" }, matrix },
        { "transpose", { "matrix" }, matrix },
        { "compare", { "matrix", "other" }, matrix },
        { "mul_matrix", { "a", "b" }, matrix },
        { "mul_vector2", { "matrix", "vector" }, matrix },
        { "mul_vector3", { "matrix", "vector" }, matrix },
        { "mul_scalar", { "matrix", "scalar" }, scalar },
        { "scalar_mul", { "scalar", "matrix" }, scalar },
        { "stream", { "matrix" }, { "type", "state" } },
        { "words_to_double", { "hi", "lo" }, {} },
        { "to_double", { "value" }, {} },
        { "cross_wide", { "a", "b" }, {} },
        { "dot_wide", { "a", "b" }, {} },
    };
    return operations;
}

// Resource bound on stream precision and width (a protocol limit, not a C++ domain claim).
constexpr int64_t STREAM_MIN = -64;
constexpr int64_t STREAM_MAX = 512;


static bool Contains( const FIELDS& aFields, const std::string& aKey )
{
    return std::find_if( aFields.begin(), aFields.end(),
                         [&]( const char* aField ) { return aKey == aField; } )
           != aFields.end();
}


static void Keys( const JSON& aValue, const FIELDS& aRequired, const FIELDS& aOptional,
                  const char* aMessage = "unexpected or missing field" )
{
    if( !aValue.is_object() )
        throw std::invalid_argument( aMessage );

    for( const char* key : aRequired )
    {
        if( !aValue.contains( key ) )
            throw std::invalid_argument( aMessage );
    }

    for( const auto& [key, value] : aValue.items() )
    {
        if( !Contains( aRequired, key ) && !Contains( aOptional, key ) )
            throw std::invalid_argument( aMessage );
    }
}


enum class KIND
{
    DOUBLE,
    INT
};


static KIND Kind( const JSON& aInput, const std::string& aName )
{
    if( !aInput.contains( aName ) )
        return KIND::DOUBLE;

    const JSON& value = aInput.at( aName );

    if( value.is_string() && value.get_ref<const std::string&>() == "double" )
        return KIND::DOUBLE;

    if( value.is_string() && value.get_ref<const std::string&>() == "int" )
        return KIND::INT;

    throw std::invalid_argument( "expected " + aName + ": double or int" );
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


static int Int32( const JSON& aValue, const std::string& aName )
{
    const int64_t number = Int64( aValue, aName );

    if( number < std::numeric_limits<int>::min() || number > std::numeric_limits<int>::max() )
        throw std::invalid_argument( "integer out of range: " + aName );

    return static_cast<int>( number );
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


// Element conversion for the two instantiations.
template <class T>
static T Element( const JSON& aValue, const std::string& aName );

template <>
double Element<double>( const JSON& aValue, const std::string& aName )
{
    return Double( aValue, aName );
}

template <>
int Element<int>( const JSON& aValue, const std::string& aName )
{
    return Int32( aValue, aName );
}


static JSON ElementJSON( double aValue )
{
    return Bits( aValue );
}


static JSON ElementJSON( int aValue )
{
    return aValue;
}


template <class T, size_t N>
static std::array<T, N> Elements( const JSON& aValue, const std::string& aName,
                                  const std::string& aShape )
{
    if( !aValue.is_array() || aValue.size() != N )
        throw std::invalid_argument( "expected " + aShape + ": " + aName );

    std::array<T, N> result{};

    for( size_t i = 0; i < N; ++i )
        result[i] = Element<T>( aValue[i], aName );

    return result;
}


template <class T>
static VECTOR2<T> Vector2( const JSON& aValue, const std::string& aName )
{
    const std::array<T, 2> v = Elements<T, 2>( aValue, aName, "vector" );
    return VECTOR2<T>( v[0], v[1] );
}


template <class T>
static VECTOR3<T> Vector3( const JSON& aValue, const std::string& aName )
{
    const std::array<T, 3> v = Elements<T, 3>( aValue, aName, "vector" );
    return VECTOR3<T>( v[0], v[1], v[2] );
}


template <class T>
static MATRIX3x3<T> Matrix( const JSON& aValue, const std::string& aName )
{
    const std::string error = "expected matrix: " + aName;

    if( !aValue.is_array() || aValue.size() != 3 )
        throw std::invalid_argument( error );

    std::array<std::array<T, 3>, 3> e{};

    for( size_t i = 0; i < 3; ++i )
    {
        if( !aValue[i].is_array() || aValue[i].size() != 3 )
            throw std::invalid_argument( error );

        e[i] = Elements<T, 3>( aValue[i], aName, "matrix" );
    }

    return MATRIX3x3<T>( e[0][0], e[0][1], e[0][2], e[1][0], e[1][1], e[1][2], e[2][0],
                         e[2][1], e[2][2] );
}


template <class T>
static JSON MatrixReply( const MATRIX3x3<T>& aMatrix )
{
    JSON rows = JSON::array();

    for( int i = 0; i < 3; ++i )
    {
        JSON row = JSON::array();

        for( int j = 0; j < 3; ++j )
            row.push_back( ElementJSON( aMatrix.m_data[i][j] ) );

        rows.push_back( std::move( row ) );
    }

    return { { "matrix", std::move( rows ) } };
}


template <class T>
static JSON Vector2JSON( const VECTOR2<T>& aVector )
{
    return JSON::array( { ElementJSON( aVector.x ), ElementJSON( aVector.y ) } );
}


template <class T>
static JSON Vector3JSON( const VECTOR3<T>& aVector )
{
    return JSON::array(
            { ElementJSON( aVector.x ), ElementJSON( aVector.y ), ElementJSON( aVector.z ) } );
}


// Apply {"flags": [...], "precision": n, "width": n, "fill": "c"} to a fresh stream.
static void ApplyState( std::ostringstream& aStream, const JSON& aState )
{
    static const std::vector<std::pair<const char*, std::ios_base::fmtflags>> names = {
        { "boolalpha", std::ios_base::boolalpha }, { "dec", std::ios_base::dec },
        { "fixed", std::ios_base::fixed },         { "hex", std::ios_base::hex },
        { "internal", std::ios_base::internal },   { "left", std::ios_base::left },
        { "oct", std::ios_base::oct },             { "right", std::ios_base::right },
        { "scientific", std::ios_base::scientific }, { "showbase", std::ios_base::showbase },
        { "showpoint", std::ios_base::showpoint }, { "showpos", std::ios_base::showpos },
        { "skipws", std::ios_base::skipws },       { "unitbuf", std::ios_base::unitbuf },
        { "uppercase", std::ios_base::uppercase },
    };
    const char* error = "expected stream state";
    Keys( aState, { "flags", "precision", "width", "fill" }, {}, error );

    const JSON& flagList = aState.at( "flags" );

    if( !flagList.is_array() )
        throw std::invalid_argument( error );

    std::ios_base::fmtflags   flags{};
    std::vector<std::string> seen;

    for( const JSON& item : flagList )
    {
        if( !item.is_string() )
            throw std::invalid_argument( error );

        const std::string& name = item.get_ref<const std::string&>();
        const auto         found = std::find_if( names.begin(), names.end(),
                                                 [&]( const auto& aEntry ) { return name == aEntry.first; } );

        if( found == names.end() || std::find( seen.begin(), seen.end(), name ) != seen.end() )
            throw std::invalid_argument( error );

        seen.push_back( name );
        flags |= found->second;
    }

    auto bounded = [&]( const char* aName )
    {
        const JSON& value = aState.at( aName );
        const std::string message = std::string( "integer out of range: " ) + aName;

        if( !value.is_number_integer() )
            throw std::invalid_argument( message );

        if( value.is_number_unsigned() )
        {
            if( value.get<uint64_t>() > static_cast<uint64_t>( STREAM_MAX ) )
                throw std::invalid_argument( message );

            return static_cast<int64_t>( value.get<uint64_t>() );
        }

        const int64_t number = value.get<int64_t>();

        if( number < STREAM_MIN || number > STREAM_MAX )
            throw std::invalid_argument( message );

        return number;
    };

    const int64_t precision = bounded( "precision" );
    const int64_t width = bounded( "width" );
    const JSON&   fill = aState.at( "fill" );

    if( !fill.is_string() || fill.get_ref<const std::string&>().size() != 1
        || fill.get_ref<const std::string&>()[0] < 0x20
        || fill.get_ref<const std::string&>()[0] > 0x7e )
    {
        throw std::invalid_argument( error );
    }

    aStream.flags( flags );
    aStream.precision( static_cast<std::streamsize>( precision ) );
    aStream.width( static_cast<std::streamsize>( width ) );
    aStream.fill( fill.get_ref<const std::string&>()[0] );
}


// Undefined-behaviour preconditions of the MATRIX3x3<int> arithmetic, evaluated in the
// reference expression order. They only refuse; results always come from the templates.
struct INT_CHECK
{
    const char* upstream;

    [[noreturn]] void Overflow() const
    {
        throw std::invalid_argument( std::string( upstream ) + " overflows int" );
    }

    int Mul( int aA, int aB ) const
    {
        int result;

        if( __builtin_mul_overflow( aA, aB, &result ) )
            Overflow();

        return result;
    }

    int Add( int aA, int aB ) const
    {
        int result;

        if( __builtin_add_overflow( aA, aB, &result ) )
            Overflow();

        return result;
    }

    int Sub( int aA, int aB ) const
    {
        int result;

        if( __builtin_sub_overflow( aA, aB, &result ) )
            Overflow();

        return result;
    }

    int Cross( int aA, int aB, int aC, int aD ) const { return Sub( Mul( aA, aB ), Mul( aC, aD ) ); }

    int Dot3( int aA, int aX, int aB, int aY, int aC, int aZ ) const
    {
        return Add( Add( Mul( aA, aX ), Mul( aB, aY ) ), Mul( aC, aZ ) );
    }
};

static const INT_CHECK INT_DETERMINANT{ "MATRIX3x3<int>::Determinant" };
static const INT_CHECK INT_INVERSE{ "MATRIX3x3<int>::Inverse" };
static const INT_CHECK INT_MUL_MATRIX{ "operator*( MATRIX3x3<int>, MATRIX3x3<int> )" };
static const INT_CHECK INT_MUL_VECTOR2{ "operator*( MATRIX3x3<int>, VECTOR2<int> )" };
static const INT_CHECK INT_MUL_VECTOR3{ "operator*( MATRIX3x3<int>, VECTOR3<int> )" };
static const INT_CHECK INT_MUL_SCALAR{ "operator*( MATRIX3x3<int>, int )" };
static const char*     INT_MUL_DOUBLE = "operator*( MATRIX3x3<int>, double )";


static int CheckDeterminant( const MATRIX3x3<int>& aM )
{
    const auto& m = aM.m_data;
    const auto& c = INT_DETERMINANT;
    const int   first = c.Mul( m[0][0], c.Cross( m[1][1], m[2][2], m[1][2], m[2][1] ) );
    const int   second = c.Mul( m[0][1], c.Cross( m[1][0], m[2][2], m[1][2], m[2][0] ) );
    const int   third = c.Mul( m[0][2], c.Cross( m[1][0], m[2][1], m[1][1], m[2][0] ) );
    return c.Add( c.Sub( first, second ), third );
}


// int( aMember * aScalar ) is defined only when the truncated product is representable.
static void CheckConversions( const MATRIX3x3<int>& aM, double aScalar )
{
    for( int i = 0; i < 3; ++i )
    {
        for( int j = 0; j < 3; ++j )
        {
            const double product = aM.m_data[i][j] * aScalar;

            if( !( product > -2147483649.0 && product < 2147483648.0 ) )
            {
                throw std::invalid_argument( std::string( INT_MUL_DOUBLE )
                                             + " converts a value outside int" );
            }
        }
    }
}


static void CheckInverse( const MATRIX3x3<int>& aM )
{
    const auto&    m = aM.m_data;
    const auto&    c = INT_INVERSE;
    MATRIX3x3<int> adjugate( c.Cross( m[1][1], m[2][2], m[2][1], m[1][2] ),
                             c.Cross( m[0][2], m[2][1], m[2][2], m[0][1] ),
                             c.Cross( m[0][1], m[1][2], m[1][1], m[0][2] ),
                             c.Cross( m[1][2], m[2][0], m[2][2], m[1][0] ),
                             c.Cross( m[0][0], m[2][2], m[2][0], m[0][2] ),
                             c.Cross( m[0][2], m[1][0], m[1][2], m[0][0] ),
                             c.Cross( m[1][0], m[2][1], m[2][0], m[1][1] ),
                             c.Cross( m[0][1], m[2][0], m[2][1], m[0][0] ),
                             c.Cross( m[0][0], m[1][1], m[1][0], m[0][1] ) );
    CheckConversions( adjugate, 1.0 / CheckDeterminant( aM ) );
}


static void CheckMulMatrix( const MATRIX3x3<int>& aA, const MATRIX3x3<int>& aB )
{
    for( int i = 0; i < 3; ++i )
    {
        for( int j = 0; j < 3; ++j )
        {
            INT_MUL_MATRIX.Dot3( aA.m_data[i][0], aB.m_data[0][j], aA.m_data[i][1],
                                 aB.m_data[1][j], aA.m_data[i][2], aB.m_data[2][j] );
        }
    }
}


static void CheckMulVector2( const MATRIX3x3<int>& aM, const VECTOR2<int>& aV )
{
    const auto& m = aM.m_data;
    const auto& c = INT_MUL_VECTOR2;
    c.Add( c.Add( c.Mul( m[0][0], aV.x ), c.Mul( m[0][1], aV.y ) ), m[0][2] );
    c.Add( c.Add( c.Mul( m[1][0], aV.x ), c.Mul( m[1][1], aV.y ) ), m[1][2] );
}


static void CheckMulVector3( const MATRIX3x3<int>& aM, const VECTOR3<int>& aV )
{
    const auto& m = aM.m_data;

    for( int i = 0; i < 3; ++i )
        INT_MUL_VECTOR3.Dot3( m[i][0], aV.x, m[i][1], aV.y, m[i][2], aV.z );
}


static void CheckMulScalar( const MATRIX3x3<int>& aM, int aScalar )
{
    for( int i = 0; i < 3; ++i )
    {
        for( int j = 0; j < 3; ++j )
            INT_MUL_SCALAR.Mul( aM.m_data[i][j], aScalar );
    }
}


static std::string Decimal( KI_INT128 aValue );
static KI_INT128   Int128( const JSON& aValue, const std::string& aName );
static VECTOR2L    Vector2L( const JSON& aValue, const std::string& aName );
static JSON        WideResult( KI_INT128 aValue );


// Operations whose body is the same for both instantiations; returns null for others.
template <class T>
static JSON Shared( const std::string& aOp, const JSON& aInput )
{
    auto arg = [&]( const char* aName ) -> const JSON& { return aInput.at( aName ); };

    if( aOp == "matrix_default" )
    {
        MATRIX3x3<T> m;
        return MatrixReply( m );
    }
    else if( aOp == "matrix_from_vectors" )
    {
        const VECTOR3<T> a1 = Vector3<T>( arg( "a1" ), "a1" );
        const VECTOR3<T> a2 = Vector3<T>( arg( "a2" ), "a2" );
        const VECTOR3<T> a3 = Vector3<T>( arg( "a3" ), "a3" );
        return MatrixReply( MATRIX3x3<T>( a1, a2, a3 ) );
    }
    else if( aOp == "matrix_from_elements" )
    {
        return MatrixReply( Matrix<T>( arg( "elements" ), "elements" ) );
    }
    else if( aOp == "set_identity" )
    {
        MATRIX3x3<T> m = Matrix<T>( arg( "matrix" ), "matrix" );
        m.SetIdentity();
        return MatrixReply( m );
    }
    else if( aOp == "set_translation" )
    {
        MATRIX3x3<T>     m = Matrix<T>( arg( "matrix" ), "matrix" );
        const VECTOR2<T> translation = Vector2<T>( arg( "translation" ), "translation" );
        m.SetTranslation( translation );
        return MatrixReply( m );
    }
    else if( aOp == "get_translation" )
    {
        const MATRIX3x3<T> m = Matrix<T>( arg( "matrix" ), "matrix" );
        return { { "vector", Vector2JSON( m.GetTranslation() ) } };
    }
    else if( aOp == "set_scale" )
    {
        MATRIX3x3<T>     m = Matrix<T>( arg( "matrix" ), "matrix" );
        const VECTOR2<T> scale = Vector2<T>( arg( "scale" ), "scale" );
        m.SetScale( scale );
        return MatrixReply( m );
    }
    else if( aOp == "get_scale" )
    {
        const MATRIX3x3<T> m = Matrix<T>( arg( "matrix" ), "matrix" );
        return { { "vector", Vector2JSON( m.GetScale() ) } };
    }
    else if( aOp == "transpose" )
    {
        return MatrixReply( Matrix<T>( arg( "matrix" ), "matrix" ).Transpose() );
    }
    else if( aOp == "compare" )
    {
        const MATRIX3x3<T> m = Matrix<T>( arg( "matrix" ), "matrix" );
        const MATRIX3x3<T> other = Matrix<T>( arg( "other" ), "other" );
        return { { "equal", m == other }, { "not_equal", m != other } };
    }
    else if( aOp == "stream" )
    {
        const MATRIX3x3<T> m = Matrix<T>( arg( "matrix" ), "matrix" );
        std::ostringstream stream;

        if( !aInput.contains( "state" ) )
        {
            stream << m;
            return { { "text", stream.str() } };
        }

        ApplyState( stream, aInput.at( "state" ) );
        stream << m;
        return { { "text", stream.str() }, { "width_after", stream.width() } };
    }

    return nullptr;
}


static JSON DoubleOps( const std::string& aOp, const JSON& aInput, KIND aScalar )
{
    JSON shared = Shared<double>( aOp, aInput );

    if( !shared.is_null() )
        return shared;

    auto arg = [&]( const char* aName ) -> const JSON& { return aInput.at( aName ); };

    if( aOp == "set_rotation" )
    {
        MATRIX3x3D   m = Matrix<double>( arg( "matrix" ), "matrix" );
        const double angle = Double( arg( "angle" ), "angle" );
        m.SetRotation( angle );
        return MatrixReply( m );
    }
    else if( aOp == "determinant" )
    {
        const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );
        return { { "value", Bits( m.Determinant() ) } };
    }
    else if( aOp == "inverse" )
    {
        return MatrixReply( Matrix<double>( arg( "matrix" ), "matrix" ).Inverse() );
    }
    else if( aOp == "mul_matrix" )
    {
        const MATRIX3x3D a = Matrix<double>( arg( "a" ), "a" );
        const MATRIX3x3D b = Matrix<double>( arg( "b" ), "b" );
        return MatrixReply( a * b );
    }
    else if( aOp == "mul_vector2" )
    {
        const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );
        const VECTOR2D   v = Vector2<double>( arg( "vector" ), "vector" );
        return { { "vector", Vector2JSON( m * v ) } };
    }
    else if( aOp == "mul_vector3" )
    {
        const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );
        const VECTOR3D   v = Vector3<double>( arg( "vector" ), "vector" );
        return { { "vector", Vector3JSON( m * v ) } };
    }
    else if( aOp == "mul_scalar" )
    {
        const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );

        if( aScalar == KIND::INT )
            return MatrixReply( m * Int32( arg( "scalar" ), "scalar" ) );

        return MatrixReply( m * Double( arg( "scalar" ), "scalar" ) );
    }
    else if( aOp == "scalar_mul" )
    {
        if( aScalar == KIND::INT )
        {
            const int        scalar = Int32( arg( "scalar" ), "scalar" );
            const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );
            return MatrixReply( scalar * m );
        }

        const double     scalar = Double( arg( "scalar" ), "scalar" );
        const MATRIX3x3D m = Matrix<double>( arg( "matrix" ), "matrix" );
        return MatrixReply( scalar * m );
    }

    throw std::invalid_argument( "unknown operation" );
}


static JSON IntOps( const std::string& aOp, const JSON& aInput, KIND aScalar )
{
    JSON shared = Shared<int>( aOp, aInput );

    if( !shared.is_null() )
        return shared;

    auto arg = [&]( const char* aName ) -> const JSON& { return aInput.at( aName ); };

    if( aOp == "set_rotation" )
    {
        MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        const int      angle = Int32( arg( "angle" ), "angle" );
        m.SetRotation( angle );
        return MatrixReply( m );
    }
    else if( aOp == "determinant" )
    {
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        CheckDeterminant( m );
        return { { "value", m.Determinant() } };
    }
    else if( aOp == "inverse" )
    {
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        CheckInverse( m );
        return MatrixReply( m.Inverse() );
    }
    else if( aOp == "mul_matrix" )
    {
        const MATRIX3x3<int> a = Matrix<int>( arg( "a" ), "a" );
        const MATRIX3x3<int> b = Matrix<int>( arg( "b" ), "b" );
        CheckMulMatrix( a, b );
        return MatrixReply( a * b );
    }
    else if( aOp == "mul_vector2" )
    {
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        const VECTOR2<int>   v = Vector2<int>( arg( "vector" ), "vector" );
        CheckMulVector2( m, v );
        return { { "vector", Vector2JSON( m * v ) } };
    }
    else if( aOp == "mul_vector3" )
    {
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        const VECTOR3<int>   v = Vector3<int>( arg( "vector" ), "vector" );
        CheckMulVector3( m, v );
        return { { "vector", Vector3JSON( m * v ) } };
    }
    else if( aOp == "mul_scalar" )
    {
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );

        if( aScalar == KIND::INT )
        {
            const int scalar = Int32( arg( "scalar" ), "scalar" );
            CheckMulScalar( m, scalar );
            return MatrixReply( m * scalar );
        }

        const double scalar = Double( arg( "scalar" ), "scalar" );
        CheckConversions( m, scalar );
        return MatrixReply( m * scalar );
    }
    else if( aOp == "scalar_mul" )
    {
        if( aScalar == KIND::INT )
        {
            const int            scalar = Int32( arg( "scalar" ), "scalar" );
            const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
            CheckMulScalar( m, scalar );
            return MatrixReply( scalar * m );
        }

        const double         scalar = Double( arg( "scalar" ), "scalar" );
        const MATRIX3x3<int> m = Matrix<int>( arg( "matrix" ), "matrix" );
        CheckConversions( m, scalar );
        return MatrixReply( scalar * m );
    }

    throw std::invalid_argument( "unknown operation" );
}


static JSON WideOps( const std::string& aOp, const JSON& aInput )
{
    auto arg = [&]( const char* aName ) -> const JSON& { return aInput.at( aName ); };

    if( aOp == "words_to_double" )
    {
        const int64_t  hi = Int64( arg( "hi" ), "hi" );
        const uint64_t lo = Uint64( arg( "lo" ), "lo" );
        return { { "value", Bits( KIGEOM_WIDE::WordsToDouble( hi, lo ) ) } };
    }
    else if( aOp == "to_double" )
    {
        const KI_INT128 value = Int128( arg( "value" ), "value" );
        // The MSVC ToDouble body is WordsToDouble( _Word[1], _Word[0] ) on the high and low
        // words; std::_Signed128 is unavailable here, so pass the same two words directly.
        const double viaWords = KIGEOM_WIDE::WordsToDouble( static_cast<int64_t>( value >> 64 ),
                                                            static_cast<uint64_t>( value ) );
        return { { "value", Bits( ToDouble( value ) ) }, { "via_words", Bits( viaWords ) } };
    }
    else if( aOp == "cross_wide" )
    {
        const VECTOR2L a = Vector2L( arg( "a" ), "a" );
        const VECTOR2L b = Vector2L( arg( "b" ), "b" );
        return WideResult( CrossWide( a, b ) );
    }
    else if( aOp == "dot_wide" )
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
    Keys( aInput, required, found->optional );

    if( found->optional.empty() )
        return WideOps( op, aInput );

    const KIND element = Kind( aInput, "type" );
    const KIND scalar = Kind( aInput, "scalar_type" );

    return element == KIND::INT ? IntOps( op, aInput, scalar ) : DoubleOps( op, aInput, scalar );
}


void RegisterKimathMath( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.math.application.pure.p001",
                   "libs/kimath/include/math/matrix3x3.h:MATRIX3x3;"
                   "libs/kimath/include/math/wide_int.h:KIGEOM_WIDE",
                   KimathMath );
}
} // namespace oracle
