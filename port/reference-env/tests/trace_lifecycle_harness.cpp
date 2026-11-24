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
    std::printf( "{\"status\": \"%s\", \"steps\": %zu, \"return_address\": %lu, \"return_to\": %lu, "
                 "\"child\": %d, \"reaped\": %s, \"child_exists\": %s, \"children_left\": [",
                 last.status.c_str(), last.addresses.size(), static_cast<unsigned long>( last.returnAddress ),
                 static_cast<unsigned long>( last.returnTo ), static_cast<int>( last.child ),
                 reaped ? "true" : "false", exists ? "true" : "false" );
    const std::vector<std::string> children = ChildrenLeft();

    for( size_t i = 0; i < children.size(); ++i )
        std::printf( "%s%s", i ? ", " : "", children[i].c_str() );

    std::printf( "], \"statuses\": [" );

    for( size_t i = 0; i < results.size(); ++i )
        std::printf( "%s\"%s\"", i ? ", " : "", results[i].status.c_str() );

    std::printf( "]}\n" );
    return 0;
}
