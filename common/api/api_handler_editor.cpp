/*
 * This program source code file is part of KiCad, a free EDA CAD application.
 *
 * Copyright (C) 2024 Jon Evans <jon@craftyjon.com>
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

#include <api/api_handler_editor.h>

#include <algorithm>
#include <optional>

#include <api/api_enums.h>
#include <api/api_operation_log.h>
#include <api/api_utils.h>
#include <class_draw_panel_gal.h>
#include <drawing_sheet/ds_data_item.h>
#include <drawing_sheet/ds_data_model.h>
#include <drawing_sheet/ds_draw_item.h>
#include <eda_base_frame.h>
#include <eda_draw_frame.h>
#include <eda_item.h>
#include <eda_units.h>
#include <geometry/seg.h>
#include <gal/gal_display_options.h>
#include <gal_display_options_common.h>
#include <settings/app_settings.h>
#include <settings/grid_settings.h>
#include <title_block.h>
#include <tool/actions.h>
#include <tool/common_tools.h>
#include <tool/tool_manager.h>
#include <wx/log.h>
#include <wx/time.h>
#include <wx/timer.h>
#include <wx/wx.h>

using namespace kiapi::common::commands;


namespace
{

/// Rolls back the API commits of clients that went away; ticks once a second while any is open
class API_COMMIT_LEASE_TIMER : public wxTimer
{
public:
    explicit API_COMMIT_LEASE_TIMER( std::function<void()> aOnTick ) :
            m_onTick( std::move( aOnTick ) )
    {
    }

    void Notify() override { m_onTick(); }

private:
    std::function<void()> m_onTick;
};


long long nowMs()
{
    return wxGetLocalTimeMillis().GetValue();
}


ApiResponseStatus contractError( ApiStatusCode aCode, const std::string& aMessage )
{
    ApiResponseStatus e;
    e.set_status( aCode );
    e.set_error_message( aMessage );
    return e;
}


/**
 * The document a request names: its first top-level DocumentSpecifier, or the document of its
 * first top-level ItemHeader.  Requests that nest their document deeper name none.
 */
const types::DocumentSpecifier* requestDocument( const google::protobuf::Message& aRequest )
{
    const google::protobuf::Descriptor* descriptor = aRequest.GetDescriptor();
    const google::protobuf::Reflection* reflection = aRequest.GetReflection();

    for( int ii = 0; ii < descriptor->field_count(); ++ii )
    {
        const google::protobuf::FieldDescriptor* field = descriptor->field( ii );

        if( field->is_repeated() || field->cpp_type() != google::protobuf::FieldDescriptor::CPPTYPE_MESSAGE
            || !reflection->HasField( aRequest, field ) )
        {
            continue;
        }

        const google::protobuf::Message& value = reflection->GetMessage( aRequest, field );

        if( auto document = google::protobuf::DynamicCastToGenerated<types::DocumentSpecifier>( &value ) )
            return document;

        if( auto header = google::protobuf::DynamicCastToGenerated<types::ItemHeader>( &value ) )
        {
            if( header->has_document() )
                return &header->document();
        }
    }

    return nullptr;
}

} // namespace


API_HANDLER_EDITOR::API_HANDLER_EDITOR( EDA_BASE_FRAME* aFrame ) :
        API_HANDLER(),
        m_frame( aFrame )
{
    registerHandler<BeginCommit, BeginCommitResponse>( &API_HANDLER_EDITOR::handleBeginCommit );
    registerHandler<EndCommit, EndCommitResponse>( &API_HANDLER_EDITOR::handleEndCommit );
    registerHandler<CreateItems, CreateItemsResponse>( &API_HANDLER_EDITOR::handleCreateItems );
    registerHandler<UpdateItems, UpdateItemsResponse>( &API_HANDLER_EDITOR::handleUpdateItems );
    registerHandler<DeleteItems, DeleteItemsResponse>( &API_HANDLER_EDITOR::handleDeleteItems );
    registerHandler<HitTest, HitTestResponse>( &API_HANDLER_EDITOR::handleHitTest );
    registerHandler<GetDocumentModifiedState, GetDocumentModifiedStateResponse>(
            &API_HANDLER_EDITOR::handleGetDocumentModifiedState );
    registerHandler<GetTitleBlockInfo, types::TitleBlockInfo>( &API_HANDLER_EDITOR::handleGetTitleBlockInfo );
    registerHandler<SetTitleBlockInfo, google::protobuf::Empty>( &API_HANDLER_EDITOR::handleSetTitleBlockInfo );
    registerHandler<UndoLastChange, UndoRedoResponse>( &API_HANDLER_EDITOR::handleUndoLastChange );
    registerHandler<RedoLastChange, UndoRedoResponse>( &API_HANDLER_EDITOR::handleRedoLastChange );
    registerHandler<GetPageLayout, PageLayoutResponse>( &API_HANDLER_EDITOR::handleGetPageLayout );
    registerHandler<GetGridSettings, GridSettings>( &API_HANDLER_EDITOR::handleGetGridSettings );
    registerHandler<SetGridSettings, GridSettings>( &API_HANDLER_EDITOR::handleSetGridSettings );
    registerHandler<GetDocumentRevision, DocumentRevisionResponse>( &API_HANDLER_EDITOR::handleGetDocumentRevision,
                                                                    true );

    m_revisionEpoch = KIID().AsStdString().substr( 0, 8 );
}


API_HANDLER_EDITOR::~API_HANDLER_EDITOR()
{
    if( m_leaseTimer )
        m_leaseTimer->Stop();
}


std::string API_HANDLER_EDITOR::revisionToken( uint64_t aCounter ) const
{
    return fmt::format( "{}-{}", m_revisionEpoch, aCounter );
}


std::string API_HANDLER_EDITOR::documentRevision() const
{
    return m_frame ? revisionToken( m_frame->GetDocumentRevision() ) : std::string();
}


