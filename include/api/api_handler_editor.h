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

#ifndef KICAD_API_HANDLER_EDITOR_H
#define KICAD_API_HANDLER_EDITOR_H

#include <api/api_handler.h>
#include <api/common/commands/editor_commands.pb.h>
#include <base_units.h>
#include <commit.h>
#include <geometry/eda_angle.h>
#include <google/protobuf/empty.pb.h>
#include <kiid.h>
#include <math/box2.h>
#include <deque>
#include <memory>
#include <optional>
#include <vector>
#include <page_info.h>

using namespace kiapi::common;
using kiapi::common::types::DocumentSpecifier;
using kiapi::common::types::ItemRequestStatus;
using kiapi::common::commands::ItemDeletionStatus;

class DS_DRAW_ITEM_LIST;
class EDA_BASE_FRAME;
class EDA_DRAW_FRAME;
class TITLE_BLOCK;
class wxTimer;

/**
 * Base class for API handlers related to editor frames
 *
 * Revisions and operations (Embeint, see tools/live_schematic_mcp/REVISIONS.md): every reply to a
 * request naming this editor's document carries the document revision.  A request with an
 * expected revision is refused with AS_REVISION_MISMATCH before its handler runs if the revision
 * differs.  Mutations may carry an operation id whose outcome API_OPERATION_LOG keeps.  Open
 * commits hold a lease: one whose client sends nothing for the lease is rolled back.
 */
class API_HANDLER_EDITOR : public API_HANDLER
{
public:
    API_HANDLER_EDITOR( EDA_BASE_FRAME* aFrame = nullptr );

    ~API_HANDLER_EDITOR() override;

    /// Seconds an open commit is kept without a request from its client, unless BeginCommit asks
    static constexpr int DEFAULT_COMMIT_LEASE_S = 60;
    static constexpr int MAX_COMMIT_LEASE_S = 3600;

    /// Clients whose commit was rolled back on expiry are remembered (to refuse their later steps)
    static constexpr size_t MAX_EXPIRED_CLIENTS = 64;

protected:
    std::optional<ApiResponseStatus> beforeRequest( const kiapi::common::ApiRequestHeader& aHeader,
                                                    const google::protobuf::Message& aRequest ) override;

    void afterRequest( const kiapi::common::ApiRequestHeader& aHeader, const google::protobuf::Message& aRequest,
                       const ApiResponseStatus& aStatus ) override;

    HANDLER_RESULT<commands::DocumentRevisionResponse> handleGetDocumentRevision(
            const HANDLER_CONTEXT<commands::GetDocumentRevision>& aCtx );

    /// The document revision token, or empty for a handler without a frame
    std::string documentRevision() const;

    /// Roll back every commit (except aExceptClient's) whose lease has run out
    void expireCommits( const std::string& aExceptClient = {} );

    /**
     * Revert a client's open commit and resolve its operations as rolled back.
     * @param aExpired remembers the client so its later commit steps are refused
     */
    void rollBackCommit( const std::string& aClientName, const std::string& aReason, bool aExpired );

    /// Resolve the operations of a client's commit that has ended, and forget its state
    void finishCommit( const std::string& aClientName, commands::OperationState aState,
                       const std::string& aMessage );

    /// If the header is valid, returns the item container
    HANDLER_RESULT<std::optional<KIID>> validateItemHeaderDocument(
            const kiapi::common::types::ItemHeader& aHeader );

    HANDLER_RESULT<bool> validateDocument( const DocumentSpecifier& aDocument );

    /**
     * Checks if the editor can accept commands
     * @return an error status if busy, std::nullopt if not busy
     */
    virtual std::optional<ApiResponseStatus> checkForBusy();

    HANDLER_RESULT<commands::BeginCommitResponse> handleBeginCommit(
        const HANDLER_CONTEXT<commands::BeginCommit>& aCtx );

    HANDLER_RESULT<commands::EndCommitResponse> handleEndCommit(
        const HANDLER_CONTEXT<commands::EndCommit>& aCtx );

    COMMIT* getCurrentCommit( const std::string& aClientName );

    virtual void pushCurrentCommit( const std::string& aClientName, const wxString& aMessage );

    HANDLER_RESULT<commands::CreateItemsResponse> handleCreateItems(
        const HANDLER_CONTEXT<commands::CreateItems>& aCtx );

    HANDLER_RESULT<commands::UpdateItemsResponse> handleUpdateItems(
        const HANDLER_CONTEXT<commands::UpdateItems>& aCtx );

    HANDLER_RESULT<commands::DeleteItemsResponse> handleDeleteItems(
        const HANDLER_CONTEXT<commands::DeleteItems>& aCtx );

    HANDLER_RESULT<commands::HitTestResponse> handleHitTest(
        const HANDLER_CONTEXT<commands::HitTest>& aCtx );

