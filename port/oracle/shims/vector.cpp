// SPDX-License-Identifier: GPL-3.0-or-later
// Calls only the unmodified pinned VECTOR2/VECTOR3 APIs. Wide arithmetic below
// is definedness preflight, never an alternate implementation of returned values.
#include "registry.h"
#include <math/vector2d.h>
#include <math/vector3.h>
#include <bit>
#include <cmath>
#include <iomanip>
#include <locale>
#include <sstream>
#include <type_traits>

namespace
{
using oracle::JSON;

void Undefined() { throw std::invalid_argument( "undefined vector operation" ); }

void Keys( const JSON& v, const std::vector<std::string>& required,
           const std::vector<std::string>& optional = {} )
{
    if( !v.is_object() )
        throw std::invalid_argument( "expected object" );
    for( const auto& k : required )
        if( !v.contains( k ) )
            throw std::invalid_argument( "unexpected or missing field" );
    for( const auto& [k, value] : v.items() )
        if( std::find( required.begin(), required.end(), k ) == required.end()
            && std::find( optional.begin(), optional.end(), k ) == optional.end() )
            throw std::invalid_argument( "unexpected or missing field" );
}

bool In( const std::string& s, std::initializer_list<const char*> choices )
{
    return std::any_of( choices.begin(), choices.end(), [&]( auto p ) { return s == p; } );
}

template <typename T> T Read( const JSON& v )
{
    if constexpr( std::is_same_v<T, double> )
    {
        if( !v.is_string() )
            throw std::invalid_argument( "invalid component" );
        const std::string s = v.get<std::string>();
        if( s.size() != 16 || s.find_first_not_of( "0123456789abcdef" ) != std::string::npos )
            throw std::invalid_argument( "invalid component" );
        return std::bit_cast<double>( std::stoull( s, nullptr, 16 ) );
    }
    else
    {
        if( !v.is_number_integer() )
            throw std::invalid_argument( "invalid component" );
        if( v.is_number_unsigned() )
        {
            auto n = v.get<uint64_t>();
            if( n > uint64_t( std::numeric_limits<T>::max() ) )
                throw std::invalid_argument( "invalid component" );
            return T( n );
        }
        auto n = v.get<int64_t>();
        if( n < int64_t( std::numeric_limits<T>::min() ) || n > int64_t( std::numeric_limits<T>::max() ) )
            throw std::invalid_argument( "invalid component" );
        return T( n );
    }
}

template <typename T> JSON Wire( T value )
{
    if constexpr( std::is_same_v<T, double> )
    {
        std::ostringstream s;
        s.imbue( std::locale::classic() );
        s << std::hex << std::setfill( '0' ) << std::setw( 16 ) << std::bit_cast<uint64_t>( value );
        return s.str();
    }
    else
        return value;
}

template <typename T> JSON Wire( const VECTOR2<T>& v ) { return JSON::array( { Wire( v.x ), Wire( v.y ) } ); }
template <typename T> JSON Wire( const VECTOR3<T>& v ) { return JSON::array( { Wire( v.x ), Wire( v.y ), Wire( v.z ) } ); }
JSON Value( JSON v ) { return JSON{ { "value", v } }; }
JSON Mutation( JSON v, JSON state, bool same ) { return JSON{ { "value", v }, { "state", state }, { "same_ref", same } }; }

template <typename T> VECTOR2<T> V2( const JSON& v )
{
    if( !v.is_array() || v.size() != 2 ) throw std::invalid_argument( "invalid vector" );
    return VECTOR2<T>( Read<T>( v[0] ), Read<T>( v[1] ) );
}
template <typename T> VECTOR3<T> V3( const JSON& v )
{
    if( !v.is_array() || v.size() != 3 ) throw std::invalid_argument( "invalid vector" );
    return VECTOR3<T>( Read<T>( v[0] ), Read<T>( v[1] ), Read<T>( v[2] ) );
}

// Each signed multiplication/subtraction/addition is checked separately. C++
// cancellation after an overflowing product does not make a call defined.
template <typename T> T Checked( __int128 n )
{
    if( n < std::numeric_limits<T>::min() || n > std::numeric_limits<T>::max() ) Undefined();
    return T( n );
}
template <typename T> T Add( T a, T b )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> ) return Checked<T>( __int128( a ) + b );
    else return a + b;
}
template <typename T> T Sub( T a, T b )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> ) return Checked<T>( __int128( a ) - b );
    else return a - b;
}
template <typename T> T Mul( T a, T b )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> ) return Checked<T>( __int128( a ) * b );
    else return a * b;
}
template <typename T> void Div( T a, T b )
{
    if constexpr( std::is_integral_v<T> )
    {
        if( b == 0 ) Undefined();
        if constexpr( std::is_signed_v<T> )
            if( a == std::numeric_limits<T>::min() && b == -1 ) Undefined();
    }
}
template <typename T> void Neg( T a )
{
    if constexpr( std::is_integral_v<T> && std::is_signed_v<T> )
        if( a == std::numeric_limits<T>::min() ) Undefined();
}
template <typename T> void Cast( double a )
{
    if constexpr( std::is_integral_v<T> )
    {
        double t = std::trunc( a );
        double upper = std::is_signed_v<T> ? -double( std::numeric_limits<T>::min() )
                                          : double( std::numeric_limits<T>::max() ) + 1.0;
        if( !std::isfinite( t ) || t < double( std::numeric_limits<T>::min() ) || t >= upper ) Undefined();
    }
}
template <typename U, typename T> void Cast2( const VECTOR2<T>& a )
{
    if constexpr( std::is_floating_point_v<T> && std::is_integral_v<U> )
    {
        Cast<U>( std::clamp( a.x, T( std::numeric_limits<U>::min() ), T( std::numeric_limits<U>::max() ) ) );
        Cast<U>( std::clamp( a.y, T( std::numeric_limits<U>::min() ), T( std::numeric_limits<U>::max() ) ) );
    }
}
template <typename U, typename T> void Cast3( const VECTOR3<T>& a )
{
    if constexpr( std::is_floating_point_v<T> && std::is_integral_v<U> )
    { Cast<U>( a.x ); Cast<U>( a.y ); Cast<U>( a.z ); }
}

