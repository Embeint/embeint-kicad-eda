// SPDX-License-Identifier: GPL-3.0-or-later
// Test harness for probe/trace_call.h: traces probe-local callees that return,
// block, exit or abort, and reports the tracer's result and child cleanup as
// JSON. Used only by tests/test_dispatch.py (LifecycleTest).
//
// Usage: trace_lifecycle_harness SCENARIO TOTAL_MS [--announce]

#include "trace_call.h"

#include <cerrno>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dirent.h>
#include <fcntl.h>
#include <string>
#include <unistd.h>
#include <vector>

namespace
{

enum class BEHAVIOUR
{
    RETURN,
    BLOCK_ALWAYS,
    BLOCK_TRACED,
    EXIT_TRACED,
    ABORT_TRACED,
    EXIT_WARM_UP,
    LOOP
};

BEHAVIOUR    g_behaviour = BEHAVIOUR::RETURN;
bool         g_announce = false;
int          g_calls = 0; // per process: the child's first call is the untraced warm-up
volatile int g_sink = 0;
bool         g_simulateLost = false;
bool         g_waitLost = false;
int          g_killsAfterLost = 0;

__attribute__( ( noinline ) ) int Callee( int aValue )
{
    const bool traced = ++g_calls == 2;

    switch( g_behaviour )
    {
    case BEHAVIOUR::RETURN: break;
    case BEHAVIOUR::BLOCK_ALWAYS:
        for( ;; )
            pause();
    case BEHAVIOUR::BLOCK_TRACED:
        if( traced )
        {
            if( g_announce )
            {
                std::printf( "{\"child\": %d}\n", static_cast<int>( getpid() ) );
                std::fflush( stdout );
            }

            for( ;; )
                pause();
        }
        break;
    case BEHAVIOUR::EXIT_TRACED:
        if( traced )
            _exit( 7 );
        break;
    case BEHAVIOUR::ABORT_TRACED:
        if( traced )
            abort();
        break;
    case BEHAVIOUR::EXIT_WARM_UP:
        if( !traced )
            _exit( 9 );
        break;
    case BEHAVIOUR::LOOP:
        for( int i = 0; i < 100000; ++i )
            g_sink = g_sink + i;
        break;
    }

    return aValue * 2;
}

int ( *volatile g_callee )( int ) = Callee;

void Call( const void* )
{
    g_sink = g_callee( 21 );
}

/// Child pids of every thread of this process (none should remain).
std::vector<std::string> ChildrenLeft()
{
    std::vector<std::string> children;
    DIR*                     tasks = opendir( "/proc/self/task" );

    for( dirent* entry = tasks ? readdir( tasks ) : nullptr; entry; entry = readdir( tasks ) )
    {
        if( entry->d_name[0] == '.' )
            continue;

        std::string path = std::string( "/proc/self/task/" ) + entry->d_name + "/children";
        FILE*       file = std::fopen( path.c_str(), "r" );
        char        pid[32];

        while( file && std::fscanf( file, "%31s", pid ) == 1 )
            children.push_back( pid );

        if( file )
            std::fclose( file );
    }

    if( tasks )
        closedir( tasks );

    return children;
}

bool Exists( pid_t aChild )
{
    return aChild > 0 && ( kill( aChild, 0 ) == 0 || errno != ESRCH );
}

} // namespace

extern "C" pid_t __real_waitpid( pid_t, int*, int );
extern "C" int __real_kill( pid_t, int );

extern "C" pid_t __wrap_waitpid( pid_t aChild, int* aStatus, int aOptions )
{
    const pid_t got = __real_waitpid( aChild, aStatus, aOptions );

    if( g_simulateLost && got == aChild && ( WIFEXITED( *aStatus ) || WIFSIGNALED( *aStatus ) ) )
    {
        // The real child is already collected: model another reaper taking
        // ownership before the helper can obtain its wait status.
        g_waitLost = true;
        errno = ECHILD;
        return -1;
    }

    return got;
}

extern "C" int __wrap_kill( pid_t aChild, int aSignal )
{
    if( g_waitLost && aSignal == SIGKILL )
    {
        ++g_killsAfterLost;
        errno = ESRCH;
        return -1; // Never send a test signal to a released/recycled pid.
    }

    return __real_kill( aChild, aSignal );
}