std::optional<ApiResponseStatus> API_HANDLER_EDITOR::beforeRequest( const kiapi::common::ApiRequestHeader& aHeader,
                                                                    const google::protobuf::Message& aRequest )
{
    m_request = REQUEST_STATE();
    m_request.revisionBefore = documentRevision();

    const std::string& client = aHeader.client_name();
    const std::string& expected = aHeader.expected_revision();
    const std::string& operation = aHeader.operation_id();
    const bool         contract = !expected.empty() || !operation.empty();
    const bool         isEndCommit = google::protobuf::DynamicCastToGenerated<EndCommit>( &aRequest ) != nullptr;

    // Clean up after clients that went away before anything else touches the document
    expireCommits( client );

    if( const types::DocumentSpecifier* document = requestDocument( aRequest ) )
    {
        if( document->type() != thisDocumentType() )
        {
            // Another editor's document.  With a revision or operation id, make sure only the
            // editor that owns the document can run it.
            if( contract )
                return contractError( ApiStatusCode::AS_UNHANDLED, "" );

            return std::nullopt;
        }

        if( tl::expected<bool, ApiResponseStatus> valid = validateDocumentInternal( *document ); valid )
        {
            m_request.namesThisDocument = true;
            m_request.document.CopyFrom( *document );
        }
        else if( !expected.empty() )
        {
            m_request.refusedBeforeRun = true;
            return valid.error();
        }
    }

    if( !operation.empty() )
    {
        if( std::optional<std::string> problem = API_OPERATION_LOG::CheckId( operation ) )
        {
            m_request.doNotRecord = true;
            return contractError( ApiStatusCode::AS_BAD_REQUEST, *problem );
        }

        if( API_OPERATION_LOG::Instance().Contains( operation ) )
        {
            m_request.doNotRecord = true;
            return contractError( ApiStatusCode::AS_DUPLICATE_OPERATION,
                                  fmt::format( "operation {} was already received; the request was not run.  "
                                               "Use GetOperationStatus for its outcome",
                                               operation ) );
        }
    }

    // A client whose commit was rolled back on expiry must end it before doing anything else, so
    // later steps of its batch cannot land outside the commit
    if( auto it = m_expiredClients.find( client ); it != m_expiredClients.end() && !isEndCommit
                                                   && !m_commits.contains( client ) )
    {
        m_request.refusedBeforeRun = true;
        return contractError( ApiStatusCode::AS_BAD_REQUEST,
                              fmt::format( "the open commit of this client was rolled back ({}); nothing it "
                                           "staged was committed.  End the commit before continuing",
                                           it->second ) );
    }

    if( auto it = m_commitStates.find( client ); it != m_commitStates.end() )
    {
        COMMIT_STATE& state = it->second;
        state.lastActivityMs = nowMs();

        if( m_frame && m_frame->GetDocumentRevision() != state.lastSeenRevision && !state.foreignChange )
        {
            state.foreignChange = true;
            state.foreignChangeFrom = revisionToken( state.lastSeenRevision );
        }

        if( state.strict && state.foreignChange && !isEndCommit )
        {
            m_request.refusedBeforeRun = true;
            return contractError( ApiStatusCode::AS_REVISION_MISMATCH,
                                  fmt::format( "the document changed (revision {} to {}) while this commit was "
                                               "open; nothing was changed.  End the commit: it will be "
                                               "rolled back",
                                               state.foreignChangeFrom, documentRevision() ) );
        }
    }

    if( !expected.empty() )
    {
        m_request.refusedBeforeRun = true;

        if( !m_request.namesThisDocument )
        {
            return contractError( ApiStatusCode::AS_BAD_REQUEST,
                                  "expected_revision needs a request that names an open document" );
        }

        if( !m_frame )
        {
            return contractError( ApiStatusCode::AS_BAD_REQUEST,
                                  "revision checks need an editor window; this editor is headless" );
        }

        if( expected != documentRevision() )
        {
            return contractError( ApiStatusCode::AS_REVISION_MISMATCH,
                                  fmt::format( "the document is at revision {}, not {}; nothing was changed",
                                               documentRevision(), expected ) );
        }

        m_request.refusedBeforeRun = false;
    }

    return std::nullopt;
}


void API_HANDLER_EDITOR::afterRequest( const kiapi::common::ApiRequestHeader& aHeader,
                                       const google::protobuf::Message& aRequest, const ApiResponseStatus& aStatus )
{
    // Not this editor's request: the server offers it to the next handler
    if( aStatus.status() == ApiStatusCode::AS_UNHANDLED )
        return;

    const std::string& client = aHeader.client_name();
    const std::string& operation = aHeader.operation_id();
    const bool         ok = aStatus.status() == ApiStatusCode::AS_OK;
    const auto*        begin = google::protobuf::DynamicCastToGenerated<BeginCommit>( &aRequest );
    const bool         isRevisionQuery =
            google::protobuf::DynamicCastToGenerated<GetDocumentRevision>( &aRequest ) != nullptr;

    if( m_commits.contains( client ) )
    {
        // Explicit (BeginCommit) or implicit (a request that staged without pushing) commit
        auto [it, created] = m_commitStates.try_emplace( client );
        COMMIT_STATE& state = it->second;

        if( created )
        {
            state.revisionAtBegin = m_request.revisionBefore;

            if( begin && ok )
            {
                state.strict = !aHeader.expected_revision().empty();

                if( begin->lease_seconds() > 0 )
                {
                    state.leaseSeconds = static_cast<int>(
                            std::min<uint32_t>( begin->lease_seconds(), MAX_COMMIT_LEASE_S ) );
                }
            }
        }

        // Staged changes are already visible to readers.  Advance the revision so other clients'
        // checks see them, and attribute the change to this client's commit.
        if( m_frame && ( !created || !begin ) && !isRevisionQuery && !m_request.refusedBeforeRun )
            m_frame->AdvanceDocumentRevision();

        if( m_frame )
            state.lastSeenRevision = m_frame->GetDocumentRevision();

        state.lastActivityMs = nowMs();
    }

    const std::string revisionAfter = documentRevision();

    // Operations resolved by this request see the revision after any final onModified()
    for( const std::string& resolved : m_request.resolvedOperations )
    {
        OperationStatusResponse entry = API_OPERATION_LOG::Instance().Find( resolved );
        API_OPERATION_LOG::Instance().Resolve( resolved, entry.state(), revisionAfter, "" );
    }

    if( !operation.empty() && !m_request.doNotRecord )
    {
        OperationStatusResponse entry;
        entry.set_operation_id( operation );
        entry.set_client_name( client );
        entry.set_request_type( std::string( aRequest.GetTypeName() ) );
        entry.set_revision_before( m_request.revisionBefore );

        if( m_request.namesThisDocument )
            entry.mutable_document()->CopyFrom( m_request.document );

        auto commitState = m_commitStates.find( client );

        if( m_request.refusedBeforeRun )
        {
            entry.set_state( OperationState::OPS_REJECTED );
            entry.set_revision_after( revisionAfter );
            entry.set_message( aStatus.error_message() );
        }
        else if( commitState != m_commitStates.end() && m_commits.contains( client ) )
        {
            // Decided when the commit ends
            entry.set_state( OperationState::OPS_IN_PROGRESS );
            commitState->second.operations.push_back( operation );

            if( begin )
                entry.set_revision_before( commitState->second.revisionAtBegin );
        }
        else if( m_request.endCommitOutcome )
        {
            entry.set_state( *m_request.endCommitOutcome );
            entry.set_revision_after( revisionAfter );
            entry.set_message( aStatus.error_message() );
        }
        else
        {
            entry.set_state( ok ? OperationState::OPS_COMMITTED : OperationState::OPS_FAILED );
            entry.set_revision_after( revisionAfter );
            entry.set_message( aStatus.error_message() );
        }

        API_OPERATION_LOG::Instance().Record( entry );
    }

    if( m_request.namesThisDocument && m_frame )
        m_responseHeader.set_document_revision( revisionAfter );

    ensureLeaseTimer();
}


void API_HANDLER_EDITOR::ensureLeaseTimer()
{
    if( !m_frame )
        return;

    if( m_commitStates.empty() )
    {
        if( m_leaseTimer )
            m_leaseTimer->Stop();

        return;
    }

    if( !m_leaseTimer )
    {
        m_leaseTimer = std::make_unique<API_COMMIT_LEASE_TIMER>(
                [this]()
                {
                    expireCommits();
                    ensureLeaseTimer();
                } );
    }

    if( !m_leaseTimer->IsRunning() )
        m_leaseTimer->Start( 1000 );
}


