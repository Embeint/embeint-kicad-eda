// SPDX-License-Identifier: GPL-3.0-or-later
//
// Genuine reference records for kicad_math::glibc_trig.
//
// Usage: probe DOMAIN.bin RECORDS.bin LIBDIR COUNT SIN_OFF COS_OFF SINCOS_OFF
//
// Run only through the locked package's loader (see glibc_trig_reference.py). Before any call
// the probe refuses unless:
//  - every loaded object is the probe itself, the vDSO, or LIBDIR's ld-linux-x86-64.so.2,
//    libc.so.6 or libm.so.6, and all three of those are loaded;
//  - sin, cos and sincos, resolved with dlsym, lie in LIBDIR/libm.so.6 at the given offsets
//    (the locked __sin_sse2, __cos_sse2, __sincos_sse2);
//  - the MXCSR control bits are 0x1f80 (round to nearest, all exceptions masked, no
//    flush-to-zero or denormals-are-zero) and the x87 control word is 0x037f.
// Exception status flags raised by the calls are ordinary and ignored; the control bits are
// checked again after the last call. DOMAIN.bin must hold exactly COUNT little-endian u64
// inputs. For each input, in order, the probe writes five little-endian u64 words:
// x, sin(x), cos(x), and the two outputs of sincos(x).
//
// Inputs pass through a volatile object and the calls go through the resolved pointers, so
// the compiler neither folds nor fuses them and signalling-NaN bits reach the library.
// Build: gcc -O0 -std=c11 -Wall -Wextra -Werror -o probe probe.c

#define _GNU_SOURCE
#include <dlfcn.h>
#include <limits.h>
#include <link.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <xmmintrin.h>

typedef double ( *UNARY )( double );
typedef void ( *SINCOS )( double, double*, double* );

static volatile uint64_t g_bits;
static char g_self[PATH_MAX];
static char g_expected[3][PATH_MAX]; // loader, libc, libm
static int  g_seen[3];
static int  g_bad;

static void fail( const char* aWhat, const char* aDetail )
{
    fprintf( stderr, "probe: refused: %s %s\n", aWhat, aDetail ? aDetail : "" );
    exit( 3 );
}

static double from_bits( uint64_t aBits )
{
    double value;
    memcpy( &value, &aBits, sizeof value );
    return value;
}

static uint64_t to_bits( double aValue )
{
    uint64_t bits;
    memcpy( &bits, &aValue, sizeof bits );
    return bits;
}

static void check_control( const char* aWhen )
{
    unsigned int   mxcsr = _mm_getcsr();
    unsigned short x87 = 0;
    __asm__ volatile( "fnstcw %0" : "=m"( x87 ) );

    if( ( mxcsr & ~0x3fu ) != 0x1f80u || x87 != 0x037f )
    {
        char detail[96];
        snprintf( detail, sizeof detail, "%s: mxcsr 0x%x x87 0x%x", aWhen, mxcsr, x87 );
        fail( "floating-point control mode", detail );
    }
}

static int object( struct dl_phdr_info* aInfo, size_t aSize, void* aData )
{
    (void) aSize;
    (void) aData;
    const char* name = aInfo->dlpi_name;
    char        real[PATH_MAX];

    if( !name || !*name || strncmp( name, "linux-vdso", 10 ) == 0 )
        return 0;

    if( !realpath( name, real ) )
    {
        fprintf( stderr, "probe: unresolvable loaded object %s\n", name );
        g_bad = 1;
        return 0;
    }

    if( strcmp( real, g_self ) == 0 )
        return 0;

    for( int i = 0; i < 3; ++i )
    {
        if( strcmp( real, g_expected[i] ) == 0 )
        {
            g_seen[i] = 1;
            printf( "loaded %s\n", real );
            return 0;
        }
    }

    fprintf( stderr, "probe: unexpected loaded object %s\n", real );
    g_bad = 1;
    return 0;
}

static void* resolve( const char* aName, const char* aOffset )
{
    void*         symbol = dlsym( RTLD_DEFAULT, aName );
    Dl_info       info;
    char          real[PATH_MAX];
    unsigned long want = strtoul( aOffset, NULL, 0 );

    if( !symbol || !dladdr( symbol, &info ) || !info.dli_fbase || !info.dli_fname
        || !realpath( info.dli_fname, real ) )
        fail( "cannot resolve", aName );

    unsigned long offset = (unsigned long) ( (uintptr_t) symbol - (uintptr_t) info.dli_fbase );

    if( strcmp( real, g_expected[2] ) != 0 || offset != want )
    {
        char detail[PATH_MAX + 64];
        snprintf( detail, sizeof detail, "%s at %s+0x%lx, expected %s+0x%lx", aName, real, offset,
                  g_expected[2], want );
        fail( "dispatch", detail );
    }

    printf( "%s %s+0x%lx\n", aName, real, offset );
    return symbol;
}

int main( int argc, char** argv )
{
    if( argc != 8 )
    {
        fprintf( stderr, "usage: %s DOMAIN.bin RECORDS.bin LIBDIR COUNT SIN_OFF COS_OFF SINCOS_OFF\n",
                 argv[0] );
        return 2;
    }

    const char* names[3] = { "ld-linux-x86-64.so.2", "libc.so.6", "libm.so.6" };

    if( !realpath( argv[0], g_self ) )
        fail( "cannot resolve own path", argv[0] );

    for( int i = 0; i < 3; ++i )
    {
        char path[PATH_MAX];
        snprintf( path, sizeof path, "%s/%s", argv[3], names[i] );

        if( !realpath( path, g_expected[i] ) )
            fail( "missing", path );
    }

    dl_iterate_phdr( object, NULL );

    if( g_bad || !g_seen[0] || !g_seen[1] || !g_seen[2] )
        fail( "loaded objects", "(the locked loader, libc and libm must be the only libraries)" );

    check_control( "before" );
    UNARY  sinPtr = (UNARY) resolve( "sin", argv[5] );
    UNARY  cosPtr = (UNARY) resolve( "cos", argv[6] );
    SINCOS sincosPtr = (SINCOS) resolve( "sincos", argv[7] );

    const unsigned long long count = strtoull( argv[4], NULL, 10 );
    struct stat              st;

    if( stat( argv[1], &st ) != 0 || (unsigned long long) st.st_size != count * 8ULL )
        fail( "domain size", argv[1] );

    FILE* in = fopen( argv[1], "rb" );
    FILE* out = fopen( argv[2], "wb" );

    if( !in || !out )
        fail( "cannot open", argv[1] );

    uint64_t           bits;
    unsigned long long written = 0;

    while( fread( &bits, sizeof bits, 1, in ) == 1 )
    {
        double s = 0.0, c = 0.0;
        g_bits = bits;
        const double x = from_bits( g_bits );
        sincosPtr( x, &s, &c );
        const uint64_t record[5] = { bits, to_bits( sinPtr( x ) ), to_bits( cosPtr( x ) ),
                                     to_bits( s ), to_bits( c ) };

        if( fwrite( record, sizeof record, 1, out ) != 1 )
            fail( "write", argv[2] );

        ++written;
    }

    if( ferror( in ) || fclose( in ) != 0 || fclose( out ) != 0 || written != count )
        fail( "record count or i/o", argv[2] );

    check_control( "after" );
    printf( "records %llu\n", written );
    return 0;
}
