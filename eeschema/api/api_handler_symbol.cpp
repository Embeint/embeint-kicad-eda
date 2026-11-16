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

#include <api/api_handler_symbol.h>

#include <api/api_sch_utils.h>
#include <api/api_utils.h>
#include <view/view.h>
#include <fmt.h>
#include <magic_enum.hpp>
#include <base_units.h>
#include <lib_symbol.h>
#include <project.h>
#include <sch_bitmap.h>
#include <sch_commit.h>
#include <sch_field.h>
#include <sch_painter.h>
#include <sch_pin.h>
#include <sch_plotter.h>
#include <sch_screen.h>
#include <sch_shape.h>
#include <sch_table.h>
#include <sch_text.h>
#include <sch_textbox.h>
#include <symbol_edit_frame.h>
#include <wildcards_and_files_ext.h>

#include <wx/ffile.h>
#include <wx/filename.h>

using namespace kiapi::common::commands;
using kiapi::common::commands::DocumentModifiedState;
using kiapi::common::types::DocumentType;
using kiapi::common::types::ItemRequestStatus;


std::set<KICAD_T> API_HANDLER_SYMBOL::s_allowedTypes = {
    LIB_SYMBOL_T, SCH_FIELD_T,   SCH_PIN_T,   SCH_SHAPE_T,     SCH_BITMAP_T,
    SCH_TEXT_T,   SCH_TEXTBOX_T, SCH_TABLE_T, SCH_TABLECELL_T,
};


API_HANDLER_SYMBOL::API_HANDLER_SYMBOL( SYMBOL_EDIT_FRAME* aFrame ) :
        API_HANDLER_SYMBOL( CreateSymbolEditorFrameContext( aFrame ), aFrame )
{
}


API_HANDLER_SYMBOL::API_HANDLER_SYMBOL( std::shared_ptr<SYMBOL_EDITOR_CONTEXT> aContext, SYMBOL_EDIT_FRAME* aFrame ) :
        API_HANDLER_EDITOR( aFrame ),
        m_context( std::move( aContext ) )
{
    registerHandler<OpenLibraryItem, Empty>( &API_HANDLER_SYMBOL::handleOpenLibraryItem );
    registerHandler<GetOpenDocuments, GetOpenDocumentsResponse>( &API_HANDLER_SYMBOL::handleGetOpenDocuments );
    registerHandler<SaveDocument, Empty>( &API_HANDLER_SYMBOL::handleSaveDocument );
    registerHandler<SaveCopyOfDocument, Empty>( &API_HANDLER_SYMBOL::handleSaveCopyOfDocument );
    registerHandler<RevertDocument, Empty>( &API_HANDLER_SYMBOL::handleRevertDocument );

    registerHandler<GetItems, GetItemsResponse>( &API_HANDLER_SYMBOL::handleGetItems );
    registerHandler<GetItemsById, GetItemsResponse>( &API_HANDLER_SYMBOL::handleGetItemsById );
    registerHandler<kiapi::schematic::commands::SetLibrarySymbolProperties, Empty>(
            &API_HANDLER_SYMBOL::handleSetLibrarySymbolProperties );
    registerHandler<kiapi::schematic::commands::RenderLibrarySymbol,
                    kiapi::schematic::commands::RenderLibrarySymbolResponse>(
            &API_HANDLER_SYMBOL::handleRenderLibrarySymbol );
}


SYMBOL_EDIT_FRAME* API_HANDLER_SYMBOL::frame() const
{
    return static_cast<SYMBOL_EDIT_FRAME*>( m_frame );
}


std::unique_ptr<COMMIT> API_HANDLER_SYMBOL::createCommit()
{
    if( m_frame )
        return std::make_unique<SCH_COMMIT>( static_cast<SYMBOL_EDIT_FRAME*>( m_frame ) );

    return std::make_unique<SCH_COMMIT>( context()->GetToolManager(), /* aIsLibEditor */ true );
}