void API_HANDLER_EDITOR::expireCommits( const std::string& aExceptClient )
{
    if( m_commitStates.empty() )
        return;

    // Never revert under an open dialog or an interactive tool; try again on the next tick
    if( m_frame && !m_frame->CanAcceptApiCommands() )
        return;

    const long long          now = nowMs();
    std::vector<std::string> expired;

    for( const auto& [client, state] : m_commitStates )
    {
        if( client != aExceptClient && now - state.lastActivityMs >= state.leaseSeconds * 1000LL )
            expired.push_back( client );
    }

    for( const std::string& client : expired )
    {
        const int lease = m_commitStates.at( client ).leaseSeconds;
        rollBackCommit( client, fmt::format( "its client sent nothing for {} s and the lease expired", lease ),
                        true );
    }
}


void API_HANDLER_EDITOR::rollBackCommit( const std::string& aClientName, const std::string& aReason,
                                         bool aExpired )
{
    wxLogTrace( traceApi, wxS( "API: rolling back the commit of %s: %s" ), wxString::FromUTF8( aClientName ),
                wxString::FromUTF8( aReason ) );

    if( auto it = m_commits.find( aClientName ); it != m_commits.end() )
    {
        it->second.second->Revert();
        m_commits.erase( it );
    }

    const bool wasExplicit = m_activeClients.erase( aClientName ) > 0;

    if( m_frame )
        m_frame->AdvanceDocumentRevision();

    finishCommit( aClientName, OperationState::OPS_ROLLED_BACK, aReason );

    if( aExpired && wasExplicit )
    {
        if( !m_expiredClients.contains( aClientName ) )
            m_expiredOrder.push_back( aClientName );

        m_expiredClients[aClientName] = aReason;

        while( m_expiredOrder.size() > MAX_EXPIRED_CLIENTS )
        {
            m_expiredClients.erase( m_expiredOrder.front() );
            m_expiredOrder.pop_front();
        }
    }
}


void API_HANDLER_EDITOR::finishCommit( const std::string& aClientName, OperationState aState,
                                       const std::string& aMessage )
{
    auto it = m_commitStates.find( aClientName );

    if( it == m_commitStates.end() )
        return;

    const std::string revision = documentRevision();

    for( const std::string& operation : it->second.operations )
    {
        API_OPERATION_LOG::Instance().Resolve( operation, aState, revision, aMessage );
        m_request.resolvedOperations.push_back( operation );
    }

    m_commitStates.erase( it );
}


HANDLER_RESULT<DocumentRevisionResponse> API_HANDLER_EDITOR::handleGetDocumentRevision(
        const HANDLER_CONTEXT<GetDocumentRevision>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( !m_frame )
    {
        return tl::unexpected( contractError( ApiStatusCode::AS_UNIMPLEMENTED,
                                              "document revisions need an editor window" ) );
    }

    DocumentRevisionResponse response;
    response.set_revision( documentRevision() );
    response.set_open_commits( static_cast<int>( m_commits.size() ) );
    response.set_modified( m_frame->IsContentModified() );
    return response;
}


HANDLER_RESULT<BeginCommitResponse> API_HANDLER_EDITOR::handleBeginCommit(
        const HANDLER_CONTEXT<BeginCommit>& aCtx )
{
    // Before 11.0, commit requests had no header so we assume they are for the PCB editor
    if( aCtx.Request.has_header() )
    {
        HANDLER_RESULT<std::optional<KIID>> valid = validateItemHeaderDocument( aCtx.Request.header() );

        if( !valid )
            return tl::unexpected( valid.error() );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    if( m_commits.count( aCtx.ClientName ) )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the client {} already has a commit in progress",
                                          aCtx.ClientName ) );
        return tl::unexpected( e );
    }

    wxASSERT( !m_activeClients.count( aCtx.ClientName ) );

    BeginCommitResponse response;

    KIID id;
    m_commits[aCtx.ClientName] = std::make_pair( id, createCommit() );
    response.mutable_id()->set_value( id.AsStdString() );

    m_activeClients.insert( aCtx.ClientName );

    return response;
}


HANDLER_RESULT<EndCommitResponse> API_HANDLER_EDITOR::handleEndCommit(
        const HANDLER_CONTEXT<EndCommit>& aCtx )
{
    // Before 11.0, commit requests had no header so we assume they are for the PCB editor
    if( aCtx.Request.has_header() )
    {
        HANDLER_RESULT<std::optional<KIID>> valid = validateItemHeaderDocument( aCtx.Request.header() );

        if( !valid )
            return tl::unexpected( valid.error() );
    }

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    if( !m_commits.count( aCtx.ClientName ) )
    {
        // The commit was rolled back when its lease expired.  Dropping it confirms that outcome.
        if( auto it = m_expiredClients.find( aCtx.ClientName ); it != m_expiredClients.end() )
        {
            const std::string reason = it->second;
            m_expiredClients.erase( it );
            std::erase( m_expiredOrder, aCtx.ClientName );
            m_request.endCommitOutcome = OperationState::OPS_ROLLED_BACK;

            if( aCtx.Request.action() == kiapi::common::commands::CMA_DROP )
                return EndCommitResponse();

            return tl::unexpected( contractError( ApiStatusCode::AS_BAD_REQUEST,
                                                  fmt::format( "nothing was committed: the commit was rolled "
                                                               "back because {}",
                                                               reason ) ) );
        }

        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( fmt::format( "the client {} does not has a commit in progress",
                                          aCtx.ClientName ) );
        return tl::unexpected( e );
    }

    wxASSERT( m_activeClients.count( aCtx.ClientName ) );

    const std::pair<KIID, std::unique_ptr<COMMIT>>& pair = m_commits.at( aCtx.ClientName );
    const KIID& id = pair.first;
    const std::unique_ptr<COMMIT>& commit = pair.second;

    EndCommitResponse response;

    // Do not check IDs with drop; it is a safety net in case the id was lost on the client side
    switch( aCtx.Request.action() )
    {
    case kiapi::common::commands::CMA_DROP:
    {
        commit->Revert();
        m_commits.erase( aCtx.ClientName );
        m_activeClients.erase( aCtx.ClientName );

        // Readers may have seen the staged changes
        if( m_frame )
            m_frame->AdvanceDocumentRevision();

        finishCommit( aCtx.ClientName, OperationState::OPS_ROLLED_BACK, "dropped by its client" );
        m_request.endCommitOutcome = OperationState::OPS_ROLLED_BACK;
        break;
    }

    case kiapi::common::commands::CMA_COMMIT:
    {
        if( aCtx.Request.id().value().compare( id.AsStdString() ) != 0 )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( fmt::format( "the id {} does not match the commit in progress",
                                              aCtx.Request.id().value() ) );
            return tl::unexpected( e );
        }

        // A commit opened with an expected revision must not land on top of someone else's change
        if( auto it = m_commitStates.find( aCtx.ClientName );
            it != m_commitStates.end() && it->second.strict && it->second.foreignChange )
        {
            const std::string reason =
                    fmt::format( "another client or the user changed the document (revision {} to {}) while "
                                 "the commit was open; the commit was rolled back",
                                 it->second.foreignChangeFrom, documentRevision() );

            rollBackCommit( aCtx.ClientName, reason, false );
            m_request.endCommitOutcome = OperationState::OPS_ROLLED_BACK;
            return tl::unexpected( contractError( ApiStatusCode::AS_REVISION_MISMATCH, reason ) );
        }

        pushCurrentCommit( aCtx.ClientName, wxString( aCtx.Request.message().c_str(), wxConvUTF8 ) );
        m_request.endCommitOutcome = OperationState::OPS_COMMITTED;
        break;
    }

    default:
        break;
    }

    return response;
}


