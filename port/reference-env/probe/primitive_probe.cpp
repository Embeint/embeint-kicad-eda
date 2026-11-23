// SPDX-License-Identifier: GPL-3.0-or-later
// Bounded observation of the runtime behaviour the oracle's results depend on:
// libm dispatch and sin/cos/sincos bits, a fixed-seed angle sweep, primitive
// mixed-NaN arithmetic, floating-point control state and libstdc++/libc number
// formatting. It prints observations only; run_probe.py and
// compare_observations.py report differences without accepting any.
//
// Usage: primitive_probe --functions sin,cos,... --sweep-count N --sweep-out FILE

#include "probe_common.h"

#include <cfenv>
#include <cmath>
#include <cstdlib>
#include <dlfcn.h>
#include <gnu/libc-version.h>
#include <iomanip>
#include <link.h>
#include <locale>
#include <sstream>
#include <string>
#include <vector>
#include <xmmintrin.h>

using probe::HexDouble;
using probe::JSON;

namespace
{

using UNARY = double ( * )( double );
using SINCOS = void ( * )( double, double*, double* );

// Opaque operands: volatile loads keep the compiler from folding the operations.
volatile uint64_t g_operands[2];

__attribute__( ( noinline ) ) double Add( double aA, double aB ) { return aA + aB; }
__attribute__( ( noinline ) ) double Sub( double aA, double aB ) { return aA - aB; }
__attribute__( ( noinline ) ) double Mul( double aA, double aB ) { return aA * aB; }
__attribute__( ( noinline ) ) double Div( double aA, double aB ) { return aA / aB; }

/// The shape of MATRIX3x3::SetRotation's libm use, as this compiler emits it
/// (GCC may combine the pair into one sincos call).
__attribute__( ( noinline ) ) void CompiledPair( double aAngle, double* aCos, double* aSin )
{
    *aCos = cos( aAngle );
    *aSin = sin( aAngle );
}

void* Resolve( const char* aName )
{
    void* symbol = dlsym( RTLD_DEFAULT, aName );

    if( !symbol )
    {
        std::fprintf( stderr, "dlsym(%s) failed: %s\n", aName, dlerror() );
        std::exit( 1 );
    }

    return symbol;
}

void Macro( JSON& aOut, const char* aName, bool aDefined, const char* aValue )
{
    aOut.Begin( '{' );
    aOut.Field( "name", aName );
    aOut.FieldRaw( "defined", aDefined ? "true" : "false" );
    aOut.Field( "value", aValue );
    aOut.End( '}' );
}

#define STR2( x ) #x
#define STR( x ) STR2( x )
#ifdef __SSE2__
#define M_SSE2 true, STR( __SSE2__ )
#else
#define M_SSE2 false, ""
#endif
#ifdef __AVX__
#define M_AVX true, STR( __AVX__ )
#else
#define M_AVX false, ""
#endif
#ifdef __FMA__
#define M_FMA true, STR( __FMA__ )
#else
#define M_FMA false, ""
#endif
#ifdef __FAST_MATH__
#define M_FAST true, STR( __FAST_MATH__ )
#else
#define M_FAST false, ""
#endif
#ifdef __FP_FAST_FMA
#define M_FPFMA true, STR( __FP_FAST_FMA )
#else
#define M_FPFMA false, ""
#endif
#ifdef __NO_MATH_ERRNO__
#define M_NOERRNO true, STR( __NO_MATH_ERRNO__ )
#else
#define M_NOERRNO false, ""
#endif

void Build( JSON& aOut )
{
    aOut.Key( "build" );
    aOut.Begin( '{' );
    aOut.Field( "compiler", __VERSION__ );
    aOut.Field( "glibcxx_release", STR( _GLIBCXX_RELEASE ) );
    aOut.Field( "glibcxx_header_date", STR( __GLIBCXX__ ) );
    aOut.Field( "glibc_headers", STR( __GLIBC__ ) "." STR( __GLIBC_MINOR__ ) );
    aOut.Field( "glibc_runtime", gnu_get_libc_version() );
    aOut.Field( "flt_eval_method", STR( __FLT_EVAL_METHOD__ ) );
    aOut.Field( "gcc_iec_559", STR( __GCC_IEC_559 ) );
    aOut.Key( "macros" );
    aOut.Begin( '[' );
    Macro( aOut, "__SSE2__", M_SSE2 );
    Macro( aOut, "__AVX__", M_AVX );
    Macro( aOut, "__FMA__", M_FMA );
    Macro( aOut, "__FAST_MATH__", M_FAST );
    Macro( aOut, "__FP_FAST_FMA", M_FPFMA );
    Macro( aOut, "__NO_MATH_ERRNO__", M_NOERRNO );
    aOut.End( ']' );
    aOut.End( '}' );
}

int CollectObject( struct dl_phdr_info* aInfo, size_t, void* aData )
{
    auto* names = static_cast<std::vector<std::string>*>( aData );

    if( aInfo->dlpi_name && aInfo->dlpi_name[0] )
        names->push_back( aInfo->dlpi_name );

    return 0;
}

void Runtime( JSON& aOut )
{
    std::vector<std::string> names;
    dl_iterate_phdr( CollectObject, &names );
    aOut.Key( "loaded_objects" );
    aOut.Begin( '[' );

    for( const std::string& name : names )
        aOut.Value( name );

    aOut.End( ']' );

    aOut.Key( "floating_point_state" );
    aOut.Begin( '{' );
    unsigned short x87 = 0;
    __asm__ volatile( "fnstcw %0" : "=m"( x87 ) );
    char buffer[16];
    std::snprintf( buffer, sizeof buffer, "%08x", _mm_getcsr() );
    aOut.Field( "mxcsr", buffer );
    std::snprintf( buffer, sizeof buffer, "%04x", x87 );
    aOut.Field( "x87_control", buffer );
    aOut.FieldRaw( "round_to_nearest", fegetround() == FE_TONEAREST ? "true" : "false" );
    aOut.Field( "global_locale", std::locale().name() );
    aOut.End( '}' );
}

void Dispatch( JSON& aOut, const std::vector<std::string>& aFunctions )
{
    aOut.Key( "libm_dispatch" );
    aOut.Begin( '[' );

    for( const std::string& name : aFunctions )
    {
        void*   symbol = Resolve( name.c_str() );
        Dl_info info{};

        if( !dladdr( symbol, &info ) || !info.dli_fbase )
        {
            std::fprintf( stderr, "dladdr(%s) failed\n", name.c_str() );
            std::exit( 1 );
        }

        char offset[24];
        std::snprintf( offset, sizeof offset, "0x%lx",
                       static_cast<unsigned long>( reinterpret_cast<uintptr_t>( symbol )
                                                   - reinterpret_cast<uintptr_t>( info.dli_fbase ) ) );
        aOut.Begin( '{' );
        aOut.Field( "function", name );
        aOut.Field( "object", info.dli_fname ? info.dli_fname : "" );
        aOut.Field( "offset", offset );
        aOut.End( '}' );
    }

    aOut.End( ']' );
}

void Libm( JSON& aOut, UNARY aSin, UNARY aCos, SINCOS aSincos )
{
    aOut.Key( "libm_values" );
    aOut.Begin( '[' );

    for( uint64_t bits : probe::LIBM_INPUTS )
    {
        g_operands[0] = bits;
        const double x = probe::FromBits( g_operands[0] );
        double       s = 0.0, c = 0.0, pairCos = 0.0, pairSin = 0.0;
        aSincos( x, &s, &c );
        CompiledPair( x, &pairCos, &pairSin );
        aOut.Begin( '{' );
        aOut.Field( "x", probe::Hex( bits ) );
        aOut.Field( "sin", HexDouble( aSin( x ) ) );
        aOut.Field( "cos", HexDouble( aCos( x ) ) );
        aOut.Field( "sincos_sin", HexDouble( s ) );
        aOut.Field( "sincos_cos", HexDouble( c ) );
        aOut.Field( "compiled_cos", HexDouble( pairCos ) );
        aOut.Field( "compiled_sin", HexDouble( pairSin ) );
        aOut.End( '}' );
    }

    aOut.End( ']' );
}

/// Binary sweep record per angle: x, sin(x), cos(x) as little-endian u64 bits.
void Sweep( JSON& aOut, UNARY aSin, UNARY aCos, uint64_t aCount, const char* aPath )
{
    FILE* file = std::fopen( aPath, "wb" );

    if( !file )
    {
        std::perror( aPath );
        std::exit( 1 );
    }

    probe::SPLITMIX rng{ probe::SWEEP_SEED };

    for( uint64_t i = 0; i < aCount; ++i )
    {
        const double   x = probe::SweepAngle( rng, i );
        const uint64_t record[3] = { probe::Bits( x ), probe::Bits( aSin( x ) ),
                                     probe::Bits( aCos( x ) ) };

        if( std::fwrite( record, sizeof record, 1, file ) != 1 )
        {
            std::perror( aPath );
            std::exit( 1 );
        }
    }

    if( std::fclose( file ) != 0 )
    {
        std::perror( aPath );
        std::exit( 1 );
    }

    aOut.Key( "sweep" );
    aOut.Begin( '{' );
    aOut.Field( "seed", probe::Hex( probe::SWEEP_SEED ) );
    aOut.FieldRaw( "count", std::to_string( aCount ) );
    aOut.Field( "record", "x,sin,cos as little-endian u64" );
    aOut.End( '}' );
}

void NanArithmetic( JSON& aOut )
{
    aOut.Key( "nan_arithmetic" );
    aOut.Begin( '[' );

    for( uint64_t a : probe::NAN_OPERANDS )
    {
        for( uint64_t b : probe::NAN_OPERANDS )
        {
            g_operands[0] = a;
            g_operands[1] = b;
            const double x = probe::FromBits( g_operands[0] );
            const double y = probe::FromBits( g_operands[1] );
            aOut.Begin( '{' );
            aOut.Field( "a", probe::Hex( a ) );
            aOut.Field( "b", probe::Hex( b ) );
            aOut.Field( "add", HexDouble( Add( x, y ) ) );
            aOut.Field( "sub", HexDouble( Sub( x, y ) ) );
            aOut.Field( "mul", HexDouble( Mul( x, y ) ) );
            aOut.Field( "div", HexDouble( Div( x, y ) ) );
            aOut.End( '}' );
        }
    }

    aOut.End( ']' );
}

struct FORMAT
{
    const char*        name;
    std::ios::fmtflags flags;
    int                precision;
};

void Formatting( JSON& aOut )
{
    const uint64_t values[] = {
        0x7ff0000000000000ULL, 0xfff0000000000000ULL, 0x7ff8000000000000ULL,
        0xfff8000000000000ULL, 0x7ff8000000000123ULL, 0x0000000000000000ULL,
        0x8000000000000000ULL, 0x3fb999999999999aULL, 0x7e37e43c8800759cULL,
        0x0000000000000001ULL, 0x40fe240c9fbe76c9ULL, 0x3fd5555555555555ULL,
    };
    const FORMAT formats[] = {
        { "default", std::ios::fmtflags(), 6 },
        { "fixed", std::ios::fixed, 6 },
        { "scientific", std::ios::scientific, 6 },
        { "hexfloat", std::ios::fixed | std::ios::scientific, 6 },
        { "uppercase_default", std::ios::uppercase, 6 },
        { "uppercase_fixed", std::ios::uppercase | std::ios::fixed, 6 },
        { "uppercase_scientific", std::ios::uppercase | std::ios::scientific, 6 },
        { "uppercase_hexfloat", std::ios::uppercase | std::ios::fixed | std::ios::scientific, 6 },
        { "showpos_fixed", std::ios::showpos | std::ios::fixed, 6 },
        { "precision17_default", std::ios::fmtflags(), 17 },
        { "precision0_fixed", std::ios::fixed, 0 },
    };
    const char* printfFormats[] = { "%f", "%F", "%e", "%E", "%g", "%G", "%a", "%A", "%.17g" };

    aOut.Key( "stream_format" );
    aOut.Begin( '[' );

    for( uint64_t bits : values )
    {
        const double value = probe::FromBits( bits );
        aOut.Begin( '{' );
        aOut.Field( "x", probe::Hex( bits ) );
        aOut.Key( "ostream" );
        aOut.Begin( '{' );

        for( const FORMAT& format : formats )
        {
            std::ostringstream stream;
            stream.flags( format.flags );
            stream.precision( format.precision );
            stream << value;
            aOut.Field( format.name, stream.str() );
        }

        aOut.End( '}' );
        aOut.Key( "printf" );
        aOut.Begin( '{' );

        for( const char* format : printfFormats )
        {
            char buffer[512];
            std::snprintf( buffer, sizeof buffer, format, value );
            aOut.Field( format, buffer );
        }

        aOut.End( '}' );
        aOut.End( '}' );
    }

    aOut.End( ']' );
}

std::vector<std::string> Split( const char* aText )
{
    std::vector<std::string> parts;
    std::string              current;

    for( const char* p = aText; *p; ++p )
    {
        if( *p == ',' )
        {
            parts.push_back( current );
            current.clear();
        }
        else
        {
            current.push_back( *p );
        }
    }

    if( !current.empty() )
        parts.push_back( current );

    return parts;
}

} // namespace

