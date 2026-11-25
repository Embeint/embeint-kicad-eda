// SPDX-License-Identifier: GPL-3.0-or-later
// Fresh glibc 2.39 angle reference. See README.md and reference.py.
// Reuses glibc-trig's locked-loader, loaded-object and FP-mode checks.
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
typedef double ( *BINARY )( double, double );

static volatile uint64_t g_bits[2];
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
        char detail[2 * PATH_MAX + 128];
        snprintf( detail, sizeof detail, "%s at %s+0x%lx, expected %s+0x%lx", aName, real, offset,
                  g_expected[2], want );
        fail( "dispatch", detail );
    }

    printf( "%s %s+0x%lx\n", aName, real, offset );
    return symbol;
}

// The public inverse functions are wrappers, not IFUNC exports. Confirm the
// actual already-resolved GOT slot that each wrapper calls in the locked ELF.
static void check_slot( const char* name, void* wrapper, unsigned long slot, unsigned long kernel )
{
    Dl_info info;
    void* target = NULL;
    if( !dladdr( wrapper, &info ) )
        fail( "wrapper base", name );
    memcpy( &target, (char*) info.dli_fbase + slot, sizeof target );
    if( (uintptr_t) target - (uintptr_t) info.dli_fbase != kernel )
        fail( "wrapper IFUNC GOT slot", name );
    printf( "%s GOT+0x%lx -> libm+0x%lx\n", name, slot, kernel );
}

int main( int argc, char** argv )
{
    if( argc != 5 )
        fail( "usage", "DOMAIN RECORDS LIBDIR COUNT" );
    const char* names[3] = { "ld-linux-x86-64.so.2", "libc.so.6", "libm.so.6" };
    if( !realpath( argv[0], g_self ) )
        fail( "own path", argv[0] );
    for( int i = 0; i < 3; ++i )
    {
        char path[PATH_MAX];
        snprintf( path, sizeof path, "%s/%s", argv[3], names[i] );
        if( !realpath( path, g_expected[i] ) )
            fail( "missing", path );
    }
    dl_iterate_phdr( object, NULL );
    if( g_bad || !g_seen[0] || !g_seen[1] || !g_seen[2] )
        fail( "loaded objects", "expected only locked loader/libc/libm" );
    check_control( "before" );
    // The public asin/acos wrappers use k_standard's positive NAN in POSIX
    // mode. Pin the packaged default mode rather than assuming IEEE bypass.
    void* libVersion = dlvsym( RTLD_DEFAULT, "_LIB_VERSION", "GLIBC_2.2.5" );
    Dl_info modeInfo;
    int mode = -99;
    if( !libVersion || !dladdr( libVersion, &modeInfo )
        || (uintptr_t) libVersion - (uintptr_t) modeInfo.dli_fbase != 0xe8008 )
        fail( "_LIB_VERSION symbol", NULL );
    memcpy( &mode, libVersion, sizeof mode );
    if( mode != 2 ) fail( "_LIB_VERSION", "expected _POSIX_ (2)" );
    printf( "_LIB_VERSION _POSIX_ (2) at libm+0xe8008\n" );
    UNARY atanPtr = (UNARY) resolve( "atan", "0x2d840" );
    UNARY tanPtr = (UNARY) resolve( "tan", "0x30b80" );
    UNARY asinPtr = (UNARY) resolve( "asin", "0x12a40" );
    UNARY acosPtr = (UNARY) resolve( "acos", "0x129a0" );
    BINARY atan2Ptr = (BINARY) resolve( "atan2", "0x12aa0" );
    // Force lazy PLT binding before inspecting the kernel slots.
    volatile double one = 1.0;
    asinPtr( one ); acosPtr( one ); atan2Ptr( one, one );
    check_slot( "asin", (void*) asinPtr, 0xe7f20, 0x25f70 );
    check_slot( "acos", (void*) acosPtr, 0xe7f38, 0x26780 );
    check_slot( "atan2", (void*) atan2Ptr, 0xe7ed0, 0x26fe0 );
    const unsigned long long count = strtoull( argv[4], NULL, 10 );
    struct stat st;
    if( stat( argv[1], &st ) != 0 || (unsigned long long) st.st_size != count * 16ULL )
        fail( "domain size", argv[1] );
    FILE* in = fopen( argv[1], "rb" );
    FILE* out = fopen( argv[2], "wb" );
    if( !in || !out ) fail( "cannot open", argv[1] );
    uint64_t inputs[2];
    unsigned long long written = 0;
    while( fread( inputs, sizeof inputs, 1, in ) == 1 )
    {
        g_bits[0] = inputs[0]; g_bits[1] = inputs[1];
        const double x = from_bits( g_bits[0] ), y = from_bits( g_bits[1] );
        const uint64_t record[7] = { inputs[0], inputs[1], to_bits( atanPtr(x) ),
            to_bits( atan2Ptr(y,x) ), to_bits( tanPtr(x) ), to_bits( asinPtr(x) ), to_bits( acosPtr(x) ) };
        if( fwrite( record, sizeof record, 1, out ) != 1 ) fail( "write", argv[2] );
        ++written;
    }
    if( ferror( in ) || fclose( in ) != 0 || fclose( out ) != 0 || written != count )
        fail( "record count or i/o", argv[2] );
    check_control( "after" );
    printf( "records %llu\n", written );
    return 0;
}