COMMIT* API_HANDLER_EDITOR::getCurrentCommit( const std::string& aClientName )
{
    if( !m_commits.count( aClientName ) )
    {
        KIID id;
        m_commits[aClientName] = std::make_pair( id, createCommit() );
    }

    return m_commits.at( aClientName ).second.get();
}


void API_HANDLER_EDITOR::pushCurrentCommit( const std::string& aClientName,
                                            const wxString& aMessage )
{
    auto it = m_commits.find( aClientName );

    if( it == m_commits.end() )
        return;

    it->second.second->Push( aMessage.IsEmpty() ? m_defaultCommitMessage : aMessage );
    m_commits.erase( it );
    m_activeClients.erase( aClientName );

    finishCommit( aClientName, OperationState::OPS_COMMITTED, "" );
}


HANDLER_RESULT<bool> API_HANDLER_EDITOR::validateDocument( const DocumentSpecifier& aDocument )
{
    if( tl::expected<bool, ApiResponseStatus> validation = validateDocumentInternal( aDocument ); !validation )
        return tl::unexpected( validation.error() );

    return true;
}


HANDLER_RESULT<std::optional<KIID>> API_HANDLER_EDITOR::validateItemHeaderDocument(
        const types::ItemHeader& aHeader )
{
    if( !aHeader.has_document() || aHeader.document().type() != thisDocumentType() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_UNHANDLED );
        // No error message, this is a flag that the server should try a different handler
        return tl::unexpected( e );
    }

    HANDLER_RESULT<bool> documentValidation = validateDocument( aHeader.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( tl::expected<bool, ApiResponseStatus> result = validateDocumentInternal( aHeader.document() ); !result )
        return tl::unexpected( result.error() );

    if( aHeader.has_container() && !aHeader.container().value().empty() )
    {
        return KIID( aHeader.container().value() );
    }

    // Valid header, but no container provided
    return std::nullopt;
}


std::optional<ApiResponseStatus> API_HANDLER_EDITOR::checkForBusy()
{
    if( !m_frame )
        return std::nullopt;

    if( !m_frame->CanAcceptApiCommands() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BUSY );
        e.set_error_message( "KiCad is busy and cannot respond to API requests right now" );
        return e;
    }

    return std::nullopt;
}


HANDLER_RESULT<CreateItemsResponse> API_HANDLER_EDITOR::handleCreateItems(
        const HANDLER_CONTEXT<CreateItems>& aCtx )
{
    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    CreateItemsResponse response;

    // The dedicated CreateItems.container field (when set) overrides any container in the header
    types::ItemHeader header;
    header.CopyFrom( aCtx.Request.header() );

    if( aCtx.Request.container().value().length() )
        *header.mutable_container() = aCtx.Request.container();

    HANDLER_RESULT<ItemRequestStatus> result = handleCreateUpdateItemsInternal( true,
            aCtx.ClientName,
            header, aCtx.Request.items(),
            [&]( const ItemStatus& aStatus, const google::protobuf::Any& aItem )
            {
                ItemCreationResult itemResult;
                itemResult.mutable_status()->CopyFrom( aStatus );
                itemResult.mutable_item()->CopyFrom( aItem );
                response.mutable_created_items()->Add( std::move( itemResult ) );
            } );

    if( !result.has_value() )
        return tl::unexpected( result.error() );

    response.set_status( *result );
    return response;
}


HANDLER_RESULT<UpdateItemsResponse> API_HANDLER_EDITOR::handleUpdateItems(
        const HANDLER_CONTEXT<UpdateItems>& aCtx )
{
    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    UpdateItemsResponse response;

    HANDLER_RESULT<ItemRequestStatus> result = handleCreateUpdateItemsInternal( false,
            aCtx.ClientName,
            aCtx.Request.header(), aCtx.Request.items(),
            [&]( const ItemStatus& aStatus, const google::protobuf::Any& aItem )
            {
                ItemUpdateResult itemResult;
                itemResult.mutable_status()->CopyFrom( aStatus );
                itemResult.mutable_item()->CopyFrom( aItem );
                response.mutable_updated_items()->Add( std::move( itemResult ) );
            } );

    if( !result.has_value() )
        return tl::unexpected( result.error() );

    response.set_status( *result );
    return response;
}


HANDLER_RESULT<DeleteItemsResponse> API_HANDLER_EDITOR::handleDeleteItems(
        const HANDLER_CONTEXT<DeleteItems>& aCtx )
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

    std::map<KIID, ItemDeletionStatus> itemsToDelete;

    for( const kiapi::common::types::KIID& kiidBuf : aCtx.Request.item_ids() )
    {
        if( !kiidBuf.value().empty() )
        {
            KIID kiid( kiidBuf.value() );
            itemsToDelete[kiid] = ItemDeletionStatus::IDS_NONEXISTENT;
        }
    }

    if( itemsToDelete.empty() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "no valid items to delete were given" );
        return tl::unexpected( e );
    }

    deleteItemsInternal( itemsToDelete, aCtx.ClientName );

    DeleteItemsResponse response;

    for( const auto& [id, status] : itemsToDelete )
    {
        ItemDeletionResult* result = response.add_deleted_items();
        result->mutable_id()->set_value( id.AsStdString() );
        result->set_status( status );
    }

    response.set_status( kiapi::common::types::ItemRequestStatus::IRS_OK );
    return response;
}


HANDLER_RESULT<HitTestResponse> API_HANDLER_EDITOR::handleHitTest(
        const HANDLER_CONTEXT<HitTest>& aCtx )
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

    HitTestResponse response;

    std::optional<EDA_ITEM*> item = getItemFromDocument( aCtx.Request.header().document(),
                                                         KIID( aCtx.Request.id().value() ) );

    if( !item )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "the requested item ID is not present in the given document" );
        return tl::unexpected( e );
    }

    const EDA_IU_SCALE& scale = getIuScale();
    VECTOR2I posIu = UnpackVector2( aCtx.Request.position(), scale );
    int toleranceIu = scale.NmToIU( aCtx.Request.tolerance() );

    if( ( *item )->HitTest( posIu, toleranceIu ) )
        response.set_result( HitTestResult::HTR_HIT );
    else
        response.set_result( HitTestResult::HTR_NO_HIT );

    return response;
}


HANDLER_RESULT<GetDocumentModifiedStateResponse>
API_HANDLER_EDITOR::handleGetDocumentModifiedState( const HANDLER_CONTEXT<GetDocumentModifiedState>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    GetDocumentModifiedStateResponse response;

    wxCHECK( m_frame, response );
    response.set_state( m_frame->IsContentModified() ? DocumentModifiedState::DMS_MODIFIED
                                                     : DocumentModifiedState::DMS_UNMODIFIED );
    return response;
}


std::vector<KICAD_T> API_HANDLER_EDITOR::parseRequestedItemTypes( const google::protobuf::RepeatedField<int>& aTypes )
{
    std::vector<KICAD_T> types;

    for( int typeRaw : aTypes )
    {
        auto typeMessage = static_cast<types::KiCadObjectType>( typeRaw );

        if( KICAD_T type = FromProtoEnum<KICAD_T>( typeMessage ); type != TYPE_NOT_INIT )
            types.emplace_back( type );
    }

    return types;
}


