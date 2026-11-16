/*
 * This program source code file is part of KiCad, a free EDA CAD application.
 *
 * Copyright (C) 2026 Benjamin Chung (ckfinite@gmail.com)
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

#include <api/api_handler_footprint.h>
#include <api/api_pcb_utils.h>
#include <api/api_enums.h>
#include <api/api_utils.h>
#include <board.h>
#include <board_commit.h>
#include <footprint.h>
#include <footprint_edit_frame.h>
#include <pcb_3d_body.h>
#include <ki_exception.h>
#include <pad.h>
#include <pcb_group.h>
#include <project.h>
#include <zone.h>
#include <footprint_library_adapter.h>
#include <project_pcb.h>
#include <embedded_files.h>
#include <fmt.h>
#include <pcb_painter.h>
#include <pcb_plot_params.h>
#include <pcbplot.h>
#include <plotters/plotter_png.h>
#include <settings/color_settings.h>
#include <wx/ffile.h>
#include <wx/filename.h>

#include <api/common/types/base_types.pb.h>

using namespace kiapi::common::commands;
using types::CommandStatus;
using types::DocumentType;
using types::ItemRequestStatus;


API_HANDLER_FOOTPRINT::API_HANDLER_FOOTPRINT( FOOTPRINT_EDIT_FRAME* aFrame ) :
        API_HANDLER_FOOTPRINT( CreateFootprintFrameContext( aFrame ), aFrame )
{
}


API_HANDLER_FOOTPRINT::API_HANDLER_FOOTPRINT( std::shared_ptr<FOOTPRINT_CONTEXT> aContext,
                                              FOOTPRINT_EDIT_FRAME* aFrame ) :
        API_HANDLER_BOARD( std::move( aContext ), aFrame )
{
    registerHandler<OpenLibraryItem, Empty>(
            &API_HANDLER_FOOTPRINT::handleOpenLibraryItem );
    registerHandler<GetOpenDocuments, GetOpenDocumentsResponse>(
            &API_HANDLER_FOOTPRINT::handleGetOpenDocuments );
    registerHandler<SaveDocument, Empty>( &API_HANDLER_FOOTPRINT::handleSaveDocument );
    registerHandler<SaveCopyOfDocument, Empty>(
            &API_HANDLER_FOOTPRINT::handleSaveCopyOfDocument );
    registerHandler<RevertDocument, Empty>( &API_HANDLER_FOOTPRINT::handleRevertDocument );

    registerHandler<GetItems, GetItemsResponse>( &API_HANDLER_FOOTPRINT::handleGetItems );

    using namespace kiapi::board::commands;
    registerHandler<NewLibraryFootprint, LibraryFootprintResponse>(
            &API_HANDLER_FOOTPRINT::handleNewLibraryFootprint );
    registerHandler<EditLibraryFootprint, LibraryFootprintResponse>(
            &API_HANDLER_FOOTPRINT::handleEditLibraryFootprint );
    registerHandler<SetLibraryFootprintProperties, Empty>(
            &API_HANDLER_FOOTPRINT::handleSetLibraryFootprintProperties );
    registerHandler<RenderLibraryFootprint, RenderLibraryFootprintResponse>(
            &API_HANDLER_FOOTPRINT::handleRenderLibraryFootprint );
    registerHandler<EmbedFileFromPath, EmbedFileFromPathResponse>( &API_HANDLER_FOOTPRINT::handleEmbedFileFromPath );
    registerHandler<GetEmbeddedFileList, EmbeddedFileListResponse>(
            &API_HANDLER_FOOTPRINT::handleGetEmbeddedFileList );
    registerHandler<RemoveEmbeddedFile, Empty>( &API_HANDLER_FOOTPRINT::handleRemoveEmbeddedFile );
}


FOOTPRINT_EDIT_FRAME* API_HANDLER_FOOTPRINT::frame() const
{
    return static_cast<FOOTPRINT_EDIT_FRAME*>( m_frame );
}


tl::expected<bool, ApiResponseStatus> API_HANDLER_FOOTPRINT::validateDocumentInternal( const DocumentSpecifier& aDocument ) const
{
    if( aDocument.type() != DocumentType::DOCTYPE_FOOTPRINT )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    // An empty library nickname addresses an unsaved new footprint
    if( aDocument.lib_id().library_nickname().empty() )
    {
        BOARD* board = this->board();

        if( !board || !board->GetFirstFootprint() )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( "no footprint is currently open" );
            return tl::unexpected( e );
        }

        return true;
    }

    LIB_ID target_fp = footprintContext()->GetLoadedFPID();

    if( !target_fp.IsValid() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no footprint is currently open" );
        return tl::unexpected( e );
    }

    std::string actual_lib  = target_fp.GetUniStringLibNickname().ToStdString();
    std::string actual_name = target_fp.GetUniStringLibItemName().ToStdString();

    if( 0 != aDocument.lib_id().library_nickname().compare( actual_lib ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the requested library is {} but the actual library is {}",
            aDocument.lib_id().library_nickname(), actual_lib ) );
        return tl::unexpected( e );
    }

    if( 0 != aDocument.lib_id().entry_name().compare( actual_name ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the requested footprint name is {} but the actual name is {}",
            aDocument.lib_id().entry_name(), actual_name ) );
        return tl::unexpected( e );
    }

    return true;
}


HANDLER_RESULT<FOOTPRINT*> API_HANDLER_FOOTPRINT::validateAndGetFootprint(
        const DocumentSpecifier& aDocument )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aDocument ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    FOOTPRINT* editorFootprint = board()->GetFirstFootprint();

    if( !editorFootprint )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no footprint is currently loaded" );
        return tl::unexpected( e );
    }

    return editorFootprint;
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleOpenLibraryItem(
    const HANDLER_CONTEXT<OpenLibraryItem>& aCtx )
{
    if( std::optional<ApiResponseStatus> headless = checkForHeadless( "OpenLibraryItem" ) )
        return tl::unexpected( *headless );

    if( aCtx.Request.type() != DocumentType::DOCTYPE_FOOTPRINT )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    FOOTPRINT_LIBRARY_ADAPTER* adapter = PROJECT_PCB::FootprintLibAdapter( &frame()->Prj() );

    wxString libraryName = aCtx.Request.identifier().library_nickname();
    wxString fpName = aCtx.Request.identifier().entry_name();

    LIB_ID fpid( libraryName, fpName );
    // preload the footprint to make sure it exists and so that we can make a nice error
    try
    {
        std::unique_ptr<FOOTPRINT> footprint(
                adapter->LoadFootprintWithOptionalNickname( fpid, true ) );

        if( !footprint )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( "could not open footprint" );
            return tl::unexpected( e );
        }
    }
    catch( const IO_ERROR& err )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "could not open footprint due to IO error {}",
                                          err.Problem().ToStdString() ) );
        return tl::unexpected( e );
    }

    frame()->LoadFootprintFromLibrary( fpid );
    return Empty();
}


HANDLER_RESULT<GetOpenDocumentsResponse> API_HANDLER_FOOTPRINT::handleGetOpenDocuments(
        const HANDLER_CONTEXT<GetOpenDocuments>& aCtx )
{
    if( aCtx.Request.type() != DocumentType::DOCTYPE_FOOTPRINT )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    GetOpenDocumentsResponse response;
    common::types::DocumentSpecifier doc;

    LIB_ID fpid = footprintContext()->GetLoadedFPID();

    if( !board()->GetFirstFootprint() )
        return response;

    doc.set_type( DocumentType::DOCTYPE_FOOTPRINT );
    doc.mutable_lib_id()->set_library_nickname( fpid.GetUniStringLibNickname() );
    doc.mutable_lib_id()->set_entry_name( fpid.GetUniStringLibItemName() );

    if( !project().IsNullProject() )
    {
        doc.mutable_project()->set_name( project().GetProjectName().ToStdString() );
        doc.mutable_project()->set_path( project().GetProjectDirectory().ToStdString() );
    }

    response.mutable_documents()->Add( std::move( doc ) );
    return response;
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleSaveDocument(
        const HANDLER_CONTEXT<SaveDocument>& aCtx )
{
    HANDLER_RESULT<FOOTPRINT*> footprint = validateAndGetFootprint( aCtx.Request.document() );

    if( !footprint )
        return tl::unexpected( footprint.error() );

    if( !footprintContext()->SaveFootprint( *footprint ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "failed to save footprint" );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleSaveCopyOfDocument(
        const HANDLER_CONTEXT<SaveCopyOfDocument>& aCtx )
{
    HANDLER_RESULT<FOOTPRINT*> footprint = validateAndGetFootprint( aCtx.Request.document() );

    if( !footprint )
        return tl::unexpected( footprint.error() );

    wxString pathStr = wxString::FromUTF8( aCtx.Request.path() );

    if( pathStr.IsEmpty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "path must contain the new footprint name, "
                             "optionally prefixed with a library nickname (e.g. \"lib:name\")" );
        return tl::unexpected( e );
    }

    // path can be "NewName" (same library) or "LibNick:NewName" (different library)
    LIB_ID targetId;
    targetId.Parse( pathStr );

    wxString libraryName = targetId.GetLibNickname();

    if( libraryName.IsEmpty() )
        libraryName = footprintContext()->GetLoadedFPID().GetUniStringLibNickname();

    wxString newName = targetId.GetLibItemName();

    // Clone the footprint so we don't modify the one being edited
    std::unique_ptr<FOOTPRINT> copy( static_cast<FOOTPRINT*>( ( *footprint )->Clone() ) );
    copy->SetFPID( LIB_ID( libraryName, newName ) );

    if( !footprintContext()->SaveFootprintInLibrary( copy.get(), libraryName ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "failed to save footprint copy '{}' to library '{}'",
                                          newName.ToStdString(),
                                          libraryName.ToStdString() ) );
        return tl::unexpected( e );
    }

    return Empty();
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleRevertDocument(
        const HANDLER_CONTEXT<RevertDocument>& aCtx )
{
    if( std::optional<ApiResponseStatus> headless = checkForHeadless( "RevertDocument" ) )
        return tl::unexpected( *headless );

    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    frame()->RevertFootprint( /* aSkipConfirmation = */ true );

    return Empty();
}


