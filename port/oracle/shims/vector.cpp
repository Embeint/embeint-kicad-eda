// SPDX-License-Identifier: GPL-3.0-or-later
// Calls only the unmodified pinned VECTOR2/VECTOR3 APIs. Wide arithmetic below
// is definedness preflight, never an alternate implementation of returned values.
#include "registry.h"
#include <math/vector2d.h>
#include <math/vector3.h>
#include <bit>
#include <cmath>
#include <climits>
#include <iomanip>
#include <locale>
#include <sstream>
#include <type_traits>

static_assert( CHAR_BIT == 8 && sizeof( int ) == 4 && sizeof( unsigned ) == 4 && sizeof( int64_t ) == 8
                       && sizeof( double ) == 8 && std::numeric_limits<double>::is_iec559,
               "vector adapter requires the pinned arithmetic type widths" );

static_assert( std::is_same_v<VECTOR2<>, VECTOR2I> && std::is_same_v<VECTOR3<>, VECTOR3I>,
               "default vector specializations must remain the integer aliases" );

namespace
{
using oracle::JSON;

void undefined()
{
    throw std::invalid_argument( "undefined vector operation" );
}

void keys( const JSON& aValue, const std::vector<std::string>& aRequired,
           const std::vector<std::string>& aOptional = {} )
{
    if( !aValue.is_object() )
        throw std::invalid_argument( "expected object" );
    for( const auto& k : aRequired )
        if( !aValue.contains( k ) )
            throw std::invalid_argument( "unexpected or missing field" );
    for( const auto& [k, value] : aValue.items() )
        if( std::find( aRequired.begin(), aRequired.end(), k ) == aRequired.end()
            && std::find( aOptional.begin(), aOptional.end(), k ) == aOptional.end() )
            throw std::invalid_argument( "unexpected or missing field" );
}

bool isOperation( const std::string& aOperation, std::initializer_list<const char*> aChoices )
{
    return std::any_of( aChoices.begin(), aChoices.end(),
                        [&]( const char* aChoice )
                        {
                            return aOperation == aChoice;
                        } );
}

template <typename T>
T readComponent( const JSON& aValue )
{
    if constexpr( std::is_same_v<T, double> )
    {
        if( !aValue.is_string() )
            throw std::invalid_argument( "invalid component" );
        const std::string s = aValue.get<std::string>();
        if( s.size() != 16 || s.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
            throw std::invalid_argument( "invalid component" );
        return std::bit_cast<double>( std::stoull( s, nullptr, 16 ) );
    }
    else
    {
        if( !aValue.is_number_integer() )
            throw std::invalid_argument( "invalid component" );
        if( aValue.is_number_unsigned() )
        {
            const uint64_t n = aValue.get<uint64_t>();
            if( n > uint64_t( std::numeric_limits<T>::max() ) )
                throw std::invalid_argument( "invalid component" );
            return T( n );
        }
        const int64_t n = aValue.get<int64_t>();
        if( n < int64_t( std::numeric_limits<T>::min() ) || n > int64_t( std::numeric_limits<T>::max() ) )
            throw std::invalid_argument( "invalid component" );
        return T( n );
    }
}

template <typename T>
JSON wire( T aValue )
{
    if constexpr( std::is_same_v<T, double> )
    {
        std::ostringstream s;
        s.imbue( std::locale::classic() );
        s << std::hex << std::setfill( '0' ) << std::setw( 16 ) << std::bit_cast<uint64_t>( aValue );
        return s.str();
    }
    else
        return aValue;
}

template <typename T>
JSON wire( const VECTOR2<T>& aValue )
{
    return JSON::array( { wire( aValue.x ), wire( aValue.y ) } );
}
template <typename T>
JSON wire( const VECTOR3<T>& aValue )
{
    return JSON::array( { wire( aValue.x ), wire( aValue.y ), wire( aValue.z ) } );
}
JSON valueResult( JSON aValue )
{
    return JSON{ { "value", aValue } };
}
JSON mutationResult( JSON aValue, JSON aState, bool aSame )
{
    return JSON{ { "value", aValue }, { "state", aState }, { "same_ref", aSame } };
}

template <typename T>
VECTOR2<T> readVector2( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 2 )
        throw std::invalid_argument( "invalid vector" );
    return VECTOR2<T>( readComponent<T>( aValue[0] ), readComponent<T>( aValue[1] ) );
}
template <typename T>
VECTOR3<T> readVector3( const JSON& aValue )
{
    if( !aValue.is_array() || aValue.size() != 3 )
        throw std::invalid_argument( "invalid vector" );
    return VECTOR3<T>( readComponent<T>( aValue[0] ), readComponent<T>( aValue[1] ), readComponent<T>( aValue[2] ) );
}

// Each signed multiplication/subtraction/addition is checked separately. C++
// cancellation after an overflowing product does not make a call defined.
template <typename T>
T checked( __int128 aNumber )
{
    if( aNumber < std::numeric_limits<T>::min() || aNumber > std::numeric_limits<T>::max() )
        undefined();
    return T( aNumber );
}
template <typename T>
T checkedAdd( T aLeft, T aRight )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> )
        return checked<T>( __int128( aLeft ) + aRight );
    else
        return aLeft + aRight;
}
template <typename T>
T checkedSub( T aLeft, T aRight )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> )
        return checked<T>( __int128( aLeft ) - aRight );
    else
        return aLeft - aRight;
}
template <typename T>
T checkedMul( T aLeft, T aRight )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> )
        return checked<T>( __int128( aLeft ) * aRight );
    else
        return aLeft * aRight;
}
template <typename T>
void checkDivision( T aLeft, T aRight )
{
    if constexpr( std::is_integral_v<T> )
    {
        if( aRight == 0 )
            undefined();
        if constexpr( std::is_signed_v<T> )
            if( aLeft == std::numeric_limits<T>::min() && aRight == -1 )
                undefined();
    }
}
template <typename T>
void checkNegation( T aValue )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> )
        if( aValue == std::numeric_limits<T>::min() )
            undefined();
}
template <typename T>
void checkCast( double aValue )
{
    if constexpr( std::is_integral_v<T> )
    {
        double t = std::trunc( aValue );
        double upper = std::is_signed_v<T> ? -double( std::numeric_limits<T>::min() )
                                           : double( std::numeric_limits<T>::max() ) + 1.0;
        if( !std::isfinite( t ) || t < double( std::numeric_limits<T>::min() ) || t >= upper )
            undefined();
    }
}
template <typename U, typename T>
void checkCast2( const VECTOR2<T>& aVector )
{
    if constexpr( std::is_floating_point_v<T> && std::is_integral_v<U> )
    {
        checkCast<U>( std::clamp( aVector.x, T( std::numeric_limits<U>::min() ), T( std::numeric_limits<U>::max() ) ) );
        checkCast<U>( std::clamp( aVector.y, T( std::numeric_limits<U>::min() ), T( std::numeric_limits<U>::max() ) ) );
    }
}
template <typename U, typename T>
void checkCast3( const VECTOR3<T>& aVector )
{
    if constexpr( std::is_floating_point_v<T> && std::is_integral_v<U> )
    {
        checkCast<U>( aVector.x );
        checkCast<U>( aVector.y );
        checkCast<U>( aVector.z );
    }
}