HANDLER_RESULT<types::TitleBlockInfo>
API_HANDLER_EDITOR::handleGetTitleBlockInfo( const HANDLER_CONTEXT<GetTitleBlockInfo>& aCtx )
{
    HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    std::optional<TITLE_BLOCK*> optBlock = getTitleBlock( aCtx.Request.document() );

    if( !optBlock )
    {
        ApiResponseStatus e;
        e.set_status( AS_BAD_REQUEST );
        e.set_error_message( "this editor does not support a title block" );
        return tl::unexpected( e );
    }

    TITLE_BLOCK& block = **optBlock;

    types::TitleBlockInfo response;

    response.set_title( block.GetTitle().ToUTF8() );
    response.set_date( block.GetDate().ToUTF8() );
    response.set_revision( block.GetRevision().ToUTF8() );
    response.set_company( block.GetCompany().ToUTF8() );
    response.set_comment1( block.GetComment( 0 ).ToUTF8() );
    response.set_comment2( block.GetComment( 1 ).ToUTF8() );
    response.set_comment3( block.GetComment( 2 ).ToUTF8() );
    response.set_comment4( block.GetComment( 3 ).ToUTF8() );
    response.set_comment5( block.GetComment( 4 ).ToUTF8() );
    response.set_comment6( block.GetComment( 5 ).ToUTF8() );
    response.set_comment7( block.GetComment( 6 ).ToUTF8() );
    response.set_comment8( block.GetComment( 7 ).ToUTF8() );
    response.set_comment9( block.GetComment( 8 ).ToUTF8() );

    return response;
}


HANDLER_RESULT<google::protobuf::Empty>
API_HANDLER_EDITOR::handleSetTitleBlockInfo( const HANDLER_CONTEXT<SetTitleBlockInfo>& aCtx )
{
    HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( !aCtx.Request.has_title_block() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "SetTitleBlockInfo requires title_block" );
        return tl::unexpected( e );
    }

    std::optional<TITLE_BLOCK*> optBlock = getTitleBlock( aCtx.Request.document() );

    if( !optBlock )
    {
        ApiResponseStatus e;
        e.set_status( AS_BAD_REQUEST );
        e.set_error_message( "this editor does not support a title block" );
        return tl::unexpected( e );
    }

    TITLE_BLOCK& block = **optBlock;

    const types::TitleBlockInfo& request = aCtx.Request.title_block();

    block.SetTitle( wxString::FromUTF8( request.title() ) );
    block.SetDate( wxString::FromUTF8( request.date() ) );
    block.SetRevision( wxString::FromUTF8( request.revision() ) );
    block.SetCompany( wxString::FromUTF8( request.company() ) );
    block.SetComment( 0, wxString::FromUTF8( request.comment1() ) );
    block.SetComment( 1, wxString::FromUTF8( request.comment2() ) );
    block.SetComment( 2, wxString::FromUTF8( request.comment3() ) );
    block.SetComment( 3, wxString::FromUTF8( request.comment4() ) );
    block.SetComment( 4, wxString::FromUTF8( request.comment5() ) );
    block.SetComment( 5, wxString::FromUTF8( request.comment6() ) );
    block.SetComment( 6, wxString::FromUTF8( request.comment7() ) );
    block.SetComment( 7, wxString::FromUTF8( request.comment8() ) );
    block.SetComment( 8, wxString::FromUTF8( request.comment9() ) );

    onModified();

    return google::protobuf::Empty();
}


HANDLER_RESULT<types::PageSettings> API_HANDLER_EDITOR::handleGetPageSettings(
        const HANDLER_CONTEXT<commands::GetPageSettings>& aCtx)
{
    HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    std::optional<PAGE_INFO> optPageInfo = getPageSettings( aCtx.Request.document() );

    if( !optPageInfo )
    {
        ApiResponseStatus e;
        e.set_status( AS_BAD_REQUEST );
        e.set_error_message( "this editor does not support page settings" );
        return tl::unexpected( e );
    }

    PAGE_INFO& pageInfo = *optPageInfo;

    types::PageSettings response;
    response.set_page_size( ToProtoEnum<PAGE_SIZE_TYPE, types::PageSize>( pageInfo.GetType() ) );

    if( pageInfo.IsCustom() )
        PackVector2( *response.mutable_user_page_size(), pageInfo.GetSizeIU( pcbIUScale.IU_PER_MILS ) );

    response.set_orientation( pageInfo.IsPortrait() ? types::PageOrientation::PO_PORTRAIT
                                                    : types::PageOrientation::PO_LANDSCAPE );
    response.set_drawing_sheet( getDrawingSheetFileName().ToUTF8() );

    return response;
}


HANDLER_RESULT<types::PageSettings> API_HANDLER_EDITOR::handleSetPageSettings(
        const HANDLER_CONTEXT<commands::SetPageSettings>& aCtx )
{
    HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() );

    if( !documentValidation )
        return tl::unexpected( documentValidation.error() );

    if( !aCtx.Request.has_page_settings() )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "SetPageSettings requires page_settings" );
        return tl::unexpected( e );
    }

    std::optional<PAGE_INFO> optPageInfo = getPageSettings( aCtx.Request.document() );

    if( !optPageInfo )
    {
        ApiResponseStatus e;
        e.set_status( AS_BAD_REQUEST );
        e.set_error_message( "this editor does not support page settings" );
        return tl::unexpected( e );
    }

    const types::PageSettings& request = aCtx.Request.page_settings();

    if( request.page_size() == types::PageSize::PS_UNKNOWN )
    {
        ApiResponseStatus e;
        e.set_status( ApiStatusCode::AS_BAD_REQUEST );
        e.set_error_message( "SetPageSettings requires page_size" );
        return tl::unexpected( e );
    }

    PAGE_INFO pageInfo = *optPageInfo;
    PAGE_SIZE_TYPE pageSizeType = FromProtoEnum<PAGE_SIZE_TYPE>( request.page_size() );

    if( pageSizeType == PAGE_SIZE_TYPE::User )
    {
        if( !request.has_user_page_size() )
        {
            ApiResponseStatus e;
            e.set_status( ApiStatusCode::AS_BAD_REQUEST );
            e.set_error_message( "custom page size requires user_page_size" );
            return tl::unexpected( e );
        }

        VECTOR2D sizeIu = UnpackVector2( request.user_page_size() );
        PAGE_INFO::SetCustomWidthMils( pcbIUScale.IUToMils( sizeIu.x ) );
        PAGE_INFO::SetCustomHeightMils( pcbIUScale.IUToMils( sizeIu.y ) );

        pageInfo.SetType( PAGE_SIZE_TYPE::User );
    }
    else
    {
        bool portrait = ( request.orientation() == types::PageOrientation::PO_PORTRAIT );
        pageInfo.SetType( pageSizeType, portrait );
    }

    if( !setPageSettings( aCtx.Request.document(), pageInfo ) )
    {
        ApiResponseStatus e;
        e.set_status( AS_BAD_REQUEST );
        e.set_error_message( "this editor does not support page settings" );
        return tl::unexpected( e );
    }

    wxString drawingSheet( wxString::FromUTF8( request.drawing_sheet() ) );
    setDrawingSheetFileName( drawingSheet );

    onModified();

    types::PageSettings response;
    response.set_page_size( ToProtoEnum<PAGE_SIZE_TYPE, types::PageSize>( pageInfo.GetType() ) );

    if( pageInfo.IsCustom() )
        PackVector2( *response.mutable_user_page_size(), pageInfo.GetSizeIU( pcbIUScale.IU_PER_MILS ) );

    response.set_orientation( pageInfo.IsPortrait() ? types::PageOrientation::PO_PORTRAIT
                                                    : types::PageOrientation::PO_LANDSCAPE );
    response.set_drawing_sheet( getDrawingSheetFileName().ToUTF8() );

    return response;
}