tl::expected<bool, ApiResponseStatus>
API_HANDLER_SYMBOL::validateDocumentInternal( const DocumentSpecifier& aDocument ) const
{
    if( aDocument.type() != DocumentType::DOCTYPE_SYMBOL )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    const PROJECT& prj = context()->Prj();

    if( aDocument.has_project() )
    {
        if( aDocument.project().name().compare( prj.GetProjectName().ToUTF8() ) != 0 )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( fmt::format( "the requested project {} is not open", aDocument.project().name() ) );
            return tl::unexpected( e );
        }

        if( aDocument.project().path().compare( prj.GetProjectPath().ToUTF8() ) != 0 )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( fmt::format( "the requested project {} is not open at path {}",
                                              aDocument.project().name(), aDocument.project().path() ) );
            return tl::unexpected( e );
        }
    }

    LIB_ID loaded = context()->GetLoadedLibId();

    // An empty library nickname addresses an unsaved new symbol
    if( aDocument.lib_id().library_nickname().empty() )
    {
        if( loaded.GetLibItemName().empty() )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( "no symbol is currently open" );
            return tl::unexpected( e );
        }

        return true;
    }

    if( !loaded.IsValid() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no symbol is currently open" );
        return tl::unexpected( e );
    }

    if( aDocument.lib_id().library_nickname() != loaded.GetUniStringLibNickname().ToStdString() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the requested library is {} but the open library is {}",
                                          aDocument.lib_id().library_nickname(),
                                          loaded.GetUniStringLibNickname().ToStdString() ) );
        return tl::unexpected( e );
    }

    if( aDocument.lib_id().entry_name() != loaded.GetUniStringLibItemName().ToStdString() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the requested symbol is {} but the open symbol is {}",
                                          aDocument.lib_id().entry_name(),
                                          loaded.GetUniStringLibItemName().ToStdString() ) );
        return tl::unexpected( e );
    }

    return true;
}

HANDLER_RESULT<commands::GetOpenDocumentsResponse>
API_HANDLER_SYMBOL::handleGetOpenDocuments( const HANDLER_CONTEXT<commands::GetOpenDocuments>& aCtx )
{
    if( aCtx.Request.type() != DocumentType::DOCTYPE_SYMBOL )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    GetOpenDocumentsResponse response;

    if( !context()->GetCurSymbol() )
        return response;

    types::DocumentSpecifier doc;
    LIB_ID libId = context()->GetCurSymbol()->GetLibId();

    doc.set_type( DocumentType::DOCTYPE_SYMBOL );
    doc.mutable_lib_id()->set_library_nickname( libId.GetUniStringLibNickname().ToStdString() );
    doc.mutable_lib_id()->set_entry_name( libId.GetUniStringLibItemName().ToStdString() );

    PROJECT& prj = context()->Prj();

    if( !prj.IsNullProject() )
    {
        doc.mutable_project()->set_name( prj.GetProjectName().ToUTF8() );
        doc.mutable_project()->set_path( prj.GetProjectPath().ToUTF8() );
    }

    response.mutable_documents()->Add( std::move( doc ) );
    return response;
}


HANDLER_RESULT<Empty>
API_HANDLER_SYMBOL::handleOpenLibraryItem( const HANDLER_CONTEXT<commands::OpenLibraryItem>& aCtx )
{
    if( aCtx.Request.type() != DocumentType::DOCTYPE_SYMBOL )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    LIB_ID libId = UnpackLibId( aCtx.Request.identifier() );

    if( !libId.IsValid() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "could not parse library identifier {}.{}",
                                          aCtx.Request.identifier().library_nickname(),
                                          aCtx.Request.identifier().entry_name() ) );
        return tl::unexpected( e );
    }

    if( !context()->LoadSymbol( libId ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "could not open symbol {}.{}",
                                          aCtx.Request.identifier().library_nickname(),
                                          aCtx.Request.identifier().entry_name() ) );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<Empty> API_HANDLER_SYMBOL::handleSaveDocument( const HANDLER_CONTEXT<commands::SaveDocument>& aCtx )
{
    if( context()->GetLoadedLibId().GetLibNickname().empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "the open symbol has no library yet; use SaveCopyOfDocument with a "
                             "path to save it" );
        return tl::unexpected( e );
    }

    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    if( !context()->SaveSymbol() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "failed to save symbol (is the library read-only?)" );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<Empty>