int main( int argc, char** argv )
{
    std::vector<std::string> functions;
    uint64_t                 sweepCount = 0;
    const char*              sweepOut = nullptr;

    for( int i = 1; i + 1 < argc; i += 2 )
    {
        const std::string option = argv[i];

        if( option == "--functions" )
            functions = Split( argv[i + 1] );
        else if( option == "--sweep-count" )
            sweepCount = std::strtoull( argv[i + 1], nullptr, 10 );
        else if( option == "--sweep-out" )
            sweepOut = argv[i + 1];
        else
            argc = 0;
    }

    // The sweep is bounded: 2,000,000 angles is 48 MB of exported bits.
    if( argc % 2 != 1 || functions.empty() || !sweepOut || sweepCount == 0 || sweepCount > 2000000 )
    {
        std::fprintf( stderr, "usage: primitive_probe --functions LIST --sweep-count 1..2000000 "
                              "--sweep-out FILE\n" );
        return 2;
    }

    const UNARY  sinPtr = reinterpret_cast<UNARY>( Resolve( "sin" ) );
    const UNARY  cosPtr = reinterpret_cast<UNARY>( Resolve( "cos" ) );
    const SINCOS sincosPtr = reinterpret_cast<SINCOS>( Resolve( "sincos" ) );

    JSON out( stdout );
    out.Begin( '{' );
    out.FieldRaw( "schema", "1" );
    out.Field( "probe", "primitive" );
    Build( out );
    Runtime( out );
    Dispatch( out, functions );
    Libm( out, sinPtr, cosPtr, sincosPtr );
    Sweep( out, sinPtr, cosPtr, sweepCount, sweepOut );
    NanArithmetic( out );
    Formatting( out );
    out.End( '}' );
    std::fputc( '\n', stdout );
    return std::fflush( stdout ) == 0 ? 0 : 1;
}
