// SPDX-License-Identifier: GPL-3.0-or-later
// Bounded observation of the runtime behaviour the oracle's results depend on:
// libm dispatch and sin/cos/sincos bits, a fixed-seed angle sweep, primitive
// mixed-NaN arithmetic, floating-point control state and libstdc++/libc number
// formatting. It prints observations only; run_probe.py and
// compare_observations.py report differences without accepting any.
//
// libm dispatch is observed by executing each checked function once in a
// ptrace-traced child (trace_call.h, bounded in steps and wall-clock time) and
// recording every instruction address it runs from the public entry point until
// it returns, the return address and target, and every GOT slot read by a
// RIP-relative jump. dispatch_proof.py validates the chain and every
// control-flow edge against the locked libm; this program draws no conclusion.
//
// Usage: primitive_probe --functions sin,cos,... --sweep-count N --sweep-out FILE

#include "probe_common.h"
#include "trace_call.h"

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

enum class SIGNATURE
{
    D1,     ///< double f( double )
    D2,     ///< double f( double, double )
    D_PAIR, ///< void f( double, double*, double* )
    F1,     ///< float f( float )
    F2,     ///< float f( float, float )
    F_PAIR  ///< void f( float, float*, float* )
};

using D1_FN = double ( * )( double );
using D2_FN = double ( * )( double, double );
using DP_FN = void ( * )( double, double*, double* );
using F1_FN = float ( * )( float );
using F2_FN = float ( * )( float, float );
using FP_FN = void ( * )( float, float*, float* );

/// A checked libm function: the address this program's own reference binds to
/// (as compiled code calls it) and ordinary finite operands for the trace.
struct LIBM_FUNCTION
{
    const char* name;
    SIGNATURE   signature;
    void*       linked;
    double      a;
    double      b;
};

#define LINK( type, f ) reinterpret_cast<void*>( static_cast<type>( &::f ) )

const std::vector<LIBM_FUNCTION>& LibmFunctions()
{
    static const std::vector<LIBM_FUNCTION> functions = {
        { "sin", SIGNATURE::D1, LINK( D1_FN, sin ), 0.5, 0.0 },
        { "cos", SIGNATURE::D1, LINK( D1_FN, cos ), 0.5, 0.0 },
        { "sincos", SIGNATURE::D_PAIR, LINK( DP_FN, sincos ), 0.5, 0.0 },
        { "tan", SIGNATURE::D1, LINK( D1_FN, tan ), 0.5, 0.0 },
        { "atan", SIGNATURE::D1, LINK( D1_FN, atan ), 0.5, 0.0 },
        { "atan2", SIGNATURE::D2, LINK( D2_FN, atan2 ), 0.5, 0.75 },
        { "exp", SIGNATURE::D1, LINK( D1_FN, exp ), 0.5, 0.0 },
        { "log", SIGNATURE::D1, LINK( D1_FN, log ), 1.5, 0.0 },
        { "pow", SIGNATURE::D2, LINK( D2_FN, pow ), 1.5, 2.5 },
        { "sinf", SIGNATURE::F1, LINK( F1_FN, sinf ), 0.5, 0.0 },
        { "cosf", SIGNATURE::F1, LINK( F1_FN, cosf ), 0.5, 0.0 },
        { "sincosf", SIGNATURE::F_PAIR, LINK( FP_FN, sincosf ), 0.5, 0.0 },
        { "expf", SIGNATURE::F1, LINK( F1_FN, expf ), 0.5, 0.0 },
        { "exp2f", SIGNATURE::F1, LINK( F1_FN, exp2f ), 0.5, 0.0 },
        { "logf", SIGNATURE::F1, LINK( F1_FN, logf ), 1.5, 0.0 },
        { "log2f", SIGNATURE::F1, LINK( F1_FN, log2f ), 1.5, 0.0 },
        { "powf", SIGNATURE::F2, LINK( F2_FN, powf ), 1.5, 2.5 },
    };
    return functions;
}

volatile double g_sink;
volatile double g_inputs[2];

/// One call through aEntry with opaque operands, so the call really happens.
__attribute__( ( noinline ) ) void CallOnce( const LIBM_FUNCTION& aFunction, void* aEntry )
{
    g_inputs[0] = aFunction.a;
    g_inputs[1] = aFunction.b;
    const double a = g_inputs[0], b = g_inputs[1];
    const float  af = static_cast<float>( a ), bf = static_cast<float>( b );
    double       first = 0.0, second = 0.0;
    float        firstf = 0.0f, secondf = 0.0f;

    switch( aFunction.signature )
    {
    case SIGNATURE::D1: g_sink = reinterpret_cast<D1_FN>( aEntry )( a ); break;
    case SIGNATURE::D2: g_sink = reinterpret_cast<D2_FN>( aEntry )( a, b ); break;
    case SIGNATURE::D_PAIR:
        reinterpret_cast<DP_FN>( aEntry )( a, &first, &second );
        g_sink = first + second;
        break;
    case SIGNATURE::F1: g_sink = reinterpret_cast<F1_FN>( aEntry )( af ); break;
    case SIGNATURE::F2: g_sink = reinterpret_cast<F2_FN>( aEntry )( af, bf ); break;
    case SIGNATURE::F_PAIR:
        reinterpret_cast<FP_FN>( aEntry )( af, &firstf, &secondf );
        g_sink = firstf + secondf;
        break;
    }
}