API_HANDLER_SYMBOL::handleSaveCopyOfDocument( const HANDLER_CONTEXT<commands::SaveCopyOfDocument>& aCtx )
{
    HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    wxFileName libPath( wxString::FromUTF8( aCtx.Request.path() ) );

    if( !libPath.IsOk() || !libPath.IsDirWritable() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "save path '{}' could not be opened", libPath.GetFullPath().ToStdString() ) );
        return tl::unexpected( e );
    }

    if( libPath.FileExists() && ( !libPath.IsFileWritable() || !aCtx.Request.options().overwrite() ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message(
                fmt::format( "save path '{}' exists and cannot be overwritten", libPath.GetFullPath().ToStdString() ) );
        return tl::unexpected( e );
    }

    if( libPath.GetExt() != wxString( FILEEXT::KiCadSymbolLibFileExtension ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message(
                fmt::format( "save path '{}' must have a kicad_sym extension", libPath.GetFullPath().ToStdString() ) );
        return tl::unexpected( e );
    }

    if( !context()->SaveSymbolCopy( libPath.GetFullPath(), aCtx.Request.options().overwrite() ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "failed to save symbol copy" );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<Empty> API_HANDLER_SYMBOL::handleRevertDocument( const HANDLER_CONTEXT<commands::RevertDocument>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
    {
        return tl::unexpected( documentValidation.error() );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    if( !context()->RevertSymbol() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "failed to revert symbol" );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<commands::GetItemsResponse>
API_HANDLER_SYMBOL::handleGetItems( const HANDLER_CONTEXT<commands::GetItems>& aCtx )
{
    if( HANDLER_RESULT<std::optional<KIID>> valid = validateItemHeaderDocument( aCtx.Request.header() );
        !valid.has_value() )
    {
        return tl::unexpected( valid.error() );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    LIB_SYMBOL* symbol = context()->GetCurSymbol();

    if( !symbol )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no symbol is currently loaded" );
        return tl::unexpected( e );
    }

    GetItemsResponse     response;
    std::set<KICAD_T>    typesRequested;
    std::vector<KICAD_T> requestedTypes = parseRequestedItemTypes( aCtx.Request.types() );

    if( aCtx.Request.types().empty() )
        requestedTypes.assign( s_allowedTypes.begin(), s_allowedTypes.end() );

    for( KICAD_T type : requestedTypes )
    {
        if( !s_allowedTypes.contains( type ) )
            continue;

        typesRequested.insert( type );
    }

    if( typesRequested.empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "none of the requested types are valid for a Symbol object" );
        return tl::unexpected( e );
    }

    auto emitItem = [&]( EDA_ITEM* item )
    {
        if( !item || !typesRequested.contains( item->Type() ) )
            return;

        google::protobuf::Any any;
        item->Serialize( any );
        response.mutable_items()->Add( std::move( any ) );
    };

    if( typesRequested.contains( LIB_SYMBOL_T ) )
        emitItem( symbol );

    for( SCH_ITEM& item : symbol->GetDrawItems() )
        emitItem( &item );

    response.set_status( ItemRequestStatus::IRS_OK );
    return response;
}


HANDLER_RESULT<commands::GetItemsResponse>
API_HANDLER_SYMBOL::handleGetItemsById( const HANDLER_CONTEXT<commands::GetItemsById>& aCtx )
{
    if( HANDLER_RESULT<std::optional<KIID>> valid = validateItemHeaderDocument( aCtx.Request.header() );
        !valid.has_value() )
    {
        return tl::unexpected( valid.error() );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    LIB_SYMBOL* symbol = context()->GetCurSymbol();

    if( !symbol )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no symbol is currently loaded" );
        return tl::unexpected( e );
    }

    GetItemsResponse response;

    for( const types::KIID& idProto : aCtx.Request.items() )
    {
        KIID id( idProto.value() );

        if( id == symbol->m_Uuid )
        {
            google::protobuf::Any any;
            symbol->Serialize( any );
            response.mutable_items()->Add( std::move( any ) );
            continue;
        }

        for( SCH_ITEM& item : symbol->GetDrawItems() )
        {
            if( item.m_Uuid != id || !s_allowedTypes.contains( item.Type() ) )
                continue;

            google::protobuf::Any any;
            item.Serialize( any );
            response.mutable_items()->Add( std::move( any ) );
            break;
        }
    }

    response.set_status( ItemRequestStatus::IRS_OK );
    return response;
}


std::optional<EDA_ITEM*> API_HANDLER_SYMBOL::getItemFromDocument( const DocumentSpecifier& aDocument, const KIID& aId )
{
    if( !validateDocument( aDocument ).has_value() )
        return std::nullopt;

    LIB_SYMBOL* symbol = context()->GetCurSymbol();

    if( !symbol )
        return std::nullopt;

    if( symbol->m_Uuid == aId )
        return std::optional<EDA_ITEM*>( symbol );

    for( SCH_ITEM& item : symbol->GetDrawItems() )
    {
        if( item.m_Uuid == aId )
            return std::optional<EDA_ITEM*>( &item );
    }

    return std::nullopt;
}


HANDLER_RESULT<types::ItemRequestStatus> API_HANDLER_SYMBOL::handleCreateUpdateItemsInternal(
        bool aCreate, const std::string& aClientName, const types::ItemHeader& aHeader,
        const google::protobuf::RepeatedPtrField<google::protobuf::Any>& aItems,
        std::function<void( ItemStatus, google::protobuf::Any )>         aItemHandler )
{
    ApiResponseStatus e;

    HANDLER_RESULT<std::optional<KIID>> containerResult = validateItemHeaderDocument( aHeader );

    if( !containerResult && containerResult.error().status() == ApiStatusCode::AS_UNHANDLED )
    {
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }
    else if( !containerResult )
    {
        e.CopyFrom( containerResult.error() );
        return tl::unexpected( e );
    }

    LIB_SYMBOL* symbol = context()->GetCurSymbol();

    if( !symbol )
    {
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no symbol is currently loaded" );
        return tl::unexpected( e );
    }

    if( containerResult->has_value() && **containerResult != symbol->m_Uuid )
    {
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the requested container {} is not the open symbol",
                                          ( *containerResult )->AsString().ToStdString() ) );
        return tl::unexpected( e );
    }

    SCH_SCREEN* screen = context()->GetScreen();
    SCH_COMMIT* commit = static_cast<SCH_COMMIT*>( getCurrentCommit( aClientName ) );

    for( const google::protobuf::Any& anyItem : aItems )
    {
        ItemStatus             status;
        std::optional<KICAD_T> type = TypeNameFromAny( anyItem );

        if( !type )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_TYPE );
            status.set_error_message( fmt::format( "Could not decode a valid type from {}", anyItem.type_url() ) );
            aItemHandler( status, anyItem );
            continue;
        }

        if( !s_allowedTypes.contains( *type ) )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_TYPE );
            status.set_error_message( fmt::format( "type {} is not supported by the symbol editor API handler",
                                                   magic_enum::enum_name( *type ) ) );
            aItemHandler( status, anyItem );
            continue;
        }

        if( *type == LIB_SYMBOL_T )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_TYPE );
            status.set_error_message( "whole-symbol create/update is not supported; "
                                      "edit the symbol's child items instead" );
            aItemHandler( status, anyItem );
            continue;
        }

        if( *type == SCH_FIELD_T )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_TYPE );
            status.set_error_message( "fields must be edited through the parent symbol's properties" );
            aItemHandler( status, anyItem );
            continue;
        }

        std::unique_ptr<EDA_ITEM> created = CreateItemForType( *type, symbol );

        if( !created )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_TYPE );
            status.set_error_message(
                    fmt::format( "could not create an item of type {}", magic_enum::enum_name( *type ) ) );
            aItemHandler( status, anyItem );
            continue;
        }

        if( !created->Deserialize( anyItem ) )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_DATA );
            status.set_error_message(
                    fmt::format( "could not unpack {} from request", created->GetClass().ToStdString() ) );
            aItemHandler( status, anyItem );
            continue;
        }

        SCH_ITEM* newItem = static_cast<SCH_ITEM*>( created.get() );

        // Schematic graphics default to the notes layer; symbol graphics belong on the device
        // layer (body color and line style), as the symbol editor's drawing tools and the
        // library loader put them
        if( newItem->Type() == SCH_SHAPE_T || newItem->Type() == SCH_TEXT_T || newItem->Type() == SCH_TEXTBOX_T )
            newItem->SetLayer( LAYER_DEVICE );

        if( newItem->Type() == SCH_FIELD_T && static_cast<SCH_FIELD*>( newItem )->IsMandatory() )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_DATA );
            status.set_error_message( "mandatory fields cannot be created or replaced directly" );
            aItemHandler( status, anyItem );
            continue;
        }

        // Look up the existing item by UUID
        SCH_ITEM* existingItem = nullptr;

        for( SCH_ITEM& item : symbol->GetDrawItems() )
        {
            if( item.m_Uuid == newItem->m_Uuid )
            {
                existingItem = &item;
                break;
            }
        }

        if( aCreate && existingItem )
        {
            status.set_code( ItemStatusCode::ISC_EXISTING );
            status.set_error_message(
                    fmt::format( "an item with UUID {} already exists", newItem->m_Uuid.AsStdString() ) );
            aItemHandler( status, anyItem );
            continue;
        }
        else if( !aCreate && !existingItem )
        {
            status.set_code( ItemStatusCode::ISC_NONEXISTENT );
            status.set_error_message(
                    fmt::format( "an item with UUID {} does not exist", newItem->m_Uuid.AsStdString() ) );
            aItemHandler( status, anyItem );
            continue;
        }

        if( !aCreate && existingItem->Type() != *type )
        {
            status.set_code( ItemStatusCode::ISC_INVALID_DATA );
            status.set_error_message(
                    fmt::format( "item {} is not of the requested type", newItem->m_Uuid.AsStdString() ) );
            aItemHandler( status, anyItem );
            continue;
        }

        status.set_code( ItemStatusCode::ISC_OK );
        google::protobuf::Any responseItem;

        commit->Modify( symbol, screen );

        if( aCreate )
        {
            if( newItem->Type() == SCH_PIN_T )
                const_cast<KIID&>( newItem->m_Uuid ) = KIID();

            newItem->SetParent( symbol );
            symbol->AddDrawItem( newItem, false );
            created.release();
            newItem->Serialize( responseItem );

            if( m_frame && frame()->GetCanvas() )
                frame()->GetCanvas()->GetView()->Add( newItem );
        }
        else
        {
            SCH_ITEM* updated = existingItem;
            updated->SwapItemData( newItem );
            updated->Serialize( responseItem );

            if( m_frame && frame()->GetCanvas() )
                frame()->GetCanvas()->GetView()->Update( updated, KIGFX::GEOMETRY );
        }

        aItemHandler( status, responseItem );
    }

    if( !m_activeClients.contains( aClientName ) )
    {
        pushCurrentCommit( aClientName, aCreate ? _( "Created items via API" ) : _( "Modified items via API" ) );
    }

    return ItemRequestStatus::IRS_OK;
}


