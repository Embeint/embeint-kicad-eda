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

#ifndef KICAD_API_OPERATION_LOG_H
#define KICAD_API_OPERATION_LOG_H

#include <deque>
#include <map>
#include <optional>
#include <string>

#include <kicommon.h>
#include <api/common/commands/editor_commands.pb.h>

/**
 * The outcomes of API mutations that carried ApiRequestHeader.operation_id, so a client whose
 * request timed out can ask what happened instead of resending it.
 *
 * One log per process, shared by every editor.  It keeps the most recent CAPACITY operations;
 * an evicted or never received operation reports OPS_UNKNOWN.  Only used on the UI thread.
 */
class KICOMMON_API API_OPERATION_LOG
{
public:
    /// Operations remembered; the oldest is forgotten when a new one would exceed this
    static constexpr size_t CAPACITY = 1024;

    /// Longest operation id accepted
    static constexpr size_t MAX_ID_LENGTH = 128;

    static API_OPERATION_LOG& Instance();

    API_OPERATION_LOG( const API_OPERATION_LOG& ) = delete;
    API_OPERATION_LOG& operator=( const API_OPERATION_LOG& ) = delete;

    /// @return an error message if aId is not a usable operation id, else std::nullopt
    static std::optional<std::string> CheckId( const std::string& aId );

    bool Contains( const std::string& aId ) const { return m_entries.contains( aId ); }

    /// Add or replace an operation; a new one may evict the oldest
    void Record( const kiapi::common::commands::OperationStatusResponse& aEntry );

    /// Set the final state of a recorded operation; does nothing for an unknown id
    void Resolve( const std::string& aId, kiapi::common::commands::OperationState aState,
                  const std::string& aRevisionAfter, const std::string& aMessage );

    /// The recorded operation, or a response with OPS_UNKNOWN
    kiapi::common::commands::OperationStatusResponse Find( const std::string& aId ) const;

    /// Forget everything (for tests)
    void Clear();

private:
    API_OPERATION_LOG() = default;

    std::map<std::string, kiapi::common::commands::OperationStatusResponse> m_entries;
    std::deque<std::string>                                                 m_order;
};

#endif // KICAD_API_OPERATION_LOG_H