template <typename T> void Guard2( const std::string& op, VECTOR2<T> a, const VECTOR2<T>& b,
                                 T scalar, const std::string& alias )
{
    using E = typename VECTOR2<T>::extended_type;
    if( In( op, { "add", "add_scalar", "sub", "sub_scalar", "mul_scalar", "scalar_mul",
                 "add_assign", "sub_assign", "mul_assign", "add_assign_scalar", "sub_assign_scalar", "mul_assign_scalar" } ) )
    {
        bool scalarOp = In( op, { "add_scalar", "sub_scalar", "mul_scalar", "scalar_mul", "add_assign_scalar", "sub_assign_scalar", "mul_assign_scalar" } );
        auto rhs = [&]( bool x ) { return scalarOp ? ( alias == "x" ? a.x : alias == "y" ? a.y : scalar ) : ( x ? b.x : b.y ); };
        auto calc = [&]( T x, T y ) { return op.starts_with( "add" ) ? Add( x, y ) : op.starts_with( "sub" ) ? Sub( x, y ) : Mul( x, y ); };
        a.x = calc( a.x, rhs( true ) );
        a.y = calc( a.y, rhs( false ) );
    }
    if( op == "neg" ) { Neg( a.x ); Neg( a.y ); }
    if( op == "perpendicular" ) Neg( a.y );
    if( In( op, { "dot", "operator_dot", "cross" } ) )
    {
        E p = Mul( E( a.x ), E( op == "cross" ? b.y : b.x ) );
        E q = Mul( E( a.y ), E( op == "cross" ? b.x : b.y ) );
        if( op == "cross" ) Sub( p, q ); else Add( p, q );
    }
    if( op == "squared_distance" )
    {
        E x = Sub( E( a.x ), E( b.x ) ), y = Sub( E( a.y ), E( b.y ) );
        Add( Mul( x, x ), Mul( y, y ) );
    }
    if( op == "resize" && !( a.x == 0 && a.y == 0 ) )
    {
        Neg( a.x ); Neg( a.y ); // std::abs signed minimum is undefined
        if( std::abs( a.x ) == std::abs( a.y ) ) Neg( scalar );
        else
        {
            E x = Mul( E( a.x ), E( a.x ) ), y = Mul( E( a.y ), E( a.y ) );
            E l = Add( x, y );
            Mul( E( scalar ), E( scalar ) );
            if constexpr( std::is_integral_v<T> ) if( l == 0 ) Undefined();
        }
    }
}