void API_HANDLER_SYMBOL::deleteItemsInternal( std::map<KIID, ItemDeletionStatus>& aItemsToDelete,
                                              const std::string& aClientName )
{
    LIB_SYMBOL* symbol = context()->GetCurSymbol();

    if( !symbol )
        return;

    SCH_SCREEN* screen = context()->GetScreen();
    SCH_COMMIT* commit = static_cast<SCH_COMMIT*>( getCurrentCommit( aClientName ) );

    for( auto& [id, status] : aItemsToDelete )
    {
        if( status != ItemDeletionStatus::IDS_NONEXISTENT )
            continue;

        SCH_ITEM* item = nullptr;

        for( SCH_ITEM& candidate : symbol->GetDrawItems() )
        {
            if( candidate.m_Uuid == id )
            {
                item = &candidate;
                break;
            }
        }

        if( !item || !s_allowedTypes.contains( item->Type() ) )
        {
            status = ItemDeletionStatus::IDS_IMMUTABLE;
            continue;
        }

        if( item->Type() == SCH_FIELD_T && static_cast<SCH_FIELD*>( item )->IsMandatory() )
        {
            status = ItemDeletionStatus::IDS_IMMUTABLE;
            continue;
        }

        commit->Modify( symbol, screen );

        if( m_frame && frame()->GetCanvas() )
            frame()->GetCanvas()->GetView()->Remove( item );

        symbol->RemoveDrawItem( item );
        status = ItemDeletionStatus::IDS_OK;
    }

    if( !m_activeClients.contains( aClientName ) )
        pushCurrentCommit( aClientName, _( "Deleted items via API" ) );
}