// Footprint types that are directly retrievable by GetItems
static const std::vector<KICAD_T> s_allowedFootprintTypes = {
        PCB_PAD_T,
        PCB_SHAPE_T,
        PCB_FIELD_T,
        PCB_TEXT_T,
        PCB_TEXTBOX_T,
        PCB_TABLE_T,
        PCB_TABLECELL_T,
        PCB_DIMENSION_T,
        PCB_ZONE_T,
        PCB_GROUP_T,
        PCB_BARCODE_T,
        PCB_3D_BODY_T
};


HANDLER_RESULT<GetItemsResponse> API_HANDLER_FOOTPRINT::handleGetItems( const HANDLER_CONTEXT<GetItems>& aCtx )
{
    if( !validateItemHeaderDocument( aCtx.Request.header() ) )
    {
        ApiResponseStatus e;
        // No message needed for AS_UNHANDLED; this is an internal flag for the API server
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        return tl::unexpected( e );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    GetItemsResponse response;

    FOOTPRINT* footprint = board()->GetFirstFootprint();
    std::vector<BOARD_ITEM*> items;
    std::set<KICAD_T> typesRequested, typesHandled;

    std::vector<KICAD_T> requestedTypes = parseRequestedItemTypes( aCtx.Request.types() );

    if( aCtx.Request.types().empty() )
        requestedTypes.assign( s_allowedFootprintTypes.begin(), s_allowedFootprintTypes.end() );

    for( KICAD_T type : requestedTypes )
    {
        typesRequested.emplace( type );

        if( typesHandled.count( type ) )
            continue;

        switch( type )
        {
        case PCB_PAD_T:
            std::ranges::copy( footprint->Pads(), std::back_inserter( items ) );
            typesHandled.insert( PCB_PAD_T );
            break;

        case PCB_FIELD_T:
            std::ranges::copy( footprint->GetFields(), std::back_inserter( items ) );
            typesHandled.insert( PCB_FIELD_T );
            break;

        case PCB_SHAPE_T:
        case PCB_TEXT_T:
        case PCB_TEXTBOX_T:
        case PCB_BARCODE_T:
        case PCB_TABLE_T:
        case PCB_DIMENSION_T:
            for( BOARD_ITEM* item : footprint->GraphicalItems() )
            {
                if( item->Type() == type || BaseType( item->Type() ) == type )
                    items.emplace_back( item );
            }

            typesHandled.insert( type );
            break;

        case PCB_TABLECELL_T:
            for( BOARD_ITEM* item : footprint->GraphicalItems() )
            {
                if( item->Type() != PCB_TABLE_T )
                    continue;

                item->RunOnChildren(
                        [&]( BOARD_ITEM* child )
                        {
                            items.emplace_back( child );
                        },
                        RECURSE_MODE::NO_RECURSE );
            }

            typesHandled.insert( PCB_TABLECELL_T );
            break;

        case PCB_ZONE_T:
            std::ranges::copy( footprint->Zones(), std::back_inserter( items ) );
            typesHandled.insert( PCB_ZONE_T );
            break;

        case PCB_GROUP_T:
            std::ranges::copy( footprint->Groups(), std::back_inserter( items ) );
            typesHandled.insert( PCB_GROUP_T );
            break;

        case PCB_3D_BODY_T:
            for( PCB_3D_BODY* body : footprint->Bodies() )
            {
                if( body->GetExtruded()->m_height > 0 )
                    items.push_back( body );
            }

            typesHandled.insert( PCB_3D_BODY_T );
            break;

        default:
            break;
        }
    }

    if( typesHandled.empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "none of the requested types are valid for a Footprint object" );
        return tl::unexpected( e );
    }

    for( const BOARD_ITEM* item : items )
    {
        if( !typesRequested.count( item->Type() ) && !typesRequested.count( BaseType( item->Type() ) ) )
            continue;

        google::protobuf::Any itemBuf;
        item->Serialize( itemBuf );
        response.mutable_items()->Add( std::move( itemBuf ) );
    }

    response.set_status( ItemRequestStatus::IRS_OK );
    return response;
}