template <typename T> JSON Run2( const JSON& i, const std::string& op )
{
    using E = typename VECTOR2<T>::extended_type;
    VECTOR2<T> a = V2<T>( i.at( "a" ) );
    VECTOR2<T> b = i.contains( "b" ) && op != "distance" ? V2<T>( i.at( "b" ) ) : VECTOR2<T>();
    T s = i.contains( "scalar" ) && !In( op, { "divide", "add_float", "sub_float" } ) ? Read<T>( i.at( "scalar" ) ) : T();
    std::string alias = i.value( "alias", std::string() );
    if( !alias.empty() && alias != "x" && alias != "y" ) throw std::invalid_argument( "invalid scalar alias" );
    Guard2( op, a, b, s, alias );
    if( op == "default" ) return Value( Wire( VECTOR2<T>() ) );
    if( op == "new" ) return Value( Wire( VECTOR2<T>( a.x, a.y ) ) );
    if( op == "copy" ) return Value( Wire( VECTOR2<T>( a ) ) );
    if( op == "assign" ) { VECTOR2<T> v; v = a; return Value( Wire( v ) ); }
    if( op == "traits" ) return JSON{ { "min", Wire( VECTOR2<T>::ECOORD_MIN ) }, { "max", Wire( VECTOR2<T>::ECOORD_MAX ) } };
    if( op == "compare" )
        return JSON{ { "eq", a == b }, { "ne", a != b }, { "lt", a < b }, { "le", a <= b },
                     { "gt", a > b }, { "ge", a >= b }, { "compare", LexicographicalCompare( a, b ) },
                     { "min", Wire( LexicographicalMin( a, b ) ) }, { "max", Wire( LexicographicalMax( a, b ) ) },
                     { "min_first", &a == &LexicographicalMin( a, b ) }, { "max_first", &a == &LexicographicalMax( a, b ) } };
    if( op == "add" ) return Value( Wire( a + b ) );
    if( op == "sub" ) return Value( Wire( a - b ) );
    if( op == "dot" ) return Value( Wire( a.Dot( b ) ) );
    if( op == "operator_dot" ) return Value( Wire( a * b ) );
    if( op == "cross" ) return Value( Wire( a.Cross( b ) ) );
    if( op == "squared_distance" ) return Value( Wire( a.SquaredDistance( b ) ) );
    if( op == "distance" )
    {
        const auto v = V2<E>( i.at( "b" ) );
        Sub( v.x, E( a.x ) ); Sub( v.y, E( a.y ) );
        return Value( Wire( a.Distance( v ) ) );
    }
    if( op == "norm" ) return Value( Wire( a.EuclideanNorm() ) );
    if( op == "squared_norm" ) return Value( Wire( a.SquaredEuclideanNorm() ) );
    if( op == "neg" ) return Value( Wire( -a ) );
    if( op == "perpendicular" ) return Value( Wire( a.Perpendicular() ) );
    if( op == "mul_scalar" ) return Value( Wire( a * s ) );
    if( op == "scalar_mul" ) return Value( Wire( s * a ) );
    if( op == "add_scalar" ) return Value( Wire( a + s ) );
    if( op == "sub_scalar" ) return Value( Wire( a - s ) );
    if( op == "add_float" ) return Value( Wire( a + Read<double>( i.at( "scalar" ) ) ) );
    if( op == "sub_float" ) return Value( Wire( a - Read<double>( i.at( "scalar" ) ) ) );
    if( op == "divide" ) return Value( Wire( a / Read<double>( i.at( "scalar" ) ) ) );
    if( op == "resize" ) return Value( Wire( a.Resize( s ) ) );
    if( op == "format" ) return Value( a.Format() );
    if( op == "stream" ) { std::ostringstream out; out.imbue( std::locale::classic() ); out << a; return Value( out.str() ); }
    if( In( op, { "cast", "cast_call" } ) )
    {
        const auto target = i.at( "target" );
#define CAST2( Name, U ) if( target == Name ) { Cast2<U>( a ); return Value( Wire( op == "cast" ? VECTOR2<U>( a ) : a.template operator()<U>() ) ); }
        CAST2( "i32", int ); CAST2( "i64", int64_t ); CAST2( "f64", double );
#undef CAST2
        throw std::invalid_argument( "unsupported target type" );
    }
    if( op == "add_assign" ) { auto& r = ( a += b ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "sub_assign" ) { auto& r = ( a -= b ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "mul_assign" ) { auto& r = ( a *= b ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    const T& operand = alias == "x" ? a.x : alias == "y" ? a.y : s;
    if( op == "add_assign_scalar" ) { auto& r = ( a += operand ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "sub_assign_scalar" ) { auto& r = ( a -= operand ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "mul_assign_scalar" ) { auto& r = ( a *= operand ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    throw std::invalid_argument( "unknown operation" );
}

template <typename T> JSON Run3( const JSON& i, const std::string& op )
{
    using E = typename VECTOR3<T>::extended_type;
    VECTOR3<T> a = V3<T>( i.at( "a" ) );
    VECTOR3<T> b = i.contains( "b" ) ? V3<T>( i.at( "b" ) ) : VECTOR3<T>();
    T s = i.contains( "scalar" ) ? Read<T>( i.at( "scalar" ) ) : T();
    std::string alias = i.value( "alias", std::string() );
    if( !alias.empty() && alias != "x" && alias != "y" && alias != "z" ) throw std::invalid_argument( "invalid scalar alias" );
    if( !alias.empty() ) s = alias == "x" ? a.x : alias == "y" ? a.y : a.z; // by VALUE
    if( op == "cross" )
    {
        Sub( Mul( a.y, b.z ), Mul( a.z, b.y ) ); Sub( Mul( a.z, b.x ), Mul( a.x, b.z ) ); Sub( Mul( a.x, b.y ), Mul( a.y, b.x ) );
    }
    if( op == "dot" ) Add( Add( Mul( E( a.x ), E( b.x ) ), Mul( E( a.y ), E( b.y ) ) ), Mul( E( a.z ), E( b.z ) ) );
    if( In( op, { "norm", "normalize" } ) )
    {
        const E sum = Add( Add( Mul( E( a.x ), E( a.x ) ), Mul( E( a.y ), E( a.y ) ) ), Mul( E( a.z ), E( a.z ) ) );
        Cast<T>( std::sqrt( sum ) );
    }
    if( op == "mul_assign_scalar" ) { Mul( a.x, s ); Mul( a.y, s ); Mul( a.z, s ); }
    if( op == "div_assign_scalar" ) { Div( a.x, s ); Div( a.y, s ); Div( a.z, s ); }
    if( op == "default" ) return Value( Wire( VECTOR3<T>() ) );
    if( op == "new" ) return Value( Wire( VECTOR3<T>( a.x, a.y, a.z ) ) );
    if( op == "copy" ) return Value( Wire( VECTOR3<T>( a ) ) );
    if( op == "assign" ) { VECTOR3<T> v; v = a; return Value( Wire( v ) ); }
    if( op == "traits" ) return JSON{ { "min", Wire( VECTOR3<T>::ECOORD_MIN ) }, { "max", Wire( VECTOR3<T>::ECOORD_MAX ) } };
    if( op == "compare" ) return JSON{ { "eq", a == b }, { "ne", a != b } };
    if( op == "dot" ) return Value( Wire( a.Dot( b ) ) );
    if( op == "cross" ) return Value( Wire( a.Cross( b ) ) );
    if( op == "norm" ) return Value( Wire( a.EuclideanNorm() ) );
    if( op == "normalize" ) { const auto v = a.Normalize(); return Mutation( Wire( v ), Wire( a ), false ); }
    if( op == "set_all" ) { const auto v = a.SetAll( s ); return Mutation( Wire( v ), Wire( a ), false ); }
    if( op == "mul_assign_scalar" ) { auto& r = ( a *= s ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "div_assign_scalar" ) { auto& r = ( a /= s ); return Mutation( Wire( r ), Wire( a ), &r == &a ); }
    if( op == "stream" ) { std::ostringstream out; out.imbue( std::locale::classic() ); out << a; return Value( out.str() ); }
    if( op == "cast" )
    {
        const auto target = i.at( "target" );
#define CAST3( Name, U ) if( target == Name ) { Cast3<U>( a ); return Value( Wire( VECTOR3<U>( a ) ) ); }
        CAST3( "i32", int ); CAST3( "u32", unsigned ); CAST3( "f64", double );
#undef CAST3
        throw std::invalid_argument( "unsupported target type" );
    }
    throw std::invalid_argument( "unknown operation" );
}

JSON Run( const JSON& i )
{
    const std::string op = i.value( "op", std::string() );
    std::vector<std::string> fields = { "op", "dim", "type", "a" }, optional;
    if( In( op, { "compare", "add", "sub", "dot", "operator_dot", "cross", "distance", "squared_distance", "add_assign", "sub_assign", "mul_assign", "equals" } ) ) fields.push_back( "b" );
    bool compound = In( op, { "mul_assign_scalar", "div_assign_scalar", "add_assign_scalar", "sub_assign_scalar" } );
    if( In( op, { "mul_scalar", "scalar_mul", "add_scalar", "sub_scalar", "divide", "resize", "set_all", "add_float", "sub_float" } ) || ( compound && !i.contains( "alias" ) ) ) fields.push_back( "scalar" );
    if( In( op, { "cast", "cast_call" } ) ) fields.push_back( "target" );
    if( compound ) optional.push_back( "alias" );
    if( op == "equals" ) optional.push_back( "epsilon" );
    Keys( i, fields, optional );
    if( In( op, { "equals", "ki_round", "ki_round_xy" } ) )
    {
        if( i.at( "dim" ) != 2 || i.at( "type" ) != "f64" ) throw std::invalid_argument( "unsupported component type" );
        const auto a = V2<double>( i.at( "a" ) );
        if( op == "equals" )
        {
            const auto b = V2<double>( i.at( "b" ) );
            return Value( i.contains( "epsilon" ) ? equals( a, b, Read<double>( i.at( "epsilon" ) ) ) : equals( a, b ) );
        }
        if( op == "ki_round" ) return Value( Wire( KiROUND( a ) ) );
        return Value( Wire( KiROUND( a.x, a.y ) ) );
    }
    if( i.at( "dim" ).is_number_integer() && i.at( "dim" ) == 2 )
    {
        if( i.at( "type" ) == "i32" ) return Run2<int>( i, op );
        if( i.at( "type" ) == "i64" ) return Run2<int64_t>( i, op );
        if( i.at( "type" ) == "f64" ) return Run2<double>( i, op );
    }
    if( i.at( "dim" ).is_number_integer() && i.at( "dim" ) == 3 )
    {
        if( i.at( "type" ) == "i32" ) return Run3<int>( i, op );
        if( i.at( "type" ) == "u32" ) return Run3<unsigned>( i, op );
        if( i.at( "type" ) == "f64" ) return Run3<double>( i, op );
    }
    throw std::invalid_argument( "unsupported component type" );
}
} // namespace
namespace oracle
{
void RegisterVector( REGISTRY& r )
{
    r.Add( "libs.kimath.include.math.application.ui.p002", "libs/kimath/include/math/vector2d.h;libs/kimath/include/math/vector3.h", Run );
}
}