static ApiResponseStatus editorError( ApiStatusCode aCode, const std::string& aMessage )
{
    ApiResponseStatus e;
    e.set_status( aCode );
    e.set_error_message( aMessage );
    return e;
}


HANDLER_RESULT<EDA_DRAW_FRAME*> API_HANDLER_EDITOR::drawFrame( const std::string& aCommandName ) const
{
    EDA_DRAW_FRAME* frame = dynamic_cast<EDA_DRAW_FRAME*>( m_frame );

    if( !frame || !frame->GetToolManager() )
    {
        return tl::unexpected( editorError( ApiStatusCode::AS_UNIMPLEMENTED,
                                            fmt::format( "{} needs an open editor window", aCommandName ) ) );
    }

    return frame;
}


HANDLER_RESULT<UndoRedoResponse> API_HANDLER_EDITOR::undoOrRedo( const DocumentSpecifier& aDocument, bool aRedo )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aDocument ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    HANDLER_RESULT<EDA_DRAW_FRAME*> frame = drawFrame( aRedo ? "RedoLastChange" : "UndoLastChange" );

    if( !frame )
        return tl::unexpected( frame.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    // An open commit would be pushed on top of the restored state
    if( !m_activeClients.empty() || !m_commits.empty() )
    {
        return tl::unexpected( editorError( ApiStatusCode::AS_BUSY,
                                            "cannot undo or redo while an API commit is open" ) );
    }

    EDA_DRAW_FRAME* editFrame = *frame;
    auto            count = [&]()
                            {
                                return aRedo ? editFrame->GetRedoCommandCount()
                                             : editFrame->GetUndoCommandCount();
                            };
    const int       before = count();

    if( before > 0 )
        editFrame->GetToolManager()->RunAction( aRedo ? ACTIONS::redo : ACTIONS::undo );

    UndoRedoResponse response;
    response.set_performed( count() < before );
    response.set_undo_count( editFrame->GetUndoCommandCount() );
    response.set_redo_count( editFrame->GetRedoCommandCount() );
    return response;
}


HANDLER_RESULT<UndoRedoResponse> API_HANDLER_EDITOR::handleUndoLastChange(
        const HANDLER_CONTEXT<UndoLastChange>& aCtx )
{
    return undoOrRedo( aCtx.Request.document(), false );
}


HANDLER_RESULT<UndoRedoResponse> API_HANDLER_EDITOR::handleRedoLastChange(
        const HANDLER_CONTEXT<RedoLastChange>& aCtx )
{
    return undoOrRedo( aCtx.Request.document(), true );
}


HANDLER_RESULT<API_HANDLER_EDITOR::PAGE_LAYOUT_INPUT>
API_HANDLER_EDITOR::getPageLayoutInput( const DocumentSpecifier& aDocument )
{
    return tl::unexpected( editorError( ApiStatusCode::AS_BAD_REQUEST, "this editor has no page layout" ) );
}


HANDLER_RESULT<PageLayoutResponse> API_HANDLER_EDITOR::handleGetPageLayout(
        const HANDLER_CONTEXT<GetPageLayout>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    HANDLER_RESULT<PAGE_LAYOUT_INPUT> input = getPageLayoutInput( aCtx.Request.document() );

    if( !input )
        return tl::unexpected( input.error() );

    const EDA_IU_SCALE& scale = getIuScale();
    const VECTOR2I      pageSize = input->page.GetSizeIU( scale.IU_PER_MILS );

    // Drawing area: the innermost rectangle spanning the page from the top-left to the
    // bottom-right corner (the inner border line).  Title block: everything anchored to the
    // bottom-right corner that is smaller than half the page.
    std::optional<BOX2I> drawingArea;
    std::optional<BOX2I> titleBlock;

    if( input->drawing )
    {
        DS_DRAW_ITEM_LIST& drawing = *input->drawing;

        for( DS_DRAW_ITEM_BASE* item = drawing.GetFirst(); item; item = drawing.GetNext() )
        {
            DS_DATA_ITEM* peer = item->GetPeer();

            if( !peer )
                continue;

            if( auto* rect = dynamic_cast<DS_DRAW_ITEM_RECT*>( item );
                rect && peer->m_Pos.m_Anchor == LT_CORNER && peer->m_End.m_Anchor == RB_CORNER )
            {
                BOX2I box( rect->GetStart(), rect->GetEnd() - rect->GetStart() );
                box.Normalize();

                if( !drawingArea || box.GetArea() < drawingArea->GetArea() )
                    drawingArea = box;

                continue;
            }

            if( peer->m_Pos.m_Anchor != RB_CORNER )
                continue;

            if( ( peer->GetType() == DS_DATA_ITEM::DS_SEGMENT || peer->GetType() == DS_DATA_ITEM::DS_RECT )
                && peer->m_End.m_Anchor != RB_CORNER )
            {
                continue;
            }

            BOX2I box = item->GetBoundingBox();
            box.Normalize();

            if( box.GetWidth() >= pageSize.x / 2 || box.GetHeight() >= pageSize.y / 2 )
                continue;

            if( titleBlock )
                titleBlock->Merge( box );
            else
                titleBlock = box;
        }
    }

    if( !drawingArea )
    {
        DS_DATA_MODEL& model = DS_DATA_MODEL::GetTheInstance();
        const int      left = scale.mmToIU( model.GetLeftMargin() );
        const int      top = scale.mmToIU( model.GetTopMargin() );
        const int      right = scale.mmToIU( model.GetRightMargin() );
        const int      bottom = scale.mmToIU( model.GetBottomMargin() );
        drawingArea = BOX2I( VECTOR2I( left, top ),
                             VECTOR2I( pageSize.x - left - right, pageSize.y - top - bottom ) );
    }

    PageLayoutResponse response;
    PackVector2( *response.mutable_page_size(), pageSize, scale );
    PackBox2( *response.mutable_drawing_area(), *drawingArea, scale );

    if( titleBlock )
        PackBox2( *response.mutable_title_block(), *titleBlock, scale );

    if( input->boardOutline )
        PackBox2( *response.mutable_board_outline(), *input->boardOutline, scale );

    std::optional<BOX2I> content;

    for( const PAGE_LAYOUT_ITEM& item : input->items )
    {
        BOX2I box = item.box;
        box.Normalize();

        if( content )
            content->Merge( box );
        else
            content = box;

        const bool outside = !drawingArea->Contains( box );
        const bool overlaps = titleBlock && box.Intersects( *titleBlock );

        if( !outside && !overlaps )
            continue;

        PageLayoutItem* problem = response.add_problems();
        problem->mutable_id()->set_value( item.id.AsStdString() );
        problem->set_type( item.type );
        problem->set_name( item.name.ToUTF8() );
        PackBox2( *problem->mutable_box(), box, scale );
        problem->set_outside_drawing_area( outside );
        problem->set_overlaps_title_block( overlaps );
    }

    if( content )
        PackBox2( *response.mutable_content(), *content, scale );

    response.set_item_count( static_cast<int>( input->items.size() ) );

    // Ignore contact closer than this: text that only touches, or a wire grazing a corner
    if( input->checkOverlaps )
        addOverlaps( response, *input, scale.mmToIU( 0.1 ) );

    return response;
}