BOARD_ITEM_CONTAINER* API_HANDLER_FOOTPRINT::getDefaultContainer()
{
    return board()->GetFirstFootprint();
}


namespace
{
ApiResponseStatus fpApiError( ApiStatusCode aCode, const std::string& aMessage )
{
    ApiResponseStatus e;
    e.set_status( aCode );
    e.set_error_message( aMessage );
    return e;
}


void packEmbeddedFileInfo( kiapi::common::commands::EmbeddedFileInfo& aOut,
                           const EMBEDDED_FILES::EMBEDDED_FILE& aFile )
{
    aOut.set_name( aFile.name.ToUTF8() );
    aOut.set_type( ToProtoEnum<EMBEDDED_FILES::EMBEDDED_FILE::FILE_TYPE, kiapi::common::types::EmbeddedFileType>(
            aFile.type ) );
    aOut.set_uri( aFile.GetLink().ToUTF8() );
    aOut.set_stored_size( static_cast<int64_t>( aFile.compressedEncodedData.size() ) );
}


void setMountingStyle( FOOTPRINT& aFootprint, kiapi::board::types::FootprintMountingStyle aStyle )
{
    int attributes = aFootprint.GetAttributes() & ~( FP_SMD | FP_THROUGH_HOLE );

    if( aStyle == kiapi::board::types::FMS_SMD )
        attributes |= FP_SMD;
    else if( aStyle == kiapi::board::types::FMS_THROUGH_HOLE )
        attributes |= FP_THROUGH_HOLE;

    aFootprint.SetAttributes( attributes );
}
} // namespace


