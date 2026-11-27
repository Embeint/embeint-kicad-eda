// SPDX-License-Identifier: GPL-3.0-or-later
// Oracle adapter for thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002:
// the pinned, unmodified clipper.h, clipper.minkowski.h, clipper.offset.h, clipper.rectclip.h,
// clipper.triangulation.h and clipper.version.h, compiled with KiCad's public USINGZ
// definition and linked against the pinned clipper2 archive, exactly as KiCad uses them.
//
// Replies come from the genuine inline bodies (reference/clipper2_include_p002_ops.inc,
// compiled here against Clipper2Lib). Undefined behaviour is decided by the same source, not
// by a hand-written model: every request first runs in a forked child on
// reference/clipper2_include_p002_guard.cpp, the same evaluation compiled against the pinned
// clipper.engine.cpp, clipper.offset.cpp and clipper.rectclip.cpp with UBSan in trap mode and
// _GLIBCXX_ASSERTIONS. A trap, failed assertion or fault there refuses the request as
// "undefined behaviour: <op>"; a child that exceeds the replay time or memory limit refuses
// it as "harness limit: <op>" (a harness bound, not a claim about the reference). Only a
// cleanly finished replay lets the genuine call run. Triangulate is declared by
// clipper.triangulation.h but not compiled by KiCad, so it has no request.
#include "registry.h"
#include <clipper2/clipper.h>
#include <clipper2/clipper.version.h>
#include <cerrno>
#include <csignal>
#include <cstdio>
#include <fcntl.h>
#include <iostream>
#include <new>
#include <string>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

#define INCLUDE_P002_NS include_p002_genuine
#include "reference/clipper2_include_p002_ops.inc"

#if defined( CLIPPER2_HI_PRECISION ) || defined( CLIPPER2_MAX_DECIMAL_PRECISION )
#error "KiCad does not define the Clipper2 precision overrides"
#endif
#if !( ( defined( __cpp_exceptions ) && __cpp_exceptions ) || ( defined( __EXCEPTIONS ) && __EXCEPTIONS ) )
#error "KiCad compiles Clipper2 with exceptions"
#endif

namespace include_p002_guard
{
oracle::JSON Evaluate( const oracle::JSON& aIn );
}

namespace
{
using oracle::JSON;

constexpr const char* INCLUDE_P002_UNIT = "thirdparty.clipper2.clipper2lib.include.clipper2.thirdparty.pure.p002";
// Replay limits for the forked UB guard.
constexpr unsigned REPLAY_SECONDS = 20;
constexpr rlim_t   REPLAY_BYTES = rlim_t( 4 ) << 30;


// Run the instrumented replay in a child process: 0 clean (or a malformed request, which the
// parent's own evaluation rejects before any reference call), 1 undefined behaviour, 2 harness
// limit.
int Replay( const JSON& aIn )
{
    std::cout.flush();
    std::cerr.flush();
    std::fflush( nullptr );
    const pid_t pid = fork();

    if( pid < 0 )
        throw std::runtime_error( "replay fork failed" );

    if( pid == 0 )
    {
        const int null = open( "/dev/null", O_WRONLY );

        if( null >= 0 )
        {
            dup2( null, STDOUT_FILENO );
            dup2( null, STDERR_FILENO );
        }

        const rlimit limit{ REPLAY_BYTES, REPLAY_BYTES };
        setrlimit( RLIMIT_AS, &limit );
        alarm( REPLAY_SECONDS );

        try
        {
            include_p002_guard::Evaluate( aIn );
        }
        catch( const std::bad_alloc& )
        {
            _exit( 3 );
        }
        catch( const std::length_error& )
        {
            // A reserve beyond max_size(): a resource bound, like bad_alloc.
            _exit( 3 );
        }
        catch( const std::invalid_argument& )
        {
            // Malformed or guarded request: the parent reports it; every request validates
            // its whole input and applies its guards before calling any reference code.
            _exit( 5 );
        }
        catch( ... )
        {
            _exit( 4 );
        }

        _exit( 0 );
    }

    int status = 0;

    while( waitpid( pid, &status, 0 ) < 0 )
    {
        if( errno != EINTR )
            throw std::runtime_error( "replay wait failed" );
    }

    if( WIFEXITED( status ) )
    {
        switch( WEXITSTATUS( status ) )
        {
        case 0:
        case 5: return 0;
        case 3: return 2;
        default: throw std::runtime_error( "replay failed" );
        }
    }

    if( WIFSIGNALED( status ) )
    {
        switch( WTERMSIG( status ) )
        {
        case SIGILL:
        case SIGTRAP:
        case SIGABRT:
        case SIGSEGV:
        case SIGBUS:
        case SIGFPE: return 1;
        case SIGALRM:
        case SIGKILL: return 2;
        default: break;
        }
    }

    throw std::runtime_error( "replay ended abnormally" );
}


JSON Run( const JSON& aIn )
{
    if( !aIn.is_object() || !aIn.contains( "op" ) || !aIn.at( "op" ).is_string() )
        throw std::invalid_argument( "invalid input: op" );

    const std::string op = aIn.at( "op" ).get<std::string>();

    switch( Replay( aIn ) )
    {
    case 0: return include_p002_genuine::Evaluate( aIn );
    case 1: throw std::invalid_argument( "undefined behaviour: " + op );
    default: throw std::invalid_argument( "harness limit: " + op );
    }
}
} // namespace


namespace oracle
{
void RegisterClipper2IncludeP002( REGISTRY& aRegistry )
{
    aRegistry.Add( INCLUDE_P002_UNIT, "thirdparty/clipper2/Clipper2Lib/include/clipper2/clipper.h",
                   Run );
}
} // namespace oracle
