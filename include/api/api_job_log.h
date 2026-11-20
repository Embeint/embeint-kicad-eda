/*
 * This program source code file is part of KiCad, a free EDA CAD application.
 *
 * Copyright The KiCad Developers, see AUTHORS.txt for contributors.
 *
 * This program is free software: you can redistribute it and/or modify it
 * under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#ifndef KICAD_API_JOB_LOG_H
#define KICAD_API_JOB_LOG_H

#include <wx/log.h>
#include <wx/string.h>

/**
 * Collects wxWidgets log warnings and errors while an API job runs.
 *
 * Exporters report some problems with wxLogWarning or wxLogError (a malformed board outline, for
 * example).  In the GUI these are shown in a modal dialog at the next idle event, after the API
 * reply has been sent, and the editor then answers every API command with AS_BUSY until someone
 * closes the dialog.  While an instance exists it is the active log target, so the messages are
 * collected for the API reply instead; the previous target is restored when it is destroyed.
 */
class API_JOB_LOG_CAPTURE : public wxLog
{
public:
    API_JOB_LOG_CAPTURE() :
            m_previous( wxLog::SetActiveTarget( this ) )
    {
    }

    ~API_JOB_LOG_CAPTURE() override
    {
        wxLog::SetActiveTarget( m_previous );
    }

    /**
     * Collect messages that other threads logged during the job, then return all of them.
     *
     * @return the warnings and errors logged while this instance was active, one per line.
     */
    const wxString& Finish()
    {
        wxLog::FlushActive();
        return m_messages;
    }

protected:
    void DoLogRecord( wxLogLevel aLevel, const wxString& aMessage, const wxLogRecordInfo& aInfo ) override
    {
        // Information and debug messages stay out of the reply, as they stay out of the dialog
        if( aLevel > wxLOG_Warning )
            return;

        if( !m_messages.IsEmpty() )
            m_messages << wxS( "\n" );

        m_messages << aMessage;
    }

private:
    wxLog*   m_previous; ///< The log target before this one, restored on destruction
    wxString m_messages; ///< Warnings and errors collected so far
};

#endif // KICAD_API_JOB_LOG_H
