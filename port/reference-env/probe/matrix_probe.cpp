// SPDX-License-Identifier: GPL-3.0-or-later
// Mixed-NaN operand behaviour of the unchanged pinned MATRIX3x3<double> templates
// (libs/kimath/include/math/matrix3x3.h at the KiCad pin), compiled by
// run_probe.py with the flags of an actual oracle shim translation unit from the
// audited compile database. Which NaN payload survives depends on the operand
// order this compiler emits, so these are observations of this toolchain on the
// pinned code, not a statement of KiCad semantics. Inlining context can differ
// from a unit's own oracle shim; a unit must re-measure through its shim.

#include "probe_common.h"

#include <math/matrix3x3.h>

#include <sstream>
#include <vector>

using probe::HexDouble;
using probe::JSON;

namespace
{

constexpr uint64_t MATRIX_SEED = 0x4b694361644d6174ULL; // "KiCadMat"
constexpr int      CASES = 96;

const double FINITE[] = { 1.0, -2.0, 0.5, 3.25, -0.75, 4.0, 1e-3, -1e3, 0.0 };

const uint64_t PAYLOADS[] = { 0x7ff8000000000011ULL, 0x7ff8000000000022ULL, 0xfff8000000000033ULL,
                              0x7ff8000000000044ULL, 0x7ff0000000000055ULL };

volatile uint64_t g_sink;

double Opaque( uint64_t aBits )
{
    g_sink = aBits;
    return probe::FromBits( g_sink );
}

double Pick( probe::SPLITMIX& aRng, bool aAllowNan )
{
    const uint64_t r = aRng.Next();

    if( aAllowNan && r % 4 == 0 )
        return Opaque( PAYLOADS[( r >> 8 ) % 5] );

    return Opaque( probe::Bits( FINITE[( r >> 16 ) % 9] ) );
}

MATRIX3x3<double> Matrix( probe::SPLITMIX& aRng )
{
    MATRIX3x3<double> m;

    for( auto& row : m.m_data )
    {
        for( double& value : row )
            value = Pick( aRng, false );
    }

    // Two or three distinct payloads at distinct positions.
    const int count = 2 + static_cast<int>( aRng.Next() % 2 );
    bool      used[9] = {};
    int       payload = static_cast<int>( aRng.Next() % 5 );

    for( int placed = 0; placed < count; )
    {
        const int position = static_cast<int>( aRng.Next() % 9 );

        if( used[position] )
            continue;

        used[position] = true;
        m.m_data[position / 3][position % 3] = Opaque( PAYLOADS[payload] );
        payload = ( payload + 1 ) % 5;
        ++placed;
    }

    return m;
}

void Emit( JSON& aOut, const char* aKey, const MATRIX3x3<double>& aMatrix )
{
    aOut.Key( aKey );
    aOut.Begin( '[' );

    for( const auto& row : aMatrix.m_data )
    {
        for( double value : row )
            aOut.Value( HexDouble( value ) );
    }

    aOut.End( ']' );
}

} // namespace

int main()
{
    probe::SPLITMIX rng{ MATRIX_SEED };
    JSON            out( stdout );
    out.Begin( '{' );
    out.FieldRaw( "schema", "1" );
    out.Field( "probe", "matrix" );
    out.Field( "compiler", __VERSION__ );
    out.Key( "cases" );
    out.Begin( '[' );

    for( int i = 0; i < CASES; ++i )
    {
        const MATRIX3x3<double> a = Matrix( rng );
        const MATRIX3x3<double> b = Matrix( rng );
        // Sequential draws: argument evaluation order is unspecified in C++.
        double draws[6];

        for( double& draw : draws )
            draw = Pick( rng, true );

        const VECTOR2<double> v2( draws[0], draws[1] );
        const VECTOR3<double> v3( draws[2], draws[3], draws[4] );
        const double          scalar = draws[5];

        out.Begin( '{' );
        out.FieldRaw( "case", std::to_string( i ) );
        Emit( out, "a", a );
        Emit( out, "b", b );
        out.Key( "v2" );
        out.Begin( '[' );
        out.Value( HexDouble( v2.x ) );
        out.Value( HexDouble( v2.y ) );
        out.End( ']' );
        out.Key( "v3" );
        out.Begin( '[' );
        out.Value( HexDouble( v3.x ) );
        out.Value( HexDouble( v3.y ) );
        out.Value( HexDouble( v3.z ) );
        out.End( ']' );
        out.Field( "scalar", HexDouble( scalar ) );

        out.Field( "determinant", HexDouble( a.Determinant() ) );
        Emit( out, "inverse", a.Inverse() );
        Emit( out, "mul_matrix", a * b );
        const VECTOR2<double> r2 = a * v2;
        out.Key( "mul_vector2" );
        out.Begin( '[' );
        out.Value( HexDouble( r2.x ) );
        out.Value( HexDouble( r2.y ) );
        out.End( ']' );
        const VECTOR3<double> r3 = a * v3;
        out.Key( "mul_vector3" );
        out.Begin( '[' );
        out.Value( HexDouble( r3.x ) );
        out.Value( HexDouble( r3.y ) );
        out.Value( HexDouble( r3.z ) );
        out.End( ']' );
        Emit( out, "mul_scalar_right", a * scalar );
        Emit( out, "mul_scalar_left", scalar * a );
        out.End( '}' );
    }

    out.End( ']' );

    // The pinned operator<< through libstdc++'s runtime num_put.
    out.Key( "stream" );
    out.Begin( '{' );
    probe::SPLITMIX streamRng{ MATRIX_SEED ^ 1 };
    const MATRIX3x3<double> m = Matrix( streamRng );
    const std::pair<const char*, std::ios::fmtflags> flags[] = {
        { "default", std::ios::fmtflags() },
        { "uppercase_fixed", std::ios::uppercase | std::ios::fixed },
        { "uppercase_scientific", std::ios::uppercase | std::ios::scientific },
    };

    for( const auto& [name, value] : flags )
    {
        std::ostringstream stream;
        stream.flags( value );
        stream << m;
        out.Field( name, stream.str() );
    }

    out.End( '}' );
    out.End( '}' );
    std::fputc( '\n', stdout );
    return std::fflush( stdout ) == 0 ? 0 : 1;
}