kiapi::board::commands::LibraryFootprintResponse
API_HANDLER_FOOTPRINT::openFootprintDocument( const LIB_ID& aId ) const
{
    kiapi::board::commands::LibraryFootprintResponse response;
    types::DocumentSpecifier* doc = response.mutable_document();
    doc->set_type( DocumentType::DOCTYPE_FOOTPRINT );
    doc->mutable_lib_id()->set_library_nickname( aId.GetUniStringLibNickname().ToUTF8() );
    doc->mutable_lib_id()->set_entry_name( aId.GetUniStringLibItemName().ToUTF8() );

    if( !project().IsNullProject() )
    {
        doc->mutable_project()->set_name( project().GetProjectName().ToUTF8() );
        doc->mutable_project()->set_path( project().GetProjectDirectory().ToUTF8() );
    }

    return response;
}


HANDLER_RESULT<kiapi::board::commands::LibraryFootprintResponse> API_HANDLER_FOOTPRINT::handleNewLibraryFootprint(
        const HANDLER_CONTEXT<kiapi::board::commands::NewLibraryFootprint>& aCtx )
{
    if( std::optional<ApiResponseStatus> headless = checkForHeadless( "NewLibraryFootprint" ) )
        return tl::unexpected( *headless );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    const wxString library = wxString::FromUTF8( aCtx.Request.library_nickname() );
    const wxString name = wxString::FromUTF8( aCtx.Request.name() );

    if( library.IsEmpty() || name.IsEmpty() )
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST, "library_nickname and name are required" ) );

    if( name.find_first_of( wxS( ":/\\" ) ) != wxString::npos )
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST, "footprint names cannot contain : / or \\" ) );

    FOOTPRINT_LIBRARY_ADAPTER* adapter = PROJECT_PCB::FootprintLibAdapter( &frame()->Prj() );

    if( !adapter->HasLibrary( library, true ) )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "footprint library '{}' is not in the library tables",
                                                        aCtx.Request.library_nickname() ) ) );
    }

    if( !adapter->IsFootprintLibWritable( library ) )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "footprint library '{}' is read-only",
                                                        aCtx.Request.library_nickname() ) ) );
    }

    if( adapter->FootprintExists( library, name ) && !aCtx.Request.replace_existing() )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "'{}' already exists in '{}'", aCtx.Request.name(),
                                                        aCtx.Request.library_nickname() ) ) );
    }

    // Opening another footprint would ask what to do with these edits, which no client can answer
    if( !frame()->CanOpenFootprintWithoutPrompt() )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BUSY,
                                           "the Footprint Editor holds unsaved changes outside a tab" ) );
    }

    if( !frame()->BeginNewFootprint( library ) )
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BUSY, "the Footprint Editor declined a new footprint" ) );

    FOOTPRINT* footprint = frame()->CreateNewFootprint( name, library );

    if( !footprint )
        return tl::unexpected( fpApiError( ApiStatusCode::AS_UNKNOWN, "the footprint could not be created" ) );

    // CreateNewFootprint makes the name unique; a replacement keeps the requested one.  Set before
    // the footprint is installed so the editor records it as the loaded name (a later save must not
    // look like a rename, which deletes the library entry of the old name).
    const LIB_ID id( library, name );
    footprint->SetFPID( id );
    footprint->SetValue( name );

    if( aCtx.Request.mounting_style() != kiapi::board::types::FMS_UNKNOWN )
        setMountingStyle( *footprint, aCtx.Request.mounting_style() );

    frame()->AddFootprintToBoard( footprint );

    // Nets and netclasses must exist before pads can be drawn, as the New Footprint tool does
    board()->BuildListOfNets();
    footprint->SetPosition( VECTOR2I( 0, 0 ) );
    footprint->ClearFlags();

    frame()->Zoom_Automatique( false );
    frame()->GetScreen()->SetContentModified();
    frame()->OnModify();
    frame()->UpdateView();
    frame()->GetCanvas()->ForceRefresh();
    frame()->Show( true );
    frame()->Raise();

    return openFootprintDocument( id );
}


