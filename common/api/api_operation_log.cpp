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

#include <api/api_operation_log.h>

#include <fmt/format.h>

using namespace kiapi::common::commands;


API_OPERATION_LOG& API_OPERATION_LOG::Instance()
{
    static API_OPERATION_LOG log;
    return log;
}


std::optional<std::string> API_OPERATION_LOG::CheckId( const std::string& aId )
{
    if( aId.empty() )
        return std::string( "the operation id is empty" );

    if( aId.size() > MAX_ID_LENGTH )
        return fmt::format( "the operation id is longer than {} characters", MAX_ID_LENGTH );

    for( unsigned char c : aId )
    {
        if( c < 0x21 || c > 0x7e )
            return std::string( "the operation id must be printable ASCII without spaces" );
    }

    return std::nullopt;
}


void API_OPERATION_LOG::Record( const OperationStatusResponse& aEntry )
{
    const std::string& id = aEntry.operation_id();

    if( !m_entries.contains( id ) )
    {
        m_order.push_back( id );

        while( m_order.size() > CAPACITY )
        {
            m_entries.erase( m_order.front() );
            m_order.pop_front();
        }
    }

    OperationStatusResponse& entry = m_entries[id];
    entry.CopyFrom( aEntry );
    entry.set_history_capacity( static_cast<int>( CAPACITY ) );
}


void API_OPERATION_LOG::Resolve( const std::string& aId, OperationState aState,
                                 const std::string& aRevisionAfter, const std::string& aMessage )
{
    auto it = m_entries.find( aId );

    if( it == m_entries.end() )
        return;

    it->second.set_state( aState );
    it->second.set_revision_after( aRevisionAfter );

    if( !aMessage.empty() )
        it->second.set_message( aMessage );
}


OperationStatusResponse API_OPERATION_LOG::Find( const std::string& aId ) const
{
    auto it = m_entries.find( aId );

    if( it != m_entries.end() )
        return it->second;

    OperationStatusResponse unknown;
    unknown.set_operation_id( aId );
    unknown.set_state( OperationState::OPS_UNKNOWN );
    unknown.set_history_capacity( static_cast<int>( CAPACITY ) );
    return unknown;
}


void API_OPERATION_LOG::Clear()
{
    m_entries.clear();
    m_order.clear();
}