int main( int argc, char** argv )
{
    if( argc < 3 )
    {
        std::fprintf( stderr, "usage: %s SCENARIO TOTAL_MS [--announce]\n", argv[0] );
        return 2;
    }

    const std::string scenario = argv[1];
    trace::LIMITS     limits;
    limits.totalMs = std::atol( argv[2] );
    limits.waitMs = limits.totalMs;
    limits.reapMs = 2000;
    g_announce = argc > 3 && std::strcmp( argv[3], "--announce" ) == 0;
    int repeat = 1;

    if( scenario == "success" )
        g_behaviour = BEHAVIOUR::RETURN;
    else if( scenario == "repeat17" )
        repeat = 17;
    else if( scenario == "block-before-entry" )
        g_behaviour = BEHAVIOUR::BLOCK_ALWAYS;
    else if( scenario == "block-in-callee" )
        g_behaviour = BEHAVIOUR::BLOCK_TRACED;
    else if( scenario == "exit-in-callee" )
        g_behaviour = BEHAVIOUR::EXIT_TRACED;
    else if( scenario == "abort-in-callee" )
        g_behaviour = BEHAVIOUR::ABORT_TRACED;
    else if( scenario == "exit-before-entry" )
        g_behaviour = BEHAVIOUR::EXIT_WARM_UP;
    else if( scenario == "step-bound" )
    {
        g_behaviour = BEHAVIOUR::LOOP;
        limits.maxSteps = 5000;
    }
    else if( scenario == "auto-ignore" || scenario == "auto-nocldwait" )
    {
        struct sigaction disposition{};
        disposition.sa_handler = scenario == "auto-ignore" ? SIG_IGN : SIG_DFL;
        disposition.sa_flags = scenario == "auto-nocldwait" ? SA_NOCLDWAIT : 0;
        sigemptyset( &disposition.sa_mask );
        sigaction( SIGCHLD, &disposition, nullptr );
    }
    else if( scenario == "lost-child" )
    {
        g_simulateLost = true;
        g_behaviour = BEHAVIOUR::EXIT_WARM_UP;
    }
    else if( scenario == "full-output" )
    {
        int descriptors[2];

        if( pipe( descriptors ) != 0 )
            return 3;

        fcntl( descriptors[1], F_SETFL, O_NONBLOCK );
        char filling[4096]{};

        while( write( descriptors[1], filling, sizeof filling ) > 0 )
        {}

        fcntl( descriptors[1], F_SETFL, 0 );
        dup2( descriptors[1], STDOUT_FILENO );
        static char buffered[4096];
        setvbuf( stdout, buffered, _IOFBF, sizeof buffered );
        std::fputs( "pending buffered output", stdout );
    }
    else
    {
        std::fprintf( stderr, "unknown scenario %s\n", scenario.c_str() );
        return 2;
    }

    std::vector<trace::RESULT> results;

    for( int i = 0; i < repeat; ++i )
        results.push_back( trace::TraceCall( Call, nullptr, reinterpret_cast<uintptr_t>( &Callee ), limits ) );

    bool reaped = true, exists = false;

    for( const trace::RESULT& result : results )
    {
        reaped = reaped && result.reaped;
        exists = exists || Exists( result.child );
    }

    const trace::RESULT& last = results.back();

    if( scenario == "full-output" )
    {
        // Report without flushing the deliberately blocked stdout stream.
        dprintf( STDERR_FILENO, "returned=%d reaped=%d children=%zu\n",
                 last.status == "returned", last.reaped, ChildrenLeft().size() );
        _exit( last.status == "returned" && last.reaped && ChildrenLeft().empty() ? 0 : 4 );
    }

    std::printf( "{\"status\": \"%s\", \"steps\": %zu, \"return_address\": %lu, \"return_to\": %lu, "
                 "\"child\": %d, \"reaped\": %s, \"child_exists\": %s, \"kills_after_lost\": %d, \"children_left\": [",
                 last.status.c_str(), last.addresses.size(), static_cast<unsigned long>( last.returnAddress ),
                 static_cast<unsigned long>( last.returnTo ), static_cast<int>( last.child ),
                 reaped ? "true" : "false", exists ? "true" : "false", g_killsAfterLost );
    const std::vector<std::string> children = ChildrenLeft();

    for( size_t i = 0; i < children.size(); ++i )
        std::printf( "%s%s", i ? ", " : "", children[i].c_str() );

    std::printf( "], \"statuses\": [" );

    for( size_t i = 0; i < results.size(); ++i )
        std::printf( "%s\"%s\"", i ? ", " : "", results[i].status.c_str() );

    std::printf( "]}\n" );
    return 0;
}