HANDLER_RESULT<kiapi::board::commands::LibraryFootprintResponse> API_HANDLER_FOOTPRINT::handleEditLibraryFootprint(
        const HANDLER_CONTEXT<kiapi::board::commands::EditLibraryFootprint>& aCtx )
{
    if( std::optional<ApiResponseStatus> headless = checkForHeadless( "EditLibraryFootprint" ) )
        return tl::unexpected( *headless );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    const LIB_ID id( wxString::FromUTF8( aCtx.Request.id().library_nickname() ),
                     wxString::FromUTF8( aCtx.Request.id().entry_name() ) );

    FOOTPRINT_LIBRARY_ADAPTER* adapter = PROJECT_PCB::FootprintLibAdapter( &frame()->Prj() );

    if( !adapter->FootprintExists( id.GetUniStringLibNickname(), id.GetUniStringLibItemName() ) )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "footprint {}:{} not found",
                                                        aCtx.Request.id().library_nickname(),
                                                        aCtx.Request.id().entry_name() ) ) );
    }

    if( !frame()->CanOpenFootprintWithoutPrompt() )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BUSY,
                                           "the Footprint Editor holds unsaved changes outside a tab" ) );
    }

    frame()->LoadFootprintFromLibrary( id );
    frame()->Show( true );
    frame()->Raise();

    if( footprintContext()->GetLoadedFPID() != id )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "could not open {}:{} in the Footprint Editor",
                                                        aCtx.Request.id().library_nickname(),
                                                        aCtx.Request.id().entry_name() ) ) );
    }

    return openFootprintDocument( id );
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleSetLibraryFootprintProperties(
        const HANDLER_CONTEXT<kiapi::board::commands::SetLibraryFootprintProperties>& aCtx )
{
    const kiapi::board::commands::SetLibraryFootprintProperties& request = aCtx.Request;
    HANDLER_RESULT<FOOTPRINT*> found = validateAndGetFootprint( request.document() );

    if( !found )
        return tl::unexpected( found.error() );

    FOOTPRINT*    footprint = *found;
    BOARD_COMMIT* commit = static_cast<BOARD_COMMIT*>( getCurrentCommit( aCtx.ClientName ) );
    commit->Modify( footprint );

    if( request.has_description() )
        footprint->SetLibDescription( wxString::FromUTF8( request.description() ) );

    if( request.has_keywords() )
        footprint->SetKeywords( wxString::FromUTF8( request.keywords() ) );

    if( request.has_mounting_style() )
        setMountingStyle( *footprint, request.mounting_style() );

    if( request.has_exclude_from_bill_of_materials() )
        footprint->SetExcludedFromBOM( request.exclude_from_bill_of_materials() );

    if( request.has_exclude_from_position_files() )
        footprint->SetExcludedFromPosFiles( request.exclude_from_position_files() );

    if( request.has_exclude_from_simulation() )
        footprint->SetExcludedFromSim( request.exclude_from_simulation() );

    if( request.has_not_in_schematic() )
        footprint->SetBoardOnly( request.not_in_schematic() );

    if( request.has_allow_soldermask_bridges() )
        footprint->SetAllowSolderMaskBridges( request.allow_soldermask_bridges() );

    if( request.has_exempt_from_courtyard_requirement() )
        footprint->SetAllowMissingCourtyard( request.exempt_from_courtyard_requirement() );

    if( request.has_value() )
        footprint->SetValue( wxString::FromUTF8( request.value() ) );

    if( request.has_datasheet() )
        footprint->GetField( FIELD_T::DATASHEET )->SetText( wxString::FromUTF8( request.datasheet() ) );

    if( request.set_models() )
    {
        footprint->Models().clear();

        for( const kiapi::board::types::Footprint3DModel& modelMsg : request.models() )
        {
            FP_3DMODEL model;
            model.m_Filename = wxString::FromUTF8( modelMsg.filename() );
            model.m_Show = modelMsg.visible();
            model.m_Opacity = modelMsg.opacity() > 0 ? modelMsg.opacity() : 1.0;

            // Unset vectors mean identity, not a zero scale
            model.m_Scale = modelMsg.has_scale() ? kiapi::common::UnpackVector3D( modelMsg.scale() )
                                                 : VECTOR3D( 1, 1, 1 );
            model.m_Rotation = kiapi::common::UnpackVector3D( modelMsg.rotation() );
            model.m_Offset = kiapi::common::UnpackVector3D( modelMsg.offset() );
            footprint->Models().push_back( std::move( model ) );
        }
    }

    if( !m_activeClients.contains( aCtx.ClientName ) )
        pushCurrentCommit( aCtx.ClientName, _( "Edited footprint properties via API" ) );

    if( frame() )
    {
        frame()->UpdateView();
        frame()->Update3DView( true, true );
    }

    return Empty();
}