    virtual HANDLER_RESULT<commands::GetDocumentModifiedStateResponse>
    handleGetDocumentModifiedState( const HANDLER_CONTEXT<commands::GetDocumentModifiedState>& aCtx );

    HANDLER_RESULT<types::TitleBlockInfo> handleGetTitleBlockInfo(
            const HANDLER_CONTEXT<commands::GetTitleBlockInfo>& aCtx );

    HANDLER_RESULT<google::protobuf::Empty> handleSetTitleBlockInfo(
            const HANDLER_CONTEXT<commands::SetTitleBlockInfo>& aCtx );

    HANDLER_RESULT<types::PageSettings> handleGetPageSettings(
            const HANDLER_CONTEXT<commands::GetPageSettings>& aCtx );

    HANDLER_RESULT<types::PageSettings> handleSetPageSettings(
            const HANDLER_CONTEXT<commands::SetPageSettings>& aCtx );

    HANDLER_RESULT<commands::UndoRedoResponse> handleUndoLastChange(
            const HANDLER_CONTEXT<commands::UndoLastChange>& aCtx );

    HANDLER_RESULT<commands::UndoRedoResponse> handleRedoLastChange(
            const HANDLER_CONTEXT<commands::RedoLastChange>& aCtx );

    HANDLER_RESULT<commands::UndoRedoResponse> undoOrRedo( const DocumentSpecifier& aDocument, bool aRedo );

    HANDLER_RESULT<commands::PageLayoutResponse> handleGetPageLayout(
            const HANDLER_CONTEXT<commands::GetPageLayout>& aCtx );

    HANDLER_RESULT<commands::GridSettings> handleGetGridSettings(
            const HANDLER_CONTEXT<commands::GetGridSettings>& aCtx );

    HANDLER_RESULT<commands::GridSettings> handleSetGridSettings(
            const HANDLER_CONTEXT<commands::SetGridSettings>& aCtx );

    /// The frame as a drawing frame, or an error for headless handlers and non-drawing frames
    HANDLER_RESULT<EDA_DRAW_FRAME*> drawFrame( const std::string& aCommandName ) const;

    commands::GridSettings packGridSettings( EDA_DRAW_FRAME* aFrame ) const;

    /// One item of a document, for the page layout check
    struct PAGE_LAYOUT_ITEM
    {
        KIID        id;
        std::string type;
        wxString    name;
        BOX2I       box;
    };

    /// How a shape takes part in the overlap check
    enum class OVERLAP_KIND
    {
        BODY, ///< A symbol, sheet or table outline: compared with bodies and text
        TEXT, ///< Text, a field, a label or a sheet pin: compared with everything
        WIRE  ///< A wire or bus segment: compared with text only
    };

    /// One drawn part of an item for the overlap check
    struct PAGE_LAYOUT_SHAPE
    {
        KIID         id;              ///< The item to report (the owner of a field)
        KIID         group;           ///< Shapes of one item share a group
        std::string  type;
        wxString     name;
        wxString     part;            ///< Field name or footprint text; empty for the item itself
        BOX2I        box;             ///< The drawn extents, as reported

        /// The area compared, inside box: for text, the band the glyphs occupy (see textCore);
        /// box when unset
        std::optional<BOX2I> core;

        OVERLAP_KIND kind = OVERLAP_KIND::TEXT;
        bool         ownerBody = false; ///< Not compared with other shapes of its own group
        int          layer = 0;       ///< Only shapes on the same layer are compared
        wxString     layerName;
        VECTOR2I     start;           ///< WIRE only: the segment
        VECTOR2I     end;

        /// Labels and sheet pins: the connection point.  A wire or bus through it is the item's
        /// own connection, not text drawn over a wire.
        std::optional<VECTOR2I> anchor;
    };

    /**
     * The band a line of text occupies inside its drawn extents: aBox less the line spacing and
     * pen above and below the glyphs (at most half the glyph height on each side, so multi-line
     * text keeps its lines).  Text at other than 0, 90, 180 or 270 degrees keeps aBox.
     *
     * @param aBox the text's bounding box as the editor draws it
     * @param aTextHeight the glyph height (text size)
     * @param aDrawAngle the angle the text is drawn at, after any parent transform
     */
    static BOX2I textCore( const BOX2I& aBox, int aTextHeight, const EDA_ANGLE& aDrawAngle );

    /// What an editor provides for GetPageLayout: its page, its drawing sheet laid out the way
    /// the editor draws it, and the bounding box of every item on the page
    struct PAGE_LAYOUT_INPUT
    {
        PAGE_INFO                          page;
        std::shared_ptr<DS_DRAW_ITEM_LIST> drawing;
        std::vector<PAGE_LAYOUT_ITEM>      items;
        std::optional<BOX2I>               boardOutline;