template <typename T>
void guardVector2( const std::string& aOp, VECTOR2<T> aVector, const VECTOR2<T>& aOther, T aScalar,
                   const std::string& aAlias )
{
    using E = typename VECTOR2<T>::extended_type;
    if( isOperation( aOp,
                     { "add", "add_scalar", "sub", "sub_scalar", "mul_scalar", "scalar_mul", "add_assign", "sub_assign",
                       "mul_assign", "add_assign_scalar", "sub_assign_scalar", "mul_assign_scalar" } ) )
    {
        bool scalarOp = isOperation( aOp, { "add_scalar", "sub_scalar", "mul_scalar", "scalar_mul", "add_assign_scalar",
                                            "sub_assign_scalar", "mul_assign_scalar" } );
        auto rhs = [&]( bool aX )
        {
            return scalarOp ? ( aAlias == "x"   ? aVector.aX
                                : aAlias == "y" ? aVector.aY
                                                : aScalar )
                            : ( aX ? aOther.aX : aOther.aY );
        };
        auto calc = [&]( T aX, T aY )
        {
            return aOp.starts_with( "add" )   ? checkedAdd( aX, aY )
                   : aOp.starts_with( "sub" ) ? checkedSub( aX, aY )
                                              : checkedMul( aX, aY );
        };
        aVector.aX = calc( aVector.aX, rhs( true ) );
        aVector.aY = calc( aVector.aY, rhs( false ) );
    }
    if( aOp == "neg" )
    {
        checkNegation( aVector.aX );
        checkNegation( aVector.aY );
    }
    if( aOp == "perpendicular" )
        checkNegation( aVector.aY );
    if( isOperation( aOp, { "dot", "operator_dot", "cross" } ) )
    {
        E p = checkedMul( E( aVector.aX ), E( aOp == "cross" ? aOther.aY : aOther.aX ) );
        E q = checkedMul( E( aVector.aY ), E( aOp == "cross" ? aOther.aX : aOther.aY ) );
        if( aOp == "cross" )
            checkedSub( p, q );
        else
            checkedAdd( p, q );
    }
    if( aOp == "squared_distance" )
    {
        E aX = checkedSub( E( aVector.aX ), E( aOther.aX ) ), aY = checkedSub( E( aVector.aY ), E( aOther.aY ) );
        checkedAdd( checkedMul( aX, aX ), checkedMul( aY, aY ) );
    }
    if( aOp == "resize" && !( aVector.aX == 0 && aVector.aY == 0 ) )
    {
        checkNegation( aVector.aX );
        checkNegation( aVector.aY ); // std::abs signed minimum is undefined
        if( std::abs( aVector.aX ) == std::abs( aVector.aY ) )
            checkNegation( aScalar );
        else
        {
            E aX = checkedMul( E( aVector.aX ), E( aVector.aX ) ), aY = checkedMul( E( aVector.aY ), E( aVector.aY ) );
            E l = checkedAdd( aX, aY );
            checkedMul( E( aScalar ), E( aScalar ) );
            if constexpr( std::is_integral_v<T> )
                if( l == 0 )
                    undefined();
        }
    }
}