HANDLER_RESULT<kiapi::board::commands::RenderLibraryFootprintResponse>
API_HANDLER_FOOTPRINT::handleRenderLibraryFootprint(
        const HANDLER_CONTEXT<kiapi::board::commands::RenderLibraryFootprint>& aCtx )
{
    if( std::optional<ApiResponseStatus> headless = checkForHeadless( "RenderLibraryFootprint" ) )
        return tl::unexpected( *headless );

    HANDLER_RESULT<FOOTPRINT*> found = validateAndGetFootprint( aCtx.Request.document() );

    if( !found )
        return tl::unexpected( found.error() );

    LSEQ layers;

    for( int layer : aCtx.Request.layers() )
    {
        PCB_LAYER_ID id = FromProtoEnum<PCB_LAYER_ID, kiapi::board::types::BoardLayer>(
                static_cast<kiapi::board::types::BoardLayer>( layer ) );

        if( id != UNDEFINED_LAYER && id != UNSELECTED_LAYER )
            layers.push_back( id );
    }

    if( layers.empty() )
        layers = { B_Cu, F_Cu, F_Paste, F_Fab, F_CrtYd, F_SilkS };

    COLOR_SETTINGS* colors = frame()->GetColorSettings();

    BOX2I bbox = ( *found )->GetBoundingBox( true );
    bbox.Inflate( pcbIUScale.mmToIU( 1.0 ) );

    const int    dpi = std::clamp( aCtx.Request.dpi() > 0 ? aCtx.Request.dpi() : 600, MIN_PNG_DPI, MAX_PNG_DPI );
    const double iuPerInch = pcbIUScale.IU_PER_MILS * 1000.0;

    PCB_PLOT_PARAMS plotOpts;
    plotOpts.SetFormat( PLOT_FORMAT::PNG );
    plotOpts.SetColorSettings( colors );
    plotOpts.SetPlotPadNumbers( true );
    plotOpts.SetPlotReference( true );
    plotOpts.SetPlotValue( true );
    plotOpts.SetSketchPadsOnFabLayers( false );
    plotOpts.SetDrillMarksType( DRILL_MARKS::FULL_DRILL_SHAPE );

    KIGFX::PCB_RENDER_SETTINGS renderSettings;
    renderSettings.LoadColors( colors );
    renderSettings.SetDefaultPenWidth( pcbIUScale.mmToIU( 0.0212 ) );

    PNG_PLOTTER plotter;
    plotter.SetPixelSize( std::max( 1, KiROUND( bbox.GetWidth() * dpi / iuPerInch ) ),
                          std::max( 1, KiROUND( bbox.GetHeight() * dpi / iuPerInch ) ) );
    plotter.SetResolution( dpi );
    plotter.SetAntialias( true );
    plotter.SetRenderSettings( &renderSettings );
    plotter.SetColorMode( true );
    plotter.SetViewport( bbox.GetOrigin(), pcbIUScale.IU_PER_MILS / 10, 1.0, false );
    plotter.SetCreator( wxT( "Pcbnew-PNG" ) );

    wxString path = wxFileName::CreateTempFileName( wxS( "kicad-footprint-render" ) );
    wxRemoveFile( path );
    path += wxS( ".png" );

    bool ok = plotter.OpenFile( path );

    if( ok )
    {
        plotter.StartPlot( wxS( "1" ) );
        plotter.SetColor( renderSettings.GetBackgroundColor() );
        plotter.Rect( bbox.GetOrigin(), bbox.GetEnd(), FILL_T::FILLED_SHAPE, 0, 0 );
        PlotBoardLayers( board(), &plotter, layers, plotOpts );
        plotter.EndPlot();
    }

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
        return tl::unexpected( fpApiError( ApiStatusCode::AS_UNKNOWN, "the footprint could not be rendered" ) );

    kiapi::board::commands::RenderLibraryFootprintResponse response;
    response.set_image( std::move( bytes ) );
    response.set_mime_type( "image/png" );
    return response;
}