/// What the traced child calls: one checked function through its entry.
struct TRACED_CALL
{
    const LIBM_FUNCTION* function;
    void*                entry;
};

void CallTraced( const void* aContext )
{
    const auto* call = static_cast<const TRACED_CALL*>( aContext );
    CallOnce( *call->function, call->entry );
}

/// Set when any traced child could not be reaped; the probe then fails.
bool g_unreaped = false;

/// Object path and offset of an address in the loaded image.
bool Locate( const void* aAddress, std::string& aObject, uintptr_t& aOffset )
{
    Dl_info info{};

    if( !dladdr( aAddress, &info ) || !info.dli_fbase || !info.dli_fname )
        return false;

    aObject = info.dli_fname;
    aOffset = reinterpret_cast<uintptr_t>( aAddress ) - reinterpret_cast<uintptr_t>( info.dli_fbase );
    return true;
}

std::string HexOffset( uintptr_t aValue )
{
    char text[24];
    std::snprintf( text, sizeof text, "0x%lx", static_cast<unsigned long>( aValue ) );
    return text;
}

void Dispatch( JSON& aOut, const std::vector<std::string>& aFunctions )
{
    aOut.Key( "libm_dispatch" );
    aOut.Begin( '[' );

    for( const std::string& name : aFunctions )
    {
        const LIBM_FUNCTION* function = nullptr;

        for( const LIBM_FUNCTION& candidate : LibmFunctions() )
        {
            if( name == candidate.name )
                function = &candidate;
        }

        if( !function )
        {
            std::fprintf( stderr, "no traced signature for libm function %s\n", name.c_str() );
            std::exit( 1 );
        }

        void*       symbol = Resolve( name.c_str() );
        std::string object, linkedObject;
        uintptr_t   offset = 0, linkedOffset = 0;

        if( !Locate( symbol, object, offset ) || !Locate( function->linked, linkedObject, linkedOffset ) )
        {
            std::fprintf( stderr, "dladdr(%s) failed\n", name.c_str() );
            std::exit( 1 );
        }

        const uintptr_t     base = reinterpret_cast<uintptr_t>( symbol ) - offset;
        const TRACED_CALL   call{ function, symbol };
        const trace::RESULT traced = trace::TraceCall( CallTraced, &call, reinterpret_cast<uintptr_t>( symbol ),
                                                       trace::LIMITS() );
        g_unreaped = g_unreaped || !traced.reaped;

        // Addresses in the entry's mapping are offsets; any other is object-qualified.
        auto where = [&]( uintptr_t aAddress ) -> std::string
        {
            std::string found;
            uintptr_t   at = 0;

            if( !Locate( reinterpret_cast<const void*>( aAddress ), found, at ) )
                return "!unmapped+" + HexOffset( aAddress );

            if( found == object && aAddress - at == base )
                return HexOffset( at );

            return "!" + found + "+" + HexOffset( at );
        };

        aOut.Begin( '{' );
        aOut.Field( "function", name );
        aOut.Field( "object", object );
        aOut.Field( "offset", HexOffset( offset ) );
        aOut.Field( "linked_object", linkedObject );
        aOut.Field( "linked_offset", HexOffset( linkedOffset ) );
        aOut.Key( "inputs" );
        aOut.Begin( '[' );
        aOut.Value( HexDouble( function->a ) );
        aOut.Value( HexDouble( function->b ) );
        aOut.End( ']' );
        aOut.Field( "trace_status", traced.status );
        aOut.Field( "return_address", where( traced.returnAddress ) );
        aOut.Field( "return_to", where( traced.returnTo ) );
        aOut.Key( "trace" );
        aOut.Begin( '[' );

        for( uintptr_t address : traced.addresses )
            aOut.Value( where( address ) );

        aOut.End( ']' );
        aOut.Key( "slot_reads" );
        aOut.Begin( '[' );

        for( const trace::SLOT_READ& read : traced.slotReads )
        {
            aOut.Begin( '{' );
            aOut.Field( "at", where( read.at ) );
            aOut.Field( "slot", where( read.slot ) );
            aOut.Field( "value", where( read.value ) );
            aOut.End( '}' );
        }

        aOut.End( ']' );
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
    out.FieldRaw( "schema", "2" );
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
    if( g_unreaped )
    {
        std::fprintf( stderr, "a traced child could not be reaped\n" );
        return 1;
    }

    return std::fflush( stdout ) == 0 ? 0 : 1;
}