template <typename T>
JSON runVector2( const JSON& aInput, const std::string& aOp )
{
    using E = typename VECTOR2<T>::extended_type;
    VECTOR2<T>  a = readVector2<T>( aInput.at( "a" ) );
    VECTOR2<T>  b = aInput.contains( "b" ) && aOp != "distance" ? readVector2<T>( aInput.at( "b" ) ) : VECTOR2<T>();
    T           s = aInput.contains( "scalar" ) && !isOperation( aOp, { "divide", "add_float", "sub_float" } )
                            ? readComponent<T>( aInput.at( "scalar" ) )
                            : T();
    std::string alias = aInput.value( "alias", std::string() );
    if( !alias.empty() && alias != "x" && alias != "y" )
        throw std::invalid_argument( "invalid scalar alias" );
    guardVector2( aOp, a, b, s, alias );
    if( aOp == "default" )
        return valueResult( wire( VECTOR2<T>() ) );
    if( aOp == "new" )
        return valueResult( wire( VECTOR2<T>( a.x, a.y ) ) );
    if( aOp == "copy" )
        return valueResult( wire( VECTOR2<T>( a ) ) );
    if( aOp == "assign" )
    {
        VECTOR2<T>  v;
        VECTOR2<T>& result = ( v = a );
        return mutationResult( wire( result ), wire( v ), &result == &v );
    }
    if( aOp == "traits" )
        return JSON{ { "min", wire( VECTOR2<T>::ECOORD_MIN ) }, { "max", wire( VECTOR2<T>::ECOORD_MAX ) } };
    if( aOp == "compare" )
        return JSON{ { "eq", a == b },
                     { "ne", a != b },
                     { "lt", a < b },
                     { "le", a <= b },
                     { "gt", a > b },
                     { "ge", a >= b },
                     { "compare", LexicographicalCompare( a, b ) },
                     { "min", wire( LexicographicalMin( a, b ) ) },
                     { "max", wire( LexicographicalMax( a, b ) ) },
                     { "min_first", &a == &LexicographicalMin( a, b ) },
                     { "max_first", &a == &LexicographicalMax( a, b ) } };
    if( aOp == "add" )
        return valueResult( wire( a + b ) );
    if( aOp == "sub" )
        return valueResult( wire( a - b ) );
    if( aOp == "dot" )
        return valueResult( wire( a.Dot( b ) ) );
    if( aOp == "operator_dot" )
        return valueResult( wire( a * b ) );
    if( aOp == "cross" )
        return valueResult( wire( a.Cross( b ) ) );
    if( aOp == "squared_distance" )
        return valueResult( wire( a.SquaredDistance( b ) ) );
    if( aOp == "distance" )
    {
        const auto v = readVector2<E>( aInput.at( "b" ) );
        checkedSub( v.x, E( a.x ) );
        checkedSub( v.y, E( a.y ) );
        return valueResult( wire( a.Distance( v ) ) );
    }
    if( aOp == "norm" )
        return valueResult( wire( a.EuclideanNorm() ) );
    if( aOp == "squared_norm" )
        return valueResult( wire( a.SquaredEuclideanNorm() ) );
    if( aOp == "neg" )
        return valueResult( wire( -a ) );
    if( aOp == "perpendicular" )
        return valueResult( wire( a.Perpendicular() ) );
    if( aOp == "mul_scalar" )
        return valueResult( wire( a * s ) );
    if( aOp == "scalar_mul" )
        return valueResult( wire( s * a ) );
    if( aOp == "add_scalar" )
        return valueResult( wire( a + s ) );
    if( aOp == "sub_scalar" )
        return valueResult( wire( a - s ) );
    if( aOp == "add_float" )
        return valueResult( wire( a + readComponent<double>( aInput.at( "scalar" ) ) ) );
    if( aOp == "sub_float" )
        return valueResult( wire( a - readComponent<double>( aInput.at( "scalar" ) ) ) );
    if( aOp == "divide" )
        return valueResult( wire( a / readComponent<double>( aInput.at( "scalar" ) ) ) );
    if( aOp == "resize" )
        return valueResult( wire( a.Resize( s ) ) );
    if( aOp == "format" )
        return valueResult( a.Format() );
    if( aOp == "stream" )
    {
        std::ostringstream out;
        out.imbue( std::locale::classic() );
        out << a;
        return valueResult( out.str() );
    }
    if( isOperation( aOp, { "cast", "cast_call" } ) )
    {
        const auto target = aInput.at( "target" );
#define CAST2( Name, U )                                                                                               \
    if( target == Name )                                                                                               \
    {                                                                                                                  \
        checkCast2<U>( a );                                                                                            \
        return valueResult( wire( aOp == "cast" ? VECTOR2<U>( a ) : a.template operator()<U>() ) );                    \
    }
        CAST2( "i32", int );
        CAST2( "i64", int64_t );
        CAST2( "f64", double );
#undef CAST2
        throw std::invalid_argument( "unsupported target type" );
    }
    if( aOp == "add_assign" )
    {
        VECTOR2<T>& r = ( a += b );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "sub_assign" )
    {
        VECTOR2<T>& r = ( a -= b );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "mul_assign" )
    {
        VECTOR2<T>& r = ( a *= b );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    const T& operand = alias == "x" ? a.x : alias == "y" ? a.y : s;
    if( aOp == "add_assign_scalar" )
    {
        VECTOR2<T>& r = ( a += operand );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "sub_assign_scalar" )
    {
        VECTOR2<T>& r = ( a -= operand );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "mul_assign_scalar" )
    {
        VECTOR2<T>& r = ( a *= operand );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    throw std::invalid_argument( "unknown operation" );
}

template <typename T>
JSON runVector3( const JSON& aInput, const std::string& aOp )
{
    using E = typename VECTOR3<T>::extended_type;
    VECTOR3<T>  a = readVector3<T>( aInput.at( "a" ) );
    VECTOR3<T>  b = aInput.contains( "b" ) ? readVector3<T>( aInput.at( "b" ) ) : VECTOR3<T>();
    T           s = aInput.contains( "scalar" ) ? readComponent<T>( aInput.at( "scalar" ) ) : T();
    std::string alias = aInput.value( "alias", std::string() );
    if( !alias.empty() && alias != "x" && alias != "y" && alias != "z" )
        throw std::invalid_argument( "invalid scalar alias" );
    if( !alias.empty() )
        s = alias == "x" ? a.x : alias == "y" ? a.y : a.z; // by VALUE
    if( aOp == "cross" )
    {
        checkedSub( checkedMul( a.y, b.z ), checkedMul( a.z, b.y ) );
        checkedSub( checkedMul( a.z, b.x ), checkedMul( a.x, b.z ) );
        checkedSub( checkedMul( a.x, b.y ), checkedMul( a.y, b.x ) );
    }
    if( aOp == "dot" )
        checkedAdd( checkedAdd( checkedMul( E( a.x ), E( b.x ) ), checkedMul( E( a.y ), E( b.y ) ) ),
                    checkedMul( E( a.z ), E( b.z ) ) );
    if( isOperation( aOp, { "norm", "normalize" } ) )
    {
        const E sum = checkedAdd( checkedAdd( checkedMul( E( a.x ), E( a.x ) ), checkedMul( E( a.y ), E( a.y ) ) ),
                                  checkedMul( E( a.z ), E( a.z ) ) );
        checkCast<T>( std::sqrt( sum ) );
    }
    if( aOp == "mul_assign_scalar" )
    {
        checkedMul( a.x, s );
        checkedMul( a.y, s );
        checkedMul( a.z, s );
    }
    if( aOp == "div_assign_scalar" )
    {
        checkDivision( a.x, s );
        checkDivision( a.y, s );
        checkDivision( a.z, s );
    }
    if( aOp == "default" )
        return valueResult( wire( VECTOR3<T>() ) );
    if( aOp == "new" )
        return valueResult( wire( VECTOR3<T>( a.x, a.y, a.z ) ) );
    if( aOp == "copy" )
        return valueResult( wire( VECTOR3<T>( a ) ) );
    if( aOp == "assign" )
    {
        VECTOR3<T>  v;
        VECTOR3<T>& result = ( v = a );
        return mutationResult( wire( result ), wire( v ), &result == &v );
    }
    if( aOp == "traits" )
        return JSON{ { "min", wire( VECTOR3<T>::ECOORD_MIN ) }, { "max", wire( VECTOR3<T>::ECOORD_MAX ) } };
    if( aOp == "compare" )
        return JSON{ { "eq", a == b }, { "ne", a != b } };
    if( aOp == "dot" )
        return valueResult( wire( a.Dot( b ) ) );
    if( aOp == "cross" )
        return valueResult( wire( a.Cross( b ) ) );
    if( aOp == "norm" )
        return valueResult( wire( a.EuclideanNorm() ) );
    if( aOp == "normalize" )
    {
        const VECTOR3<T> v = a.Normalize();
        return mutationResult( wire( v ), wire( a ), &v == &a );
    }
    if( aOp == "set_all" )
    {
        const VECTOR3<T> v = a.SetAll( s );
        return mutationResult( wire( v ), wire( a ), &v == &a );
    }
    if( aOp == "mul_assign_scalar" )
    {
        VECTOR3<T>& r = ( a *= s );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "div_assign_scalar" )
    {
        VECTOR3<T>& r = ( a /= s );
        return mutationResult( wire( r ), wire( a ), &r == &a );
    }
    if( aOp == "stream" )
    {
        std::ostringstream out;
        out.imbue( std::locale::classic() );
        out << a;
        return valueResult( out.str() );
    }
    if( aOp == "cast" )
    {
        const auto target = aInput.at( "target" );
#define CAST3( Name, U )                                                                                               \
    if( target == Name )                                                                                               \
    {                                                                                                                  \
        checkCast3<U>( a );                                                                                            \
        return valueResult( wire( VECTOR3<U>( a ) ) );                                                                 \
    }
        CAST3( "i32", int );
        CAST3( "u32", unsigned );
        CAST3( "f64", double );
#undef CAST3
        throw std::invalid_argument( "unsupported target type" );
    }
    throw std::invalid_argument( "unknown operation" );
}

JSON run( const JSON& aInput )
{
    if( !aInput.contains( "op" ) || !aInput.at( "op" ).is_string() )
        throw std::invalid_argument( "unknown operation" );
    const std::string        op = aInput.at( "op" ).get<std::string>();
    std::vector<std::string> fields = { "op", "dim", "type", "a" }, optional;
    if( isOperation( op, { "compare", "add", "sub", "dot", "operator_dot", "cross", "distance", "squared_distance",
                           "add_assign", "sub_assign", "mul_assign", "equals" } ) )
        fields.push_back( "b" );
    bool compound =
            isOperation( op, { "mul_assign_scalar", "div_assign_scalar", "add_assign_scalar", "sub_assign_scalar" } );
    if( isOperation( op, { "mul_scalar", "scalar_mul", "add_scalar", "sub_scalar", "divide", "resize", "set_all",
                           "add_float", "sub_float" } )
        || ( compound && !aInput.contains( "alias" ) ) )
        fields.push_back( "scalar" );
    if( isOperation( op, { "cast", "cast_call" } ) )
        fields.push_back( "target" );
    if( compound )
        optional.push_back( "alias" );
    if( op == "equals" )
        optional.push_back( "epsilon" );
    keys( aInput, fields, optional );
    if( aInput.contains( "alias" ) && ( !aInput.at( "alias" ).is_string() || aInput.at( "alias" ) == "" ) )
        throw std::invalid_argument( "invalid scalar alias" );
    if( isOperation( op, { "equals", "ki_round", "ki_round_xy" } ) )
    {
        if( !aInput.at( "dim" ).is_number_integer() || aInput.at( "dim" ) != 2 || aInput.at( "type" ) != "f64" )
            throw std::invalid_argument( "unsupported component type" );
        const auto a = readVector2<double>( aInput.at( "a" ) );
        if( op == "equals" )
        {
            const auto b = readVector2<double>( aInput.at( "b" ) );
            return valueResult( aInput.contains( "epsilon" )
                                        ? equals( a, b, readComponent<double>( aInput.at( "epsilon" ) ) )
                                        : equals( a, b ) );
        }
        if( op == "ki_round" )
            return valueResult( wire( KiROUND( a ) ) );
        return valueResult( wire( KiROUND( a.x, a.y ) ) );
    }
    if( aInput.at( "dim" ).is_number_integer() && aInput.at( "dim" ) == 2 )
    {
        if( aInput.at( "type" ) == "i32" )
            return runVector2<int>( aInput, op );
        if( aInput.at( "type" ) == "i64" )
            return runVector2<int64_t>( aInput, op );
        if( aInput.at( "type" ) == "f64" )
            return runVector2<double>( aInput, op );
    }
    if( aInput.at( "dim" ).is_number_integer() && aInput.at( "dim" ) == 3 )
    {
        if( aInput.at( "type" ) == "i32" )
            return runVector3<int>( aInput, op );
        if( aInput.at( "type" ) == "u32" )
            return runVector3<unsigned>( aInput, op );
        if( aInput.at( "type" ) == "f64" )
            return runVector3<double>( aInput, op );
    }
    throw std::invalid_argument( "unsupported component type" );
}
} // namespace
namespace oracle
{
void RegisterVector( REGISTRY& aRegistry )
{
    aRegistry.Add( "libs.kimath.include.math.application.ui.p002",
                   "libs/kimath/include/math/vector2d.h;libs/kimath/include/math/vector3.h", run );
}
} // namespace oracle
