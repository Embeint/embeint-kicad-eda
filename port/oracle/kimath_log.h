// SPDX-License-Identifier: GPL-3.0-or-later
// The wx host of util.cpp's logging (unit libs.kimath.src.math.util.application.ui.p001), as
// adapters observe it.
//
// kimathLogOverflow ends in wxFAIL_MSG, which calls wxOnAssert when wxTheAssertHandler is set.
// wx 3.2.4's default handler in this process (no wxApp) prints the message to stderr and
// returns false, so wxTrapInAssert stays false and the caller continues: KiROUND returns its
// clamped value. LOG_SCOPE wraps the process's handler for one request: it records the
// arguments of every wxOnAssert call made from util.cpp, in call order, then calls the wrapped
// handler, so the genuine continuation follows. Assertions from any other source are passed
// to the wrapped handler unrecorded, exactly as before, so an adapter's own wxCHECK and
// wxFAIL sites keep their behaviour. With no handler installed nothing is installed: the
// macro then makes no call.
//
// Logged<Run> registers an adapter whose requests may reach a logging KiROUND: when any call
// was recorded the reply gains "kimath_log", the list of calls; otherwise the reply is
// unchanged. A refusal (an exception) discards what was recorded with the request.
#pragma once

#include "registry.h"
#include <wx/debug.h>
#include <wx/string.h>
#include <stdexcept>

namespace oracle::kimath_log
{
/// The calls recorded for the current request, and the handler the recorder wraps.
inline JSON*             g_events = nullptr;
inline wxAssertHandler_t g_wrapped = nullptr;

/// True for the spelling of util.cpp's __FILE__ in the reference build.
inline bool FromUtil( const wxString& aFile )
{
    return aFile.EndsWith( wxS( "libs/kimath/src/math/util.cpp" ) );
}

/// One wxOnAssert call: line, function, condition and message (the file is checked above).
inline JSON AssertCall( int aLine, const wxString& aFunc, const wxString& aCond,
                        const wxString& aMsg )
{
    return JSON{ { "call", "wxOnAssert" },
                 { "line", aLine },
                 { "function", aFunc.utf8_string() },
                 { "condition", aCond.utf8_string() },
                 { "message", aMsg.utf8_string() } };
}

inline void RecordAssert( const wxString& aFile, int aLine, const wxString& aFunc,
                          const wxString& aCond, const wxString& aMsg )
{
    if( g_events && FromUtil( aFile ) )
        g_events->push_back( AssertCall( aLine, aFunc, aCond, aMsg ) );

    if( g_wrapped )
        g_wrapped( aFile, aLine, aFunc, aCond, aMsg );
}

/// Installs RecordAssert around the process's handler for one request, and only if a handler
/// is installed (wxFAIL_MSG calls wxOnAssert only then).
class LOG_SCOPE
{
public:
    explicit LOG_SCOPE( JSON& aEvents ) : m_outer( g_events ), m_outerWrapped( g_wrapped )
    {
        if( m_outer )
            throw std::logic_error( "nested kimath log scope" );

        g_events = &aEvents;
        g_wrapped = wxTheAssertHandler;

        if( g_wrapped )
            wxSetAssertHandler( RecordAssert );
    }

    ~LOG_SCOPE()
    {
        if( g_wrapped )
            wxSetAssertHandler( g_wrapped );

        g_events = m_outer;
        g_wrapped = m_outerWrapped;
    }

    LOG_SCOPE( const LOG_SCOPE& ) = delete;
    LOG_SCOPE& operator=( const LOG_SCOPE& ) = delete;

private:
    JSON*             m_outer;
    wxAssertHandler_t m_outerWrapped;
};

/// Runs a request under LOG_SCOPE; a reply whose request made util.cpp calls gains them.
template <SHIM S>
JSON Logged( const JSON& aInput )
{
    JSON events = JSON::array();
    JSON result;

    {
        LOG_SCOPE scope( events );
        result = S( aInput );
    }

    if( !events.empty() )
    {
        if( !result.is_object() || result.contains( "kimath_log" ) )
            throw std::logic_error( "kimath log needs an object reply" );

        result["kimath_log"] = std::move( events );
    }

    return result;
}
} // namespace oracle::kimath_log