        /// Shapes for the overlap check; only reported when checkOverlaps is set
        std::vector<PAGE_LAYOUT_SHAPE>     shapes;
        bool                               checkOverlaps = false;
    };

    /// Pairs of shapes whose compared areas overlap by more than aMargin in both directions
    void addOverlaps( commands::PageLayoutResponse& aResponse, const PAGE_LAYOUT_INPUT& aInput,
                      int aMargin ) const;

    /**
     * Override in editors that draw a drawing sheet (schematic, board).  The default refuses.
     */
    virtual HANDLER_RESULT<PAGE_LAYOUT_INPUT> getPageLayoutInput( const DocumentSpecifier& aDocument );

    /**
     * Override this to create an appropriate COMMIT subclass for the frame in question
     * @return a new COMMIT, bound to the editor frame
     */
    virtual std::unique_ptr<COMMIT> createCommit() = 0;

    /**
     * Override this to specify which document type this editor handles
     */
    virtual types::DocumentType thisDocumentType() const = 0;

    /**
     * @return true if the given document is valid for this editor and is currently open
     */
    virtual tl::expected<bool, ApiResponseStatus> validateDocumentInternal( const DocumentSpecifier& aDocument ) const = 0;

    /**
     * Returns the internal-unit scale that the concrete editor uses. API wire coordinates
     * are always in nanometers, so this scale drives conversion to the editor's native IU.
     * Defaults to pcbIUScale; schematic-like editors must override.
     */
    virtual const EDA_IU_SCALE& getIuScale() const { return pcbIUScale; }

    virtual HANDLER_RESULT<ItemRequestStatus> handleCreateUpdateItemsInternal( bool aCreate,
        const std::string& aClientName,
        const types::ItemHeader &aHeader,
        const google::protobuf::RepeatedPtrField<google::protobuf::Any>& aItems,
        std::function<void( commands::ItemStatus, google::protobuf::Any )> aItemHandler ) = 0;

    virtual void deleteItemsInternal( std::map<KIID, ItemDeletionStatus>& aItemsToDelete,
                                      const std::string& aClientName ) = 0;

    virtual std::optional<EDA_ITEM*> getItemFromDocument( const DocumentSpecifier& aDocument,
                                                          const KIID& aId ) = 0;

    static std::vector<KICAD_T> parseRequestedItemTypes( const google::protobuf::RepeatedField<int>& aTypes );

    virtual std::optional<TITLE_BLOCK*> getTitleBlock( const DocumentSpecifier& aDocument ) { return std::nullopt; }

    virtual std::optional<PAGE_INFO> getPageSettings( const DocumentSpecifier& aDocument ) { return std::nullopt; }

    virtual bool setPageSettings( const DocumentSpecifier& aDocument, const PAGE_INFO& aPageInfo ) { return false; }

    virtual wxString getDrawingSheetFileName() { return wxEmptyString; }

    virtual void setDrawingSheetFileName( const wxString& aFileName ) {}

    virtual void onModified() {}

protected:
    std::map<std::string, std::pair<KIID, std::unique_ptr<COMMIT>>> m_commits;

    std::set<std::string> m_activeClients;

    EDA_BASE_FRAME* m_frame;

private:
    /// Revision and lease bookkeeping for one client's open commit (explicit or implicit)
    struct COMMIT_STATE
    {
        std::vector<std::string> operations;           ///< operation ids bound to the commit
        std::string              revisionAtBegin;
        uint64_t                 lastSeenRevision = 0; ///< after the client's last request
        bool                     strict = false;       ///< opened with an expected revision
        bool                     foreignChange = false;
        std::string              foreignChangeFrom;    ///< revision when the conflict was seen
        long long                lastActivityMs = 0;
        int                      leaseSeconds = DEFAULT_COMMIT_LEASE_S;
    };

    /// What beforeRequest learned about the request being handled
    struct REQUEST_STATE
    {
        bool                                    namesThisDocument = false;
        bool                                    refusedBeforeRun = false; ///< by the revision contract
        bool                                    doNotRecord = false;      ///< duplicate or invalid id
        std::string                             revisionBefore;
        kiapi::common::types::DocumentSpecifier document;
        std::optional<commands::OperationState> endCommitOutcome;
        std::vector<std::string>                resolvedOperations; ///< by a commit ending
    };

    void ensureLeaseTimer();

    std::string revisionToken( uint64_t aCounter ) const;

    /// Distinguishes this editor window's revisions from any other's
    std::string m_revisionEpoch;

    std::map<std::string, COMMIT_STATE> m_commitStates;

    /// Client name -> why its commit was rolled back; oldest first in m_expiredOrder
    std::map<std::string, std::string> m_expiredClients;
    std::deque<std::string>            m_expiredOrder;

    REQUEST_STATE m_request;

    std::unique_ptr<wxTimer> m_leaseTimer;
};

#endif //KICAD_API_HANDLER_EDITOR_H