HANDLER_RESULT<Empty> API_HANDLER_SYMBOL::handleSetLibrarySymbolProperties(
        const HANDLER_CONTEXT<kiapi::schematic::commands::SetLibrarySymbolProperties>& aCtx )
{
    const kiapi::schematic::commands::SetLibrarySymbolProperties& request = aCtx.Request;

    if( HANDLER_RESULT<bool> documentValidation = validateDocument( request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    LIB_SYMBOL* libSymbol = symbol();

    if( !libSymbol )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no symbol is open" );
        return tl::unexpected( e );
    }

    for( const kiapi::schematic::commands::SymbolUserField& field : request.user_fields() )
    {
        wxString name = wxString::FromUTF8( field.name() );
        SCH_FIELD* existing = libSymbol->GetField( name );

        if( name.IsEmpty() || ( existing && existing->IsMandatory() ) )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( fmt::format( "'{}' is not a valid user field name; set mandatory fields "
                                              "through their own properties", field.name() ) );
            return tl::unexpected( e );
        }
    }

    SCH_SCREEN* screen = context()->GetScreen();
    SCH_COMMIT* commit = static_cast<SCH_COMMIT*>( getCurrentCommit( aCtx.ClientName ) );
    commit->Modify( libSymbol, screen );

    auto setText =
            []( SCH_FIELD& aField, const std::string& aText )
            {
                aField.SetText( wxString::FromUTF8( aText ) );
            };

    if( request.has_value() )
        setText( libSymbol->GetValueField(), request.value() );

    if( request.has_footprint() )
        setText( libSymbol->GetFootprintField(), request.footprint() );

    if( request.has_datasheet() )
        setText( libSymbol->GetDatasheetField(), request.datasheet() );

    if( request.has_description() )
        setText( libSymbol->GetDescriptionField(), request.description() );

    if( request.has_reference_prefix() )
        setText( libSymbol->GetReferenceField(), request.reference_prefix() );

    if( request.has_keywords() )
        libSymbol->SetKeyWords( wxString::FromUTF8( request.keywords() ) );

    if( request.set_footprint_filters() )
    {
        wxArrayString filters;

        for( const std::string& filter : request.footprint_filters() )
            filters.Add( wxString::FromUTF8( filter ) );

        libSymbol->SetFPFilters( filters );
    }

    if( request.has_show_pin_numbers() )
        libSymbol->SetShowPinNumbers( request.show_pin_numbers() );

    if( request.has_show_pin_names() )
        libSymbol->SetShowPinNames( request.show_pin_names() );

    if( request.has_exclude_from_bom() )
        libSymbol->SetExcludedFromBOM( request.exclude_from_bom() );

    if( request.has_exclude_from_board() )
        libSymbol->SetExcludedFromBoard( request.exclude_from_board() );

    if( request.has_exclude_from_sim() )
        libSymbol->SetExcludedFromSim( request.exclude_from_sim() );

    if( request.has_power_symbol() )
    {
        if( request.power_symbol() )
            libSymbol->SetGlobalPower();
        else
            libSymbol->SetNormal();
    }

    for( const std::string& name : request.remove_user_fields() )
    {
        SCH_FIELD* field = libSymbol->GetField( wxString::FromUTF8( name ) );

        if( field && !field->IsMandatory() )
        {
            // The canvas holds a pointer to every drawn item
            if( frame() && frame()->GetCanvas() )
                frame()->GetCanvas()->GetView()->Remove( field );

            libSymbol->RemoveField( field );
        }
    }

    // New user fields stack below the value field, like the properties dialog adds them
    VECTOR2I nextPosition = libSymbol->GetValueField().GetPosition();

    for( const kiapi::schematic::commands::SymbolUserField& userField : request.user_fields() )
    {
        wxString   name = wxString::FromUTF8( userField.name() );
        SCH_FIELD* field = libSymbol->GetField( name );

        if( !field )
        {
            field = new SCH_FIELD( libSymbol, FIELD_T::USER, name );
            nextPosition.y += schIUScale.MilsToIU( 100 );
            field->SetPosition( nextPosition );
            libSymbol->AddField( field );

            if( frame() && frame()->GetCanvas() )
                frame()->GetCanvas()->GetView()->Add( field );
        }

        field->SetText( wxString::FromUTF8( userField.value() ) );
        field->SetVisible( userField.visible() );
    }

    if( request.place_fields_around_body() )
    {
        BOX2I body = libSymbol->GetBodyBoundingBox( 1, 1, false, false );

        if( body.GetWidth() > 0 || body.GetHeight() > 0 )
        {
            // Designator above value at the body's bottom right.  Both are left-aligned on one
            // x, so their first letters line up, chosen so the longer text ends at the body's
            // right edge.  They sit just under the body unless that would cross a pin on the
            // bottom edge, in which case they drop below the pin ends.
            const int  step = schIUScale.MilsToIU( 100 );
            SCH_FIELD& reference = libSymbol->GetReferenceField();
            SCH_FIELD& value = libSymbol->GetValueField();

            reference.SetHorizJustify( GR_TEXT_H_ALIGN_LEFT );
            value.SetHorizJustify( GR_TEXT_H_ALIGN_LEFT );

            // Snapped to the 50 mil grid: x rounds left so the text stays within the body's
            // width, the top rounds down the page so it stays clear of what is above
            const int grid = schIUScale.MilsToIU( 50 );
            const int widest = std::max( reference.GetBoundingBox().GetWidth(), value.GetBoundingBox().GetWidth() );
            const int x = static_cast<int>( std::floor( double( body.GetRight() - widest ) / grid ) ) * grid;

            auto placeAt =
                    [&]( int aTop )
                    {
                        aTop = static_cast<int>( std::ceil( double( aTop ) / grid ) ) * grid;
                        reference.SetPosition( VECTOR2I( x, aTop ) );
                        value.SetPosition( VECTOR2I( x, aTop + step ) );
                    };

            placeAt( body.GetBottom() + step / 2 );

            BOX2I text = reference.GetBoundingBox();
            text.Merge( value.GetBoundingBox() );

            for( SCH_PIN* pin : libSymbol->GetGraphicalPins() )
            {
                if( pin->IsVisible() && pin->GetBoundingBox().Intersects( text ) )
                {
                    placeAt( libSymbol->GetBodyBoundingBox( 1, 1, true, false ).GetBottom() + step / 2 );
                    break;
                }
            }

            // Hidden fields (footprint, datasheet, manufacturer, ...) stack under the value
            int y = value.GetPosition().y + step;
            std::vector<SCH_FIELD*> fields;
            libSymbol->GetFields( fields );

            for( SCH_FIELD* field : fields )
            {
                if( field == &reference || field == &value || field->IsVisible() )
                    continue;

                field->SetHorizJustify( GR_TEXT_H_ALIGN_LEFT );
                field->SetPosition( VECTOR2I( x, y ) );
                y += step;
            }
        }
    }

    if( !m_activeClients.contains( aCtx.ClientName ) )
        pushCurrentCommit( aCtx.ClientName, _( "Edited symbol properties via API" ) );

    if( frame() )
    {
        frame()->RebuildView();
        frame()->UpdateSymbolMsgPanelInfo();
    }

    return Empty();
}