HANDLER_RESULT<EmbedFileFromPathResponse> API_HANDLER_FOOTPRINT::handleEmbedFileFromPath(
        const HANDLER_CONTEXT<EmbedFileFromPath>& aCtx )
{
    HANDLER_RESULT<FOOTPRINT*> found = validateAndGetFootprint( aCtx.Request.document() );

    if( !found )
        return tl::unexpected( found.error() );

    wxFileName path( wxString::FromUTF8( aCtx.Request.path() ) );

    if( !path.IsAbsolute() || !path.FileExists() )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "'{}' is not an absolute path to a file KiCad can read",
                                                        aCtx.Request.path() ) ) );
    }

    EMBEDDED_FILES* files = ( *found )->GetEmbeddedFiles();

    if( files->HasFile( path.GetFullName() ) && !aCtx.Request.overwrite() )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "a file named '{}' is already embedded",
                                                        path.GetFullName().ToStdString() ) ) );
    }

    BOARD_COMMIT* commit = static_cast<BOARD_COMMIT*>( getCurrentCommit( aCtx.ClientName ) );
    commit->Modify( *found );

    EMBEDDED_FILES::EMBEDDED_FILE* file = files->AddFile( path, aCtx.Request.overwrite() );

    if( !file )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_UNKNOWN,
                                           fmt::format( "could not embed '{}'", aCtx.Request.path() ) ) );
    }

    EmbedFileFromPathResponse response;
    packEmbeddedFileInfo( *response.mutable_file(), *file );

    if( !m_activeClients.contains( aCtx.ClientName ) )
        pushCurrentCommit( aCtx.ClientName, _( "Embedded a file via API" ) );

    return response;
}