BOX2I API_HANDLER_EDITOR::textCore( const BOX2I& aBox, int aTextHeight, const EDA_ANGLE& aDrawAngle )
{
    BOX2I     core = aBox;
    EDA_ANGLE angle = aDrawAngle;

    core.Normalize();
    angle.Normalize();

    const bool horizontal = angle.IsHorizontal();

    if( aTextHeight <= 0 || !( horizontal || angle.IsVertical() ) )
        return core;

    // Text boxes include the line spacing above and below the glyphs: without it, text stacked at
    // its usual pitch (fields, labels on adjacent wires) or sitting on a wire would overlap
    const int across = horizontal ? core.GetHeight() : core.GetWidth();
    const int pad = std::clamp( ( across - aTextHeight ) / 2, 0, aTextHeight / 2 );

    if( horizontal )
        core.Inflate( 0, -pad );
    else
        core.Inflate( -pad, 0 );

    return core;
}


void API_HANDLER_EDITOR::addOverlaps( PageLayoutResponse& aResponse, const PAGE_LAYOUT_INPUT& aInput,
                                      int aMargin ) const
{
    static constexpr int MAX_REPORTED_OVERLAPS = 200;

    const EDA_IU_SCALE& scale = getIuScale();

    struct FOUND
    {
        size_t first;
        size_t second;
        BOX2I  overlap;
    };

    const std::vector<PAGE_LAYOUT_SHAPE>& shapes = aInput.shapes;
    std::vector<size_t>                   order( shapes.size() );
    std::vector<FOUND>                    found;

    for( size_t ii = 0; ii < order.size(); ++ii )
        order[ii] = ii;

    std::sort( order.begin(), order.end(),
               [&]( size_t a, size_t b )
               {
                   return shapes[a].box.GetLeft() < shapes[b].box.GetLeft();
               } );

    auto boxOverlap = [&]( const BOX2I& a, const BOX2I& b ) -> std::optional<BOX2I>
    {
        const int left = std::max( a.GetLeft(), b.GetLeft() );
        const int right = std::min( a.GetRight(), b.GetRight() );
        const int top = std::max( a.GetTop(), b.GetTop() );
        const int bottom = std::min( a.GetBottom(), b.GetBottom() );

        if( right - left <= aMargin || bottom - top <= aMargin )
            return std::nullopt;

        return BOX2I::ByCorners( VECTOR2I( left, top ), VECTOR2I( right, bottom ) );
    };

    auto coreOf = []( const PAGE_LAYOUT_SHAPE& aShape ) -> const BOX2I&
    {
        return aShape.core ? *aShape.core : aShape.box;
    };

    auto wireOverlap = [&]( const PAGE_LAYOUT_SHAPE& aWire,
                            const PAGE_LAYOUT_SHAPE& aTextShape ) -> std::optional<BOX2I>
    {
        const BOX2I& text = coreOf( aTextShape );

        // A label on its wire, or a wire or bus ending at a label or sheet pin, is its connection
        if( aTextShape.anchor && SEG( aWire.start, aWire.end ).Contains( *aTextShape.anchor ) )
            return std::nullopt;

        // The wire must cross the text, not graze its edge
        if( text.GetWidth() <= 2 * aMargin || text.GetHeight() <= 2 * aMargin )
            return std::nullopt;

        BOX2I inner = text;
        inner.Inflate( -aMargin, -aMargin );

        if( !inner.Intersects( aWire.start, aWire.end ) )
            return std::nullopt;

        const BOX2I segment = BOX2I::ByCorners( aWire.start, aWire.end );
        const int   left = std::max( segment.GetLeft(), text.GetLeft() );
        const int   right = std::min( segment.GetRight(), text.GetRight() );
        const int   top = std::max( segment.GetTop(), text.GetTop() );
        const int   bottom = std::min( segment.GetBottom(), text.GetBottom() );

        return BOX2I::ByCorners( VECTOR2I( left, top ),
                                 VECTOR2I( std::max( left, right ), std::max( top, bottom ) ) );
    };

    // Sweep along x: only shapes starting before this one ends can overlap it
    for( size_t ii = 0; ii < order.size(); ++ii )
    {
        const PAGE_LAYOUT_SHAPE& a = shapes[order[ii]];

        for( size_t jj = ii + 1; jj < order.size(); ++jj )
        {
            const PAGE_LAYOUT_SHAPE& b = shapes[order[jj]];

            if( b.box.GetLeft() > a.box.GetRight() )
                break;

            if( a.layer != b.layer )
                continue;

            if( a.group == b.group && ( a.ownerBody || b.ownerBody ) )
                continue;

            const bool           aWire = a.kind == OVERLAP_KIND::WIRE;
            const bool           bWire = b.kind == OVERLAP_KIND::WIRE;
            std::optional<BOX2I> overlap;

            if( aWire && bWire )
                continue;

            if( aWire || bWire )
            {
                const PAGE_LAYOUT_SHAPE& wire = aWire ? a : b;
                const PAGE_LAYOUT_SHAPE& other = aWire ? b : a;

                if( other.kind == OVERLAP_KIND::TEXT )
                    overlap = wireOverlap( wire, other );
            }
            else
            {
                overlap = boxOverlap( coreOf( a ), coreOf( b ) );
            }

            if( overlap )
                found.push_back( { order[ii], order[jj], *overlap } );
        }
    }

    // Report top to bottom, left to right, so a reply reads like the page
    std::sort( found.begin(), found.end(),
               []( const FOUND& a, const FOUND& b )
               {
                   if( a.overlap.GetTop() != b.overlap.GetTop() )
                       return a.overlap.GetTop() < b.overlap.GetTop();

                   return a.overlap.GetLeft() < b.overlap.GetLeft();
               } );

    auto packShape = [&]( PageLayoutItem* aOut, const PAGE_LAYOUT_SHAPE& aShape )
    {
        aOut->mutable_id()->set_value( aShape.id.AsStdString() );
        aOut->set_type( aShape.type );
        aOut->set_name( aShape.name.ToUTF8() );
        aOut->set_part( aShape.part.ToUTF8() );

        PackBox2( *aOut->mutable_box(), aShape.box, scale );
    };

    for( const FOUND& entry : found )
    {
        if( aResponse.overlaps_size() >= MAX_REPORTED_OVERLAPS )
            break;

        PageLayoutOverlap* overlap = aResponse.add_overlaps();
        packShape( overlap->mutable_first(), shapes[entry.first] );
        packShape( overlap->mutable_second(), shapes[entry.second] );
        PackBox2( *overlap->mutable_overlap(), entry.overlap, scale );

        if( !shapes[entry.first].layerName.IsEmpty() )
            overlap->set_layer( shapes[entry.first].layerName.ToUTF8() );
    }

    aResponse.set_overlap_count( static_cast<int>( found.size() ) );
    aResponse.set_overlaps_checked( true );
}


