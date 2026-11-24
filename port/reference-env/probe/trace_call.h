// SPDX-License-Identifier: GPL-3.0-or-later
// Bounded single-step trace of one function call in a ptrace-traced child.
//
// The child calls the function once untraced (any lazy binding happens), stops
// itself, and the parent single-steps it from that stop until the entry
// address is reached, then records every instruction address until the stack
// pointer rises above its value at the entry (the call returned).
//
// Every wait has a monotonic wall-clock deadline (SIGCHLD is blocked and
// awaited with sigtimedwait, and waitpid is polled with WNOHANG), as do the
// whole trace and the final reap. The child dies with the tracer
// (PR_SET_PDEATHSIG before tracing, PTRACE_O_EXITKILL once traced). On
// success, timeout or any error, an owned child that has not been reaped is
// killed and reaped. Auto-reaping dispositions are rejected before fork; ECHILD
// means ownership is lost and that pid must never be signalled. No process-wide
// alarm or signal handler is installed.

#ifndef REFENV_TRACE_CALL_H
#define REFENV_TRACE_CALL_H

#include <cerrno>
#include <csignal>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <string>
#include <sys/prctl.h>
#include <sys/ptrace.h>
#include <sys/user.h>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

namespace trace
{

/// Bounds of one traced call. Durations are wall-clock milliseconds.
struct LIMITS
{
    long   maxSteps = 2000000;    ///< single steps in total, including those before the entry
    size_t maxRecorded = 50000;   ///< instruction addresses recorded from the entry
    long   totalMs = 20000;       ///< whole trace from fork until the call returned
    long   waitMs = 10000;        ///< any single wait for the child
    long   reapMs = 5000;         ///< killing and reaping the child afterwards
};

/// A GOT slot read when the tracee was at a RIP-relative indirect jump.
struct SLOT_READ
{
    uintptr_t at = 0;
    uintptr_t slot = 0;
    uintptr_t value = 0;
};

struct RESULT
{
    std::string            status;            ///< "returned" or why the trace is incomplete
    std::vector<uintptr_t> addresses;         ///< recorded instruction addresses from the entry
    uintptr_t              returnAddress = 0; ///< return address on the stack at the entry
    uintptr_t              returnTo = 0;      ///< instruction address after the call returned
    std::vector<SLOT_READ> slotReads;
    pid_t                  child = -1;
    bool                   reaped = false;    ///< the child's termination was collected
};

inline long long NowMs()
{
    timespec now{};
    clock_gettime( CLOCK_MONOTONIC, &now );
    return static_cast<long long>( now.tv_sec ) * 1000 + now.tv_nsec / 1000000;
}

enum class CHILD
{
    STOPPED,
    EXITED,
    SIGNALED,
    TIMEOUT,
    LOST,
    FAILED
};

/// Wait for a state change of aChild until aDeadline (monotonic ms). Never
/// blocks past the deadline; EINTR is retried. SIGCHLD must be blocked.
inline CHILD WaitChild( pid_t aChild, long long aDeadline, int& aStatus )
{
    sigset_t chld;
    sigemptyset( &chld );
    sigaddset( &chld, SIGCHLD );

    for( ;; )
    {
        const pid_t got = waitpid( aChild, &aStatus, WNOHANG | __WALL );

        if( got == aChild )
        {
            if( WIFSTOPPED( aStatus ) )
                return CHILD::STOPPED;

            return WIFEXITED( aStatus ) ? CHILD::EXITED : CHILD::SIGNALED;
        }

        if( got < 0 && errno == ECHILD )
            return CHILD::LOST;

        if( got < 0 && errno != EINTR )
            return CHILD::FAILED;

        const long long remaining = aDeadline - NowMs();

        if( remaining <= 0 )
            return CHILD::TIMEOUT;

        // Short slices: a coalesced or missed SIGCHLD costs at most 10 ms.
        const long long slice = remaining < 10 ? remaining : 10;
        timespec        timeout{ 0, static_cast<long>( slice ) * 1000000L };
        sigtimedwait( &chld, nullptr, &timeout );
    }
}

inline std::string Describe( CHILD aState, int aStatus, const char* aWhen )
{
    char text[96];

    switch( aState )
    {
    case CHILD::EXITED:
        std::snprintf( text, sizeof text, "child exited with status %d %s", WEXITSTATUS( aStatus ), aWhen );
        break;
    case CHILD::SIGNALED:
        std::snprintf( text, sizeof text, "child killed by signal %d %s", WTERMSIG( aStatus ), aWhen );
        break;
    case CHILD::STOPPED:
        std::snprintf( text, sizeof text, "child stopped by signal %d %s", WSTOPSIG( aStatus ), aWhen );
        break;
    case CHILD::TIMEOUT: std::snprintf( text, sizeof text, "timeout %s", aWhen ); break;
    case CHILD::LOST: std::snprintf( text, sizeof text, "child no longer owned %s", aWhen ); break;
    case CHILD::FAILED: std::snprintf( text, sizeof text, "waitpid failed %s", aWhen ); break;
    }

    return text;
}

/// Read one word of the tracee; false if the read failed.
inline bool Peek( pid_t aChild, enum __ptrace_request aRequest, uintptr_t aAddress, uintptr_t& aValue )
{
    errno = 0;
    const long value = ptrace( aRequest, aChild, reinterpret_cast<void*>( aAddress ), nullptr );

    if( errno != 0 )
        return false;

    aValue = static_cast<uintptr_t>( value );
    return true;
}

/// Trace one call of aCall( aContext ), whose first instruction is aEntry.
inline RESULT TraceCall( void ( *aCall )( const void* ), const void* aContext, uintptr_t aEntry,
                         const LIMITS& aLimits )
{
    RESULT          result;
    struct sigaction disposition{};

    // Auto-reaping releases a pid without a wait status. Do not create a child
    // whose lifetime this helper cannot own; leave the caller's disposition alone.
    if( sigaction( SIGCHLD, nullptr, &disposition ) != 0
        || disposition.sa_handler == SIG_IGN || ( disposition.sa_flags & SA_NOCLDWAIT ) )
    {
        result.status = "unsupported SIGCHLD auto-reaping disposition";
        return result;
    }

    const long long start = NowMs();
    const long long deadline = start + aLimits.totalMs;
    sigset_t        chld, previous;
    sigemptyset( &chld );
    sigaddset( &chld, SIGCHLD );
    sigprocmask( SIG_BLOCK, &chld, &previous );
    // The child uses _exit on every path, so it cannot flush inherited stdio.
    // Flushing the parent's streams here could block before any bounded wait.
    const pid_t parent = getpid();
    const pid_t child = fork();

    if( child < 0 )
    {
        sigprocmask( SIG_SETMASK, &previous, nullptr );
        result.status = "fork failed";
        return result;
    }

    if( child == 0 )
    {
        // Die with the tracer, also before tracing starts.
        if( prctl( PR_SET_PDEATHSIG, SIGKILL ) != 0 || getppid() != parent )
            _exit( 121 );

        sigprocmask( SIG_SETMASK, &previous, nullptr );
        aCall( aContext );

        if( ptrace( PTRACE_TRACEME, 0, nullptr, nullptr ) != 0 )
            _exit( 120 );

        raise( SIGSTOP );
        aCall( aContext );
        _exit( 0 );
    }

    result.child = child;
    bool  owned = true;
    int   status = 0;
    auto  waitFor = [&]( const char* aWhen ) -> CHILD
    {
        const long long limit = NowMs() + aLimits.waitMs;
        const CHILD     state = WaitChild( child, limit < deadline ? limit : deadline, status );

        if( state == CHILD::EXITED || state == CHILD::SIGNALED )
            result.reaped = true;

        if( state == CHILD::LOST )
            owned = false;

        if( state == CHILD::TIMEOUT || state == CHILD::FAILED || state == CHILD::LOST || state == CHILD::EXITED
            || state == CHILD::SIGNALED )
        {
            result.status = Describe( state, status, aWhen );

            if( state == CHILD::EXITED && WEXITSTATUS( status ) == 120 )
                result.status = "PTRACE_TRACEME refused";
        }

        return state;
    };

    CHILD state = waitFor( "before the traced call started" );

    if( state == CHILD::STOPPED && WSTOPSIG( status ) != SIGSTOP )
        result.status = Describe( state, status, "before the traced call started" );
    else if( state == CHILD::STOPPED
             && ptrace( PTRACE_SETOPTIONS, child, nullptr,
                        reinterpret_cast<void*>( static_cast<uintptr_t>( PTRACE_O_EXITKILL ) ) ) != 0 )
        result.status = "PTRACE_SETOPTIONS failed";

    bool               entered = false;
    unsigned long long entryStack = 0;

    for( long step = 0; result.status.empty(); ++step )
    {
        user_regs_struct regs{};

        if( step >= aLimits.maxSteps )
        {
            result.status = "step bound exceeded";
            break;
        }

        if( NowMs() >= deadline )
        {
            result.status = entered ? "timeout during the traced call" : "timeout before the entry";
            break;
        }

        if( ptrace( PTRACE_GETREGS, child, nullptr, &regs ) != 0 )
        {
            result.status = "PTRACE_GETREGS failed";
            break;
        }

        if( !entered && regs.rip == aEntry )
        {
            entered = true;
            entryStack = regs.rsp;

            if( !Peek( child, PTRACE_PEEKDATA, regs.rsp, result.returnAddress ) )
            {
                result.status = "reading the return address failed";
                break;
            }
        }

        if( entered )
        {
            if( regs.rsp > entryStack )
            {
                result.returnTo = regs.rip;
                result.status = "returned";
                break;
            }

            if( result.addresses.size() >= aLimits.maxRecorded )
            {
                result.status = "recorded address bound exceeded";
                break;
            }

            result.addresses.push_back( regs.rip );
            uintptr_t code = 0;

            // jmp *rel32(%rip): record the GOT slot and the value it holds now.
            if( Peek( child, PTRACE_PEEKTEXT, regs.rip, code ) && ( code & 0xffff ) == 0x25ff )
            {
                const int32_t displacement = static_cast<int32_t>( ( code >> 16 ) & 0xffffffffu );
                SLOT_READ     read;
                read.at = regs.rip;
                read.slot = regs.rip + 6 + static_cast<intptr_t>( displacement );

                if( !Peek( child, PTRACE_PEEKDATA, read.slot, read.value ) )
                {
                    result.status = "reading a GOT slot failed";
                    break;
                }

                result.slotReads.push_back( read );
            }
        }

        if( ptrace( PTRACE_SINGLESTEP, child, nullptr, nullptr ) != 0 )
        {
            result.status = "PTRACE_SINGLESTEP failed";
            break;
        }

        state = waitFor( entered ? "during the traced call" : "before the entry" );

        if( state != CHILD::STOPPED )
            break;

        if( WSTOPSIG( status ) != SIGTRAP )
        {
            result.status = Describe( state, status, entered ? "during the traced call" : "before the entry" );
            break;
        }
    }

    if( owned && !result.reaped )
    {
        kill( child, SIGKILL );
        const long long limit = NowMs() + aLimits.reapMs;

        for( ;; )
        {
            const CHILD gone = WaitChild( child, limit, status );

            if( gone == CHILD::LOST )
            {
                owned = false;
                result.status += "; child no longer owned";
                break;
            }

            if( gone == CHILD::EXITED || gone == CHILD::SIGNALED )
            {
                result.reaped = true;
                break;
            }

            if( gone != CHILD::STOPPED )
                break;
        }

        if( owned && !result.reaped )
            result.status += "; child not reaped";
    }

    sigprocmask( SIG_SETMASK, &previous, nullptr );
    return result;
}

} // namespace trace

#endif // REFENV_TRACE_CALL_H