HANDLER_RESULT<EmbeddedFileListResponse> API_HANDLER_FOOTPRINT::handleGetEmbeddedFileList(
        const HANDLER_CONTEXT<GetEmbeddedFileList>& aCtx )
{
    HANDLER_RESULT<FOOTPRINT*> found = validateAndGetFootprint( aCtx.Request.document() );

    if( !found )
        return tl::unexpected( found.error() );

    EmbeddedFileListResponse response;

    for( const auto& [name, file] : ( *found )->GetEmbeddedFiles()->EmbeddedFileMap() )
        packEmbeddedFileInfo( *response.add_files(), *file );

    return response;
}


HANDLER_RESULT<Empty> API_HANDLER_FOOTPRINT::handleRemoveEmbeddedFile( const HANDLER_CONTEXT<RemoveEmbeddedFile>& aCtx )
{
    HANDLER_RESULT<FOOTPRINT*> found = validateAndGetFootprint( aCtx.Request.document() );

    if( !found )
        return tl::unexpected( found.error() );

    const wxString  name = wxString::FromUTF8( aCtx.Request.name() );
    EMBEDDED_FILES* files = ( *found )->GetEmbeddedFiles();

    if( !files->HasFile( name ) )
    {
        return tl::unexpected( fpApiError( ApiStatusCode::AS_BAD_REQUEST,
                                           fmt::format( "no embedded file named '{}'", aCtx.Request.name() ) ) );
    }

    BOARD_COMMIT* commit = static_cast<BOARD_COMMIT*>( getCurrentCommit( aCtx.ClientName ) );
    commit->Modify( *found );
    files->RemoveFile( name );

    if( !m_activeClients.contains( aCtx.ClientName ) )
        pushCurrentCommit( aCtx.ClientName, _( "Removed an embedded file via API" ) );

    return Empty();
}