HANDLER_RESULT<kiapi::schematic::commands::RenderLibrarySymbolResponse> API_HANDLER_SYMBOL::handleRenderLibrarySymbol(
        const HANDLER_CONTEXT<kiapi::schematic::commands::RenderLibrarySymbol>& aCtx )
{
    const kiapi::schematic::commands::RenderLibrarySymbol& request = aCtx.Request;

    if( HANDLER_RESULT<bool> documentValidation = validateDocument( request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    LIB_SYMBOL* libSymbol = symbol();

    if( !libSymbol || !frame() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "rendering needs a symbol open in the Symbol Editor window" );
        return tl::unexpected( e );
    }

    const int unit = request.unit() > 0 ? request.unit() : std::max( 1, frame()->GetUnit() );
    const int bodyStyle = request.body_style() > 0 ? request.body_style() : std::max( 1, frame()->GetBodyStyle() );

    SCH_RENDER_SETTINGS renderSettings;
    renderSettings.LoadColors( frame()->GetColorSettings() );
    renderSettings.SetDefaultPenWidth( frame()->GetRenderSettings()->GetDefaultPenWidth() );

    // Show the symbol as a schematic will: hidden pins and fields stay hidden
    renderSettings.m_ShowHiddenPins = false;
    renderSettings.m_ShowHiddenFields = false;

    // A derived symbol's graphics and pins live in its parent: draw the flattened symbol
    std::unique_ptr<LIB_SYMBOL> flattened;
    LIB_SYMBOL*                 drawSymbol = libSymbol;

    if( libSymbol->IsDerived() )
    {
        flattened = libSymbol->Flatten();

        if( flattened )
            drawSymbol = flattened.get();
    }

    BOX2I bbox = GetSymbolPlotBBox( *drawSymbol, unit, bodyStyle, false );
    bbox.Inflate( schIUScale.MilsToIU( 100 ) );

    wxString path = wxFileName::CreateTempFileName( wxS( "kicad-symbol-render" ) );
    wxRemoveFile( path );
    path += request.svg() ? wxS( ".svg" ) : wxS( ".png" );

    bool ok = request.svg() ? PlotSymbolToSVG( *drawSymbol, *drawSymbol, unit, bodyStyle, bbox, renderSettings,
                                               false, path )
                            : PlotSymbolToPNG( *drawSymbol, *drawSymbol, unit, bodyStyle, bbox, renderSettings,
                                               request.dpi() > 0 ? request.dpi() : 600, path );

    std::string bytes;

    if( ok )
    {
        if( wxFFile file( path, wxS( "rb" ) ); file.IsOpened() )
        {
            bytes.resize( file.Length() );
            ok = file.Read( bytes.data(), bytes.size() ) == bytes.size();
        }
        else
        {
            ok = false;
        }
    }

    wxRemoveFile( path );

    if( !ok || bytes.empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNKNOWN );
        e.set_error_message( "the symbol could not be rendered" );
        return tl::unexpected( e );
    }

    kiapi::schematic::commands::RenderLibrarySymbolResponse response;
    response.set_image( std::move( bytes ) );
    response.set_mime_type( request.svg() ? "image/svg+xml" : "image/png" );
    return response;
}


void API_HANDLER_SYMBOL::onModified()
{
    if( frame() )
        frame()->OnModify();

    context()->SetContentModified();
}


void API_HANDLER_SYMBOL::pushCurrentCommit( const std::string& aClientName, const wxString& aMessage )
{
    API_HANDLER_EDITOR::pushCurrentCommit( aClientName, aMessage );
    onModified();
}


HANDLER_RESULT<commands::GetDocumentModifiedStateResponse>
API_HANDLER_SYMBOL::handleGetDocumentModifiedState( const HANDLER_CONTEXT<commands::GetDocumentModifiedState>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    GetDocumentModifiedStateResponse response;
    response.set_state( context()->IsContentModified() ? DocumentModifiedState::DMS_MODIFIED
                                                       : DocumentModifiedState::DMS_UNMODIFIED );
    return response;
}