static GridStyle packGridStyle( KIGFX::GRID_STYLE aStyle )
{
    switch( aStyle )
    {
    case KIGFX::GRID_STYLE::DOTS:        return GridStyle::GST_DOTS;
    case KIGFX::GRID_STYLE::LINES:       return GridStyle::GST_LINES;
    case KIGFX::GRID_STYLE::SMALL_CROSS: return GridStyle::GST_SMALL_CROSS;
    }

    return GridStyle::GST_UNKNOWN;
}


static GridSnapping packGridSnapping( KIGFX::GRID_SNAPPING aSnapping )
{
    switch( aSnapping )
    {
    case KIGFX::GRID_SNAPPING::ALWAYS:    return GridSnapping::GSN_ALWAYS;
    case KIGFX::GRID_SNAPPING::WITH_GRID: return GridSnapping::GSN_WITH_GRID;
    case KIGFX::GRID_SNAPPING::NEVER:     return GridSnapping::GSN_NEVER;
    }

    return GridSnapping::GSN_UNKNOWN;
}


GridSettings API_HANDLER_EDITOR::packGridSettings( EDA_DRAW_FRAME* aFrame ) const
{
    const EDA_IU_SCALE&   scale = getIuScale();
    GRID_SETTINGS&        grid = aFrame->GetWindowSettings( aFrame->config() )->grid;
    COMMON_TOOLS*         tools = aFrame->GetToolManager()->GetTool<COMMON_TOOLS>();
    std::vector<VECTOR2I> sizes = tools ? tools->Grids() : std::vector<VECTOR2I>();

    GridSettings response;

    for( size_t ii = 0; ii < grid.grids.size(); ++ii )
    {
        GridPreset* preset = response.add_presets();
        preset->set_name( grid.grids[ii].name.ToUTF8() );

        VECTOR2D fromText = grid.grids[ii].ToDouble( scale );
        VECTOR2I size = ii < sizes.size() ? sizes[ii] : VECTOR2I( KiROUND( fromText.x ), KiROUND( fromText.y ) );
        PackVector2( *preset->mutable_size(), size, scale );
    }

    const int index = std::clamp( grid.last_size_idx, 0, std::max( 0, response.presets_size() - 1 ) );
    response.set_preset_index( index );

    if( index < response.presets_size() )
        *response.mutable_size() = response.presets( index ).size();

    response.set_visible( grid.show );
    response.set_style( packGridStyle( grid.style ) );
    response.set_snapping( packGridSnapping( grid.snap ) );
    PackVector2( *response.mutable_origin(), aFrame->GetGridOrigin(), scale );
    return response;
}


HANDLER_RESULT<GridSettings> API_HANDLER_EDITOR::handleGetGridSettings(
        const HANDLER_CONTEXT<GetGridSettings>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    HANDLER_RESULT<EDA_DRAW_FRAME*> frame = drawFrame( "GetGridSettings" );

    if( !frame )
        return tl::unexpected( frame.error() );

    return packGridSettings( *frame );
}


HANDLER_RESULT<GridSettings> API_HANDLER_EDITOR::handleSetGridSettings(
        const HANDLER_CONTEXT<SetGridSettings>& aCtx )
{
    if( HANDLER_RESULT<bool> documentValidation = validateDocument( aCtx.Request.document() ); !documentValidation )
        return tl::unexpected( documentValidation.error() );

    HANDLER_RESULT<EDA_DRAW_FRAME*> frameResult = drawFrame( "SetGridSettings" );

    if( !frameResult )
        return tl::unexpected( frameResult.error() );

    if( std::optional<ApiResponseStatus> busy = checkForBusy() )
        return tl::unexpected( *busy );

    EDA_DRAW_FRAME*     frame = *frameResult;
    const EDA_IU_SCALE& scale = getIuScale();
    WINDOW_SETTINGS*    window = frame->GetWindowSettings( frame->config() );
    GRID_SETTINGS&      grid = window->grid;
    COMMON_TOOLS*       tools = frame->GetToolManager()->GetTool<COMMON_TOOLS>();
    const auto&         request = aCtx.Request;

    if( !tools )
        return tl::unexpected( editorError( ApiStatusCode::AS_UNIMPLEMENTED, "this editor has no grid tools" ) );

    std::optional<int> index;

    if( request.has_size() )
    {
        const VECTOR2I size = UnpackVector2( request.size(), scale );

        if( size.x <= 0 || size.y <= 0 )
            return tl::unexpected( editorError( ApiStatusCode::AS_BAD_REQUEST, "the grid size must be positive" ) );

        std::vector<VECTOR2I> sizes = tools->Grids();

        for( size_t ii = 0; ii < sizes.size(); ++ii )
        {
            if( std::abs( sizes[ii].x - size.x ) <= 1 && std::abs( sizes[ii].y - size.y ) <= 1 )
            {
                index = static_cast<int>( ii );
                break;
            }
        }

        if( !index )
        {
            auto text = [&]( int aValue )
                        {
                            return EDA_UNIT_UTILS::UI::StringFromValue( scale, EDA_UNITS::MM, aValue, true );
                        };

            grid.grids.push_back( GRID{ wxEmptyString, text( size.x ), text( size.y ) } );
            index = static_cast<int>( grid.grids.size() ) - 1;

            // Rebuilds the tool's grid list from the settings
            tools->Reset( TOOL_BASE::MODEL_RELOAD );
        }
    }
    else if( request.has_preset_index() )
    {
        if( request.preset_index() < 0 || request.preset_index() >= static_cast<int>( grid.grids.size() ) )
        {
            return tl::unexpected( editorError( ApiStatusCode::AS_BAD_REQUEST,
                                                fmt::format( "preset_index must be 0 to {}",
                                                             grid.grids.size() - 1 ) ) );
        }

        index = request.preset_index();
    }

    if( request.has_style() )
    {
        switch( request.style() )
        {
        case GridStyle::GST_DOTS:        grid.style = KIGFX::GRID_STYLE::DOTS;        break;
        case GridStyle::GST_LINES:       grid.style = KIGFX::GRID_STYLE::LINES;       break;
        case GridStyle::GST_SMALL_CROSS: grid.style = KIGFX::GRID_STYLE::SMALL_CROSS; break;
        default: return tl::unexpected( editorError( ApiStatusCode::AS_BAD_REQUEST, "unknown grid style" ) );
        }
    }

    if( request.has_snapping() )
    {
        switch( request.snapping() )
        {
        case GridSnapping::GSN_ALWAYS:    grid.snap = KIGFX::GRID_SNAPPING::ALWAYS;    break;
        case GridSnapping::GSN_WITH_GRID: grid.snap = KIGFX::GRID_SNAPPING::WITH_GRID; break;
        case GridSnapping::GSN_NEVER:     grid.snap = KIGFX::GRID_SNAPPING::NEVER;     break;
        default: return tl::unexpected( editorError( ApiStatusCode::AS_BAD_REQUEST, "unknown grid snapping" ) );
        }
    }

    if( request.has_style() || request.has_snapping() )
        frame->GetGalDisplayOptions().ReadWindowSettings( *window );

    if( index )
        tools->GridPreset( *index, false );

    if( request.has_visible() )
        frame->SetGridVisibility( request.visible() );

    frame->UpdateGridSelectBox();

    if( frame->GetCanvas() )
        frame->GetCanvas()->Refresh();

    return packGridSettings( frame );
}
