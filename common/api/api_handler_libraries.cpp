/*
 * This program source code file is part of KiCad, a free EDA CAD application.
 *
 * Copyright The KiCad Developers, see AUTHORS.txt for contributors.
 * @author Jon Evans <jon@craftyjon.com>
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

#include <api/api_handler_libraries.h>

#include <magic_enum.hpp>
#include <ranges>

#include <api/api_enums.h>
#include <api/api_utils.h>
#include <common.h>
#include <libraries/library_manager.h>
#include <wx/filename.h>
#include <wx/tokenzr.h>
#include <kiway.h>
#include <pgm_base.h>
#include <settings/settings_manager.h>

#include <api/common/types/base_types.pb.h>
#include <api/common/commands/editor_commands.pb.h>
#include <api/common/types/library_types.pb.h>

#include <design_block_library_adapter.h>
#include <design_block.h>

using namespace kiapi::common::commands;
using kiapi::common::types::LibraryCommandStatus;
using kiapi::common::types::LibraryLoadStatus;
using kiapi::common::types::LibraryTableScope;
using kiapi::common::types::LibraryType;

API_HANDLER_LIBRARIES::API_HANDLER_LIBRARIES( LIBRARY_TABLE_TYPE aType ) :
        m_type( aType )
{
    registerHandler<GetLibraryItems, LibraryItemsResponse>( &API_HANDLER_LIBRARIES::handleGetLibraryItems );
    registerHandler<GetLibraryStatuses, LibraryStatusResponse>( &API_HANDLER_LIBRARIES::handleGetLibraryStatuses );
    registerHandler<ReloadLibrary, LibraryCommandStatus>( &API_HANDLER_LIBRARIES::handleReloadLibrary );
    registerHandler<GetItemsFromLibrary, GetItemsResponse>( &API_HANDLER_LIBRARIES::handleGetItemsFromLibrary );
    registerHandler<GetLibraryTable, LibraryTableResponse>( &API_HANDLER_LIBRARIES::handleGetLibraryTable );
    registerHandler<AddLibraryTableEntry, LibraryCommandStatus>(
            &API_HANDLER_LIBRARIES::handleAddLibraryTableEntry );
    registerHandler<UpdateLibraryTableEntry, LibraryCommandStatus>(
            &API_HANDLER_LIBRARIES::handleUpdateLibraryTableEntry );
    registerHandler<DeleteLibraryTableEntry, LibraryCommandStatus>(
            &API_HANDLER_LIBRARIES::handleDeleteLibraryTableEntry );
    registerHandler<SearchLibraries, SearchLibrariesResponse>( &API_HANDLER_LIBRARIES::handleSearchLibraries );

    // Only the design-block instance registers LoadAllLibraries: it is the dispatcher that
    // lazily loads the schematic and PCB kifaces and forwards to their per-type loads.  If
    // the per-type (SYMBOL/FOOTPRINT) handlers registered it too, the server's handler set
    // (ordered by pointer, not registration order) could route the command to an instance
    // without a KIWAY, which would fail the request.
    if( aType == LIBRARY_TABLE_TYPE::DESIGN_BLOCK )
        registerHandler<LoadAllLibraries, LibraryCommandStatus>( &API_HANDLER_LIBRARIES::handleLoadAllLibraries );
}


API_HANDLER_LIBRARIES::~API_HANDLER_LIBRARIES() = default;


LIBRARY_MANAGER_ADAPTER* API_HANDLER_LIBRARIES::adapterForProject( PROJECT& aProject ) const
{
    return aProject.DesignBlockLibs();
}


std::vector<wxString> API_HANDLER_LIBRARIES::getItemNames( LIBRARY_MANAGER_ADAPTER& aAdapter,
                                                           const wxString& aNickname ) const
{
    return static_cast<DESIGN_BLOCK_LIBRARY_ADAPTER&>( aAdapter ).GetDesignBlockNames( aNickname );
}


HANDLER_RESULT<LibraryItemsResponse>
API_HANDLER_LIBRARIES::handleGetLibraryItems( const HANDLER_CONTEXT<GetLibraryItems>& aCtx )
{
    if( aCtx.Request.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    PROJECT& project = Pgm().GetSettingsManager().Prj();
    LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( project );

    if( !adapter )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no library adapter is available for this library type" );
        return tl::unexpected( e );
    }

    std::vector<wxString> nicknames;

    if( aCtx.Request.nickname_size() > 0 )
    {
        for( const std::string& nickname : aCtx.Request.nickname() )
        {
            wxString nicknameStr = wxString::FromUTF8( nickname );

            // TODO(JE) decide whether or not to do blocking loads for explicit requests
            adapter->LoadLibraryEntry( nicknameStr );

            nicknames.emplace_back( nicknameStr );
        }
    }
    else
    {
        for( const LIBRARY_TABLE_ROW* row : adapter->Rows() )
            nicknames.emplace_back( row->Nickname() );
    }

    LibraryItemsResponse response;

    for( const wxString& nickname : nicknames )
    {
        for( const wxString& itemName : getItemNames( *adapter, nickname ) )
        {
            kiapi::common::types::LibraryIdentifier* id = response.add_items();
            id->set_library_nickname( nickname.ToUTF8() );
            id->set_entry_name( itemName.ToUTF8() );
        }
    }

    return response;
}


static void packTableEntry( kiapi::common::types::LibraryTableEntry& aEntry, LIBRARY_TABLE_TYPE aTableType,
                            const LIBRARY_TABLE_ROW& aRow )
{
    aEntry.set_nickname( aRow.Nickname().ToUTF8() );
    aEntry.set_uri( aRow.URI().ToUTF8() );
    aEntry.set_type( ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( aTableType ) );
    aEntry.set_scope( ToProtoEnum<LIBRARY_TABLE_SCOPE, LibraryTableScope>( aRow.Scope() ) );
    aEntry.set_description( aRow.Description().ToUTF8() );

    for( const auto& [key, value] : aRow.GetOptionsMap() )
        ( *aEntry.mutable_options() )[key] = value;

    if( std::optional<LIBRARY_MANAGER_ADAPTER*> adapter = Pgm().GetLibraryManager().Adapter( aTableType ) )
        aEntry.set_writable( ( *adapter )->IsWritable( aRow.Nickname() ) );

    aEntry.set_disabled( aRow.Disabled() );
    aEntry.set_hidden( aRow.Hidden() );
}


static void packLibraryStatusEntry( kiapi::common::types::LibraryStatusEntry* aEntry, LIBRARY_TABLE_TYPE aTableType,
                                    const LIBRARY_TABLE_ROW& aRow, const LIB_STATUS& aStatus )
{
    packTableEntry( *aEntry->mutable_entry(), aTableType, aRow );
    aEntry->set_status( ToProtoEnum<LOAD_STATUS, LibraryLoadStatus>( aStatus.load_status ) );

    if( aStatus.error )
        aEntry->set_error_message( aStatus.error->message.ToUTF8() );
}


HANDLER_RESULT<LibraryStatusResponse>
API_HANDLER_LIBRARIES::handleGetLibraryStatuses( const HANDLER_CONTEXT<GetLibraryStatuses>& aCtx )
{
    LIBRARY_TABLE_SCOPE scope = FromProtoEnum<LIBRARY_TABLE_SCOPE>( aCtx.Request.scope() );

    if( scope == LIBRARY_TABLE_SCOPE::UNINITIALIZED )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "unknown library table scope" );
        return tl::unexpected( e );
    }

    LIBRARY_MANAGER& manager = Pgm().GetLibraryManager();
    LibraryStatusResponse response;

    std::vector types = { LIBRARY_TABLE_TYPE::SYMBOL, LIBRARY_TABLE_TYPE::FOOTPRINT, LIBRARY_TABLE_TYPE::DESIGN_BLOCK };

    if( !aCtx.Request.types().empty() )
    {
        types.clear();

        for( int typeProto : aCtx.Request.types() )
        {
            LIBRARY_TABLE_TYPE type = FromProtoEnum<LIBRARY_TABLE_TYPE>( static_cast<LibraryType>( typeProto ) );

            if( type == LIBRARY_TABLE_TYPE::UNINITIALIZED )
            {
                ApiResponseStatus e;
                e.set_status( ApiStatusCode::AS_BAD_REQUEST );
                e.set_error_message( wxString::Format( "invalid library table type %d", typeProto ).ToUTF8() );
                return tl::unexpected( e );
            }

            types.push_back( type );
        }
    }

    for( LIBRARY_TABLE_TYPE tableType : types )
    {
        LIBRARY_MANAGER_ADAPTER* adapter = nullptr;

        if( std::optional<LIBRARY_MANAGER_ADAPTER*> optAdapter = manager.Adapter( tableType ) )
            adapter = *optAdapter;

        for( const LIBRARY_TABLE_ROW* row : manager.Rows( tableType, scope ) )
        {
            if( row->Disabled() )
                continue;

            kiapi::common::types::LibraryStatusEntry* entry = response.add_libraries();

            if( adapter )
            {
                if( std::optional<LIB_STATUS> status = adapter->GetLibraryStatus( row->Nickname() ) )
                {
                    packLibraryStatusEntry( entry, tableType, *row, *status );
                    continue;
                }
            }

            packLibraryStatusEntry( entry, tableType, *row, LIB_STATUS{} );
        }
    }

    return response;
}


HANDLER_RESULT<LibraryCommandStatus>
API_HANDLER_LIBRARIES::handleReloadLibrary( const HANDLER_CONTEXT<ReloadLibrary>& aCtx )
{
    auto makeStatus =
            []( LibraryCommandStatus::Code aCode, const wxString& aMessage = wxEmptyString )
            {
                LibraryCommandStatus status;
                status.set_code( aCode );

                if( !aMessage.empty() )
                    status.set_error_message( aMessage.ToUTF8() );

                return status;
            };

    if( aCtx.Request.type() == LibraryType::LT_UNKNOWN )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "a valid library type is required" );
        return tl::unexpected( e );
    }

    if( aCtx.Request.scope() == LibraryTableScope::LTS_BOTH || aCtx.Request.scope() == LibraryTableScope::LTS_UNKNOWN )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "LTS_BOTH is invalid for ReloadLibrary; choose one table" );
        return tl::unexpected( e );
    }

    const LIBRARY_TABLE_TYPE  tableType = FromProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( aCtx.Request.type() );
    const LIBRARY_TABLE_SCOPE scope = ( aCtx.Request.scope() == LibraryTableScope::LTS_GLOBAL )
                                                                                      ? LIBRARY_TABLE_SCOPE::GLOBAL
                                                                                      : LIBRARY_TABLE_SCOPE::PROJECT;

    LIBRARY_MANAGER& manager = Pgm().GetLibraryManager();

    if( !manager.Adapter( tableType ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    std::vector<wxString> nicknames;

    if( aCtx.Request.nickname_size() > 0 )
    {
        for( const std::string& nickname : aCtx.Request.nickname() )
        {
            std::optional<LIBRARY_TABLE_ROW*> row = manager.GetRow( tableType, nickname, scope );

            if( !row.has_value() )
            {
                return makeStatus( LibraryCommandStatus::LCS_NOT_FOUND,
                                   wxString::Format( "Library '%s' not found", nickname ) );
            }
            else if( row.value()->Disabled() )
            {
                return makeStatus( LibraryCommandStatus::LCS_NOT_FOUND,
                                   wxString::Format( "Library '%s' not enabled", nickname ) );
            }

            nicknames.emplace_back( wxString::FromUTF8( nickname ) );
        }
    }
    else
    {
        for( const LIBRARY_TABLE_ROW* row : manager.Rows( tableType, scope ) )
        {
            if( !row->Disabled() )
                nicknames.emplace_back( row->Nickname() );
        }
    }

    if( nicknames.empty() )
        return makeStatus( LibraryCommandStatus::LCS_NOT_FOUND, "No libraries found to reload" );

    for( const wxString& nickname : nicknames )
        manager.ReloadLibraryEntry( tableType, nickname, scope );

    return makeStatus( LibraryCommandStatus::LCS_OK );
}


HANDLER_RESULT<LibraryCommandStatus>
API_HANDLER_LIBRARIES::handleLoadAllLibraries( const HANDLER_CONTEXT<LoadAllLibraries>& aCtx )
{
    auto makeStatus =
            []( LibraryCommandStatus::Code aCode, const wxString& aMessage = wxEmptyString )
            {
                LibraryCommandStatus status;
                status.set_code( aCode );

                if( !aMessage.empty() )
                    status.set_error_message( aMessage.ToUTF8() );

                return status;
            };

    if( Pgm().IsGUI() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNIMPLEMENTED );
        e.set_error_message( "LoadAllLibraries is not available in GUI mode" );
        return tl::unexpected( e );
    }

    std::map<LIBRARY_TABLE_TYPE, KIWAY::FACE_T> typesToLoad;

    if( aCtx.Request.type_size() == 0 )
    {
        typesToLoad = { { LIBRARY_TABLE_TYPE::SYMBOL,       KIWAY::FACE_SCH },
                        { LIBRARY_TABLE_TYPE::FOOTPRINT,    KIWAY::FACE_PCB },
                        { LIBRARY_TABLE_TYPE::DESIGN_BLOCK, KIWAY::KIWAY_FACE_COUNT } };
    }
    else
    {
        for( int protoRaw : aCtx.Request.type() )
        {
            LibraryType protoType = static_cast<LibraryType>( protoRaw );
            LIBRARY_TABLE_TYPE type = FromProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( protoType );

            switch( type )
            {
            case LIBRARY_TABLE_TYPE::SYMBOL:        typesToLoad.emplace( type, KIWAY::FACE_SCH );         break;
            case LIBRARY_TABLE_TYPE::FOOTPRINT:     typesToLoad.emplace( type, KIWAY::FACE_PCB );         break;
            case LIBRARY_TABLE_TYPE::DESIGN_BLOCK:  typesToLoad.emplace( type, KIWAY::KIWAY_FACE_COUNT ); break;

            default:
            {
                ApiResponseStatus e;
                e.set_status( ApiStatusCode::AS_BAD_REQUEST );
                e.set_error_message( wxString::Format( wxS( "invalid library type %s" ),
                                                       magic_enum::enum_name( protoType ) ).ToUTF8() );
                return tl::unexpected( e );
            }
            }
        }
    }

    // should always be set in this path
    if( !m_kiway )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_NOT_READY );
        e.set_error_message( "internal error while attempting to load all libraries" );
        return tl::unexpected( e );
    }

    KIWAY& kiway = *m_kiway;
    bool   error = false;

    for( const auto& [type, face] : typesToLoad )
    {
        if( type == LIBRARY_TABLE_TYPE::DESIGN_BLOCK )
        {
            LibraryCommandStatus status = loadAllLibraries();

            if( status.code() != LibraryCommandStatus::LCS_OK )
                error = true;

            continue;
        }

        KIFACE* kiface = kiway.KiFACE( face );

        if( !kiface )
        {
            wxFAIL;
            error = true;
            continue;
        }

        if( m_handlerRegisterCallback )
            m_handlerRegisterCallback( kiface );

        // A result of "true" only means that it -started- a load, not that it finished successfully.
        // But we should still report an error if it couldn't even start.
        if( !kiface->LoadAllLibraries() )
            error = true;
    }

    return makeStatus( error ? LibraryCommandStatus::LCS_ERROR : LibraryCommandStatus::LCS_OK );
}


LibraryCommandStatus API_HANDLER_LIBRARIES::loadAllLibraries()
{
    // This base implementation handles design blocks
    LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( Pgm().GetSettingsManager().Prj() );
    LibraryCommandStatus     status;

    if( !adapter )
    {
        status.set_code( LibraryCommandStatus::LCS_ERROR );
        return status;
    }

    adapter->AsyncLoad();
    status.set_code( LibraryCommandStatus::LCS_OK );
    return status;
}


HANDLER_RESULT<GetItemsResponse>
API_HANDLER_LIBRARIES::handleGetItemsFromLibrary( const HANDLER_CONTEXT<GetItemsFromLibrary>& aCtx )
{
    if( aCtx.Request.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    wxString projectPath = wxString::FromUTF8( aCtx.Request.document().project().path() );
    SETTINGS_MANAGER& mgr = Pgm().GetSettingsManager();
    PROJECT* project = projectPath.IsEmpty() ? &mgr.Prj() : mgr.GetProjectForPath( projectPath );

    if( !project )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "the requested project is not open" );
        return tl::unexpected( e );
    }

    LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( *project );

    if( !adapter )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no library adapter is available for this library type" );
        return tl::unexpected( e );
    }

    GetItemsResponse response;
    response.mutable_header()->mutable_document()->CopyFrom( aCtx.Request.document() );
    google::protobuf::Any any;

    if( aCtx.Request.item_ids().empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "specify at least one lib_id to retrieve" );
        return tl::unexpected( e );
    }

    std::map<wxString, std::vector<wxString>> byNickname;

    for( const kiapi::common::types::LibraryIdentifier& id : aCtx.Request.item_ids() )
    {
        byNickname[wxString::FromUTF8( id.library_nickname() )].emplace_back( wxString::FromUTF8( id.entry_name() ) );
    }

    for( auto& [nickname, entryNames] : byNickname )
    {
        // TODO(JE) decide whether or not to do blocking loads for explicit requests
        adapter->LoadLibraryEntry( nickname );

        for( const wxString& entryName : entryNames )
        {
            if( packLibraryItem( LIB_ID( nickname, entryName ), any ) )
            {
                any.Swap( response.add_items() );
                any.Clear();
            }
        }
    }

    if( response.items().empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "none of the requested items were found" );
        return tl::unexpected( e );
    }

    response.set_status( kiapi::common::types::ItemRequestStatus::IRS_OK );
    return response;
}


namespace
{
LibraryCommandStatus libraryStatus( LibraryCommandStatus::Code aCode, const wxString& aMessage = wxEmptyString )
{
    LibraryCommandStatus status;
    status.set_code( aCode );

    if( !aMessage.IsEmpty() )
        status.set_error_message( aMessage.ToUTF8() );

    return status;
}


ApiResponseStatus unhandled()
{
    ApiResponseStatus e;
    e.set_status( ApiStatusCode::AS_UNHANDLED );
    return e;
}


ApiResponseStatus badRequest( const std::string& aMessage )
{
    ApiResponseStatus e;
    e.set_status( ApiStatusCode::AS_BAD_REQUEST );
    e.set_error_message( aMessage );
    return e;
}


/// The single table a mutation addresses; LTS_BOTH and LTS_UNKNOWN are refused.
std::optional<LIBRARY_TABLE_SCOPE> singleScope( LibraryTableScope aScope )
{
    if( aScope == LibraryTableScope::LTS_GLOBAL )
        return LIBRARY_TABLE_SCOPE::GLOBAL;

    if( aScope == LibraryTableScope::LTS_PROJECT )
        return LIBRARY_TABLE_SCOPE::PROJECT;

    return std::nullopt;
}


void applyEntry( LIBRARY_TABLE_ROW& aRow, const kiapi::common::types::LibraryTableEntry& aEntry )
{
    aRow.SetURI( wxString::FromUTF8( aEntry.uri() ) );
    aRow.SetDescription( wxString::FromUTF8( aEntry.description() ) );

    std::map<std::string, UTF8> options;

    for( const auto& [key, value] : aEntry.options() )
        options[key] = UTF8( value );

    aRow.SetOptions( LIBRARY_TABLE::FormatOptions( &options ) );
    aRow.SetDisabled( aEntry.disabled() );
    aRow.SetHidden( aEntry.hidden() );
}
} // namespace


std::optional<wxString> API_HANDLER_LIBRARIES::saveAndReloadTable( LIBRARY_TABLE& aTable, LIBRARY_TABLE_SCOPE aScope )
{
    if( LIBRARY_RESULT<void> saved = aTable.Save(); !saved )
        return saved.error().message;

    Pgm().GetLibraryManager().ReloadTables( aScope, { m_type } );

    // Reloading a table drops every loaded library of its type.  Load them again in the background,
    // as the library table dialog does through PreloadLibraries
    if( LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( Pgm().GetSettingsManager().Prj() ) )
        adapter->AsyncLoad();

    return std::nullopt;
}


HANDLER_RESULT<LibraryTableResponse>
API_HANDLER_LIBRARIES::handleGetLibraryTable( const HANDLER_CONTEXT<GetLibraryTable>& aCtx )
{
    if( aCtx.Request.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
        return tl::unexpected( unhandled() );

    LIBRARY_TABLE_SCOPE scope = LIBRARY_TABLE_SCOPE::BOTH;

    if( std::optional<LIBRARY_TABLE_SCOPE> single = singleScope( aCtx.Request.scope() ) )
        scope = *single;

    LIBRARY_MANAGER& manager = Pgm().GetLibraryManager();
    PROJECT&         project = Pgm().GetSettingsManager().Prj();
    LibraryTableResponse response;

    if( scope != LIBRARY_TABLE_SCOPE::BOTH )
    {
        if( std::optional<LIBRARY_TABLE*> table = manager.Table( m_type, scope ) )
        {
            kiapi::common::types::LibraryTable* info = response.mutable_table();
            info->set_path( ( *table )->Path().ToUTF8() );
            info->set_scope( aCtx.Request.scope() );
            info->set_valid( ( *table )->IsOk() );

            if( !( *table )->IsOk() )
                info->set_error_message( ( *table )->ErrorDescription().ToUTF8() );
        }
    }

    for( const LIBRARY_TABLE_ROW* row : manager.Rows( m_type, scope ) )
    {
        kiapi::common::types::LibraryTableEntry* entry = response.add_rows();
        packTableEntry( *entry, m_type, *row );

        if( aCtx.Request.substituted() )
            entry->set_uri( ExpandEnvVarSubstitutions( row->URI(), &project ).ToUTF8() );
    }

    return response;
}


HANDLER_RESULT<LibraryCommandStatus>
API_HANDLER_LIBRARIES::handleAddLibraryTableEntry( const HANDLER_CONTEXT<AddLibraryTableEntry>& aCtx )
{
    const kiapi::common::types::LibraryTableEntry& entry = aCtx.Request.entry();

    if( entry.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
        return tl::unexpected( unhandled() );

    std::optional<LIBRARY_TABLE_SCOPE> scope = singleScope( aCtx.Request.scope() );

    if( !scope )
        return tl::unexpected( badRequest( "scope must be LTS_GLOBAL or LTS_PROJECT" ) );

    const wxString nickname = wxString::FromUTF8( entry.nickname() );

    if( nickname.IsEmpty() || entry.uri().empty() )
        return tl::unexpected( badRequest( "a library entry needs a nickname and a uri" ) );

    LIBRARY_MANAGER&              manager = Pgm().GetLibraryManager();
    std::optional<LIBRARY_TABLE*> table = manager.Table( m_type, *scope );

    if( !table )
        return libraryStatus( LibraryCommandStatus::LCS_NOT_FOUND, _( "That library table is not available" ) );

    if( ( *table )->HasRow( nickname ) )
    {
        return libraryStatus( LibraryCommandStatus::LCS_ALREADY_EXISTS,
                              wxString::Format( _( "Library '%s' is already in the table" ), nickname ) );
    }

    if( ( *table )->IsReadOnly() )
        return libraryStatus( LibraryCommandStatus::LCS_READ_ONLY, ( *table )->Path() );

    PROJECT&       project = Pgm().GetSettingsManager().Prj();
    const wxString expanded = ExpandEnvVarSubstitutions( wxString::FromUTF8( entry.uri() ), &project );

    LIBRARY_TABLE_ROW& row = ( *table )->InsertRow();
    row.SetNickname( nickname );
    applyEntry( row, entry );
    row.SetType( pluginTypeForUri( expanded ) );

    if( std::optional<wxString> error = saveAndReloadTable( **table, *scope ) )
    {
        std::erase_if( ( *table )->Rows(),
                       [&]( const LIBRARY_TABLE_ROW& aRow )
                       {
                           return aRow.Nickname() == nickname;
                       } );

        return libraryStatus( LibraryCommandStatus::LCS_ERROR, *error );
    }

    LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( project );

    if( aCtx.Request.create_library() && adapter && !wxFileName::Exists( expanded ) )
    {
        if( !adapter->CreateLibrary( nickname ) )
        {
            return libraryStatus( LibraryCommandStatus::LCS_ERROR,
                                  wxString::Format( _( "Added '%s', but could not create %s" ), nickname,
                                                    expanded ) );
        }
    }

    if( adapter )
        adapter->LoadLibraryEntry( nickname );

    return libraryStatus( LibraryCommandStatus::LCS_OK );
}


HANDLER_RESULT<LibraryCommandStatus>
API_HANDLER_LIBRARIES::handleUpdateLibraryTableEntry( const HANDLER_CONTEXT<UpdateLibraryTableEntry>& aCtx )
{
    const kiapi::common::types::LibraryTableEntry& entry = aCtx.Request.entry();

    if( entry.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
        return tl::unexpected( unhandled() );

    std::optional<LIBRARY_TABLE_SCOPE> scope = singleScope( aCtx.Request.scope() );

    if( !scope )
        return tl::unexpected( badRequest( "scope must be LTS_GLOBAL or LTS_PROJECT" ) );

    if( entry.uri().empty() )
        return tl::unexpected( badRequest( "a library entry needs a uri" ) );

    const wxString                nickname = wxString::FromUTF8( entry.nickname() );
    std::optional<LIBRARY_TABLE*> table = Pgm().GetLibraryManager().Table( m_type, *scope );
    std::optional<LIBRARY_TABLE_ROW*> row;

    if( table )
        row = ( *table )->Row( nickname );

    if( !row )
    {
        return libraryStatus( LibraryCommandStatus::LCS_NOT_FOUND,
                              wxString::Format( _( "Library '%s' not found" ), nickname ) );
    }

    if( ( *table )->IsReadOnly() )
        return libraryStatus( LibraryCommandStatus::LCS_READ_ONLY, ( *table )->Path() );

    PROJECT& project = Pgm().GetSettingsManager().Prj();
    applyEntry( **row, entry );
    ( *row )->SetType( pluginTypeForUri( ExpandEnvVarSubstitutions( ( *row )->URI(), &project ) ) );

    if( std::optional<wxString> error = saveAndReloadTable( **table, *scope ) )
        return libraryStatus( LibraryCommandStatus::LCS_ERROR, *error );

    return libraryStatus( LibraryCommandStatus::LCS_OK );
}


HANDLER_RESULT<LibraryCommandStatus>
API_HANDLER_LIBRARIES::handleDeleteLibraryTableEntry( const HANDLER_CONTEXT<DeleteLibraryTableEntry>& aCtx )
{
    if( aCtx.Request.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
        return tl::unexpected( unhandled() );

    std::optional<LIBRARY_TABLE_SCOPE> scope = singleScope( aCtx.Request.scope() );

    if( !scope )
        return tl::unexpected( badRequest( "scope must be LTS_GLOBAL or LTS_PROJECT" ) );

    const wxString                nickname = wxString::FromUTF8( aCtx.Request.nickname() );
    std::optional<LIBRARY_TABLE*> table = Pgm().GetLibraryManager().Table( m_type, *scope );

    if( !table || !( *table )->HasRow( nickname ) )
    {
        return libraryStatus( LibraryCommandStatus::LCS_NOT_FOUND,
                              wxString::Format( _( "Library '%s' not found" ), nickname ) );
    }

    if( ( *table )->IsReadOnly() )
        return libraryStatus( LibraryCommandStatus::LCS_READ_ONLY, ( *table )->Path() );

    // Only the table row goes; the library file itself is left alone
    std::erase_if( ( *table )->Rows(),
                   [&]( const LIBRARY_TABLE_ROW& aRow )
                   {
                       return aRow.Nickname() == nickname;
                   } );

    if( std::optional<wxString> error = saveAndReloadTable( **table, *scope ) )
        return libraryStatus( LibraryCommandStatus::LCS_ERROR, *error );

    return libraryStatus( LibraryCommandStatus::LCS_OK );
}


int API_HANDLER_LIBRARIES::scoreSearchMatch( LIBRARY_MANAGER_ADAPTER& aAdapter, const wxString& aNickname,
                                             const wxString& aItemName, const std::vector<wxString>& aTerms ) const
{
    const wxString name = aItemName.Lower();
    int            score = 0;

    for( const wxString& term : aTerms )
    {
        if( term.Contains( '*' ) || term.Contains( '?' ) )
        {
            if( !name.Matches( term ) )
                return 0;

            score += 10;
        }
        else if( name == term )
        {
            score += 100;
        }
        else if( name.StartsWith( term ) )
        {
            score += 40;
        }
        else if( name.Contains( term ) )
        {
            score += 20;
        }
        else
        {
            return 0;
        }
    }

    return score;
}


HANDLER_RESULT<SearchLibrariesResponse>
API_HANDLER_LIBRARIES::handleSearchLibraries( const HANDLER_CONTEXT<SearchLibraries>& aCtx )
{
    if( aCtx.Request.type() != ToProtoEnum<LIBRARY_TABLE_TYPE, LibraryType>( m_type ) )
        return tl::unexpected( unhandled() );

    std::vector<wxString> terms;
    wxStringTokenizer     tokenizer( wxString::FromUTF8( aCtx.Request.query() ).Lower(), wxS( " \t" ) );

    while( tokenizer.HasMoreTokens() )
        terms.push_back( tokenizer.GetNextToken() );

    if( terms.empty() )
        return tl::unexpected( badRequest( "the search query is empty" ) );

    LIBRARY_MANAGER_ADAPTER* adapter = adapterForProject( Pgm().GetSettingsManager().Prj() );

    if( !adapter )
        return tl::unexpected( badRequest( "no library adapter is available for this library type" ) );

    struct MATCH
    {
        int      score;
        wxString nickname;
        wxString name;
    };

    std::vector<MATCH> matches;
    int                loading = 0;

    for( const LIBRARY_TABLE_ROW* row : adapter->Rows() )
    {
        if( row->Disabled() )
            continue;

        // Asking an unloaded library for its names loads it on this (the UI) thread, which can
        // take minutes across all libraries; search what is ready and let the rest load behind
        if( !adapter->IsLibraryLoaded( row->Nickname() ) )
        {
            loading++;
            continue;
        }

        for( const wxString& name : getItemNames( *adapter, row->Nickname() ) )
        {
            if( int score = scoreSearchMatch( *adapter, row->Nickname(), name, terms ) )
                matches.push_back( { score, row->Nickname(), name } );
        }
    }

    // Best first; ties keep library-table order
    std::stable_sort( matches.begin(), matches.end(),
                      []( const MATCH& aLeft, const MATCH& aRight )
                      {
                          return aLeft.score > aRight.score;
                      } );

    if( loading )
        adapter->AsyncLoad();

    SearchLibrariesResponse response;
    response.set_libraries_loading( loading );

    for( const MATCH& match : matches )
    {
        kiapi::common::types::LibraryIdentifier* id = response.add_items();
        id->set_library_nickname( match.nickname.ToUTF8() );
        id->set_entry_name( match.name.ToUTF8() );
    }

    return response;
}


bool API_HANDLER_LIBRARIES::packLibraryItem( const LIB_ID& aId, google::protobuf::Any& aOutput ) const
{
    DESIGN_BLOCK_LIBRARY_ADAPTER* adapter =
            static_cast<DESIGN_BLOCK_LIBRARY_ADAPTER*>( adapterForProject( Pgm().GetSettingsManager().Prj() ) );

    DESIGN_BLOCK* designBlock = adapter->LoadDesignBlock( aId.GetLibNickname(), aId.GetLibItemName() );

    if( !designBlock )
        return false;

    designBlock->Serialize( aOutput );
    return true;
}

