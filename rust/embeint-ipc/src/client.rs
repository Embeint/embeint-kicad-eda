// SPDX-License-Identifier: GPL-3.0-or-later
//! The live editor client: a port of `LiveSchematic.send`, the revision contract and the
//! transaction code of `tools/live_schematic_mcp/client.py` (REVISIONS.md documents the contract).
//!
//! - Endpoint selection is lazy and pinned; a pinned endpoint that stops listening is reported,
//!   never replaced.  The instance token is pinned after the first reply.
//! - Mutations are never resent.  A lost reply is reconciled with `GetOperationStatus` once.
//! - Every transaction carries an operation id and asks for a commit lease; a failed step drops
//!   the commit (rollback).  An `EndCommit` refused for a revision conflict is recorded as rolled
//!   back (KiCad rolled it back); other refusals are reported as they are, as in Python.
//!
//! Error texts are the Python client's, so MCP tool errors stay the same.

use std::time::Duration;

use prost::Message as _;
use prost_reflect::{DynamicMessage, ReflectMessage, Value};
use serde_json::{json, Map, Value as Json};

use crate::discovery::{self, DiscoveryError, Platform};
use crate::nng::{self, TransportError};
use crate::pbjson;
use crate::proto::{self, build, enum_name, int_field, message_field, str_field, with_field};

const C: &str = "kiapi.common.commands.";
const T: &str = "kiapi.common.types.";
const S: &str = "kiapi.schematic.commands.";

/// The commit lease (seconds) every BeginCommit asks for, as `DEFAULT_LEASE_S`.
pub const DEFAULT_LEASE_S: u32 = 60;
/// Reply timeout, as `LiveSchematic(timeout_ms=15000)`.
pub const DEFAULT_TIMEOUT_MS: u64 = 15000;
/// The most items one create/update/delete call accepts.
pub const MAX_ITEMS: usize = 200;

/// The Python exception a failure corresponds to; `Display` is its `str()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `DiscoveryError` (a RuntimeError): no request was sent.
    Discovery(DiscoveryError),
    /// `ReplyTimeout`: no reply.  KiCad may still apply the request.
    ReplyTimeout(String),
    /// `RevisionMismatch`: refused under the revision contract; nothing changed.
    RevisionMismatch(String),
    /// `DuplicateOperation`: the operation id was already used; the request did not run.
    DuplicateOperation(String),
    /// `RuntimeError`.
    Runtime(String),
    /// `OutcomeUnknown`: a mutation's outcome could not be confirmed.  Do not resend it.
    OutcomeUnknown {
        message: String,
        operation_id: String,
    },
    /// `ValueError`: invalid arguments, refused before anything was sent.
    Value(String),
    /// `json_format.ParseError`: invalid protobuf JSON in the arguments.
    Parse(String),
    /// `DecodeError`: a reply that is not a protobuf envelope.
    Decode(String),
}

impl Error {
    /// `isinstance(error, RuntimeError)` for the Python class this error stands for.
    pub fn is_runtime_error(&self) -> bool {
        matches!(
            self,
            Error::Discovery(_)
                | Error::RevisionMismatch(_)
                | Error::DuplicateOperation(_)
                | Error::Runtime(_)
                | Error::OutcomeUnknown { .. }
        )
    }

    /// The Python exception class name.
    pub fn class(&self) -> &'static str {
        match self {
            Error::Discovery(_) => "DiscoveryError",
            Error::ReplyTimeout(_) => "ReplyTimeout",
            Error::RevisionMismatch(_) => "RevisionMismatch",
            Error::DuplicateOperation(_) => "DuplicateOperation",
            Error::Runtime(_) => "RuntimeError",
            Error::OutcomeUnknown { .. } => "OutcomeUnknown",
            Error::Value(_) => "ValueError",
            Error::Parse(_) => "ParseError",
            Error::Decode(_) => "DecodeError",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Discovery(e) => f.write_str(&e.message),
            Error::ReplyTimeout(m)
            | Error::RevisionMismatch(m)
            | Error::DuplicateOperation(m)
            | Error::Runtime(m)
            | Error::Value(m)
            | Error::Parse(m)
            | Error::Decode(m) => f.write_str(m),
            Error::OutcomeUnknown { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<DiscoveryError> for Error {
    fn from(error: DiscoveryError) -> Self {
        Error::Discovery(error)
    }
}

impl From<pbjson::ParseError> for Error {
    fn from(error: pbjson::ParseError) -> Self {
        Error::Parse(error.0)
    }
}

/// A reply message as protobuf JSON.  Python raises (TypeError, DecodeError, RecursionError) for
/// an `Any` it cannot print; that becomes the tool's error, and so does this.
pub fn reply_json(message: &DynamicMessage) -> Result<Json, Error> {
    pbjson::to_json(message).map_err(|error| Error::Decode(error.0))
}

/// `new_operation_id()`.
pub fn new_operation_id() -> String {
    format!("mcp-{}", uuid::Uuid::new_v4())
}

/// Expected revision and operation id for the mutations of one tool call (`mutation_options`).
#[derive(Debug, Default)]
struct MutationOptions {
    expected_revision: Option<String>,
    operation_id: Option<String>,
    operations: Vec<Json>,
}

/// One connection to a running KiCad (`LiveSchematic`).  Callers serialise access (the MCP server
/// holds it behind a mutex, as the Python client holds its lock for a whole transaction).
pub struct Client {
    socket_url: String,
    socket_selected: bool,
    token: String,
    client_name: String,
    timeout: Duration,
    received_reply: bool,
    /// (expected_revision, operation_id) for the next send only; set by `send_with`
    pending_header: Option<(String, String)>,
    /// The revision KiCad reported in the last reply, or None
    pub last_revision: Option<String>,
    options: Option<MutationOptions>,
    /// Lease every BeginCommit asks for; 0 asks for none
    pub lease_seconds: u32,
    /// {"operation_id", "state", "revision"} of the last mutation this client finished
    pub last_operation: Option<Json>,
}

impl Client {
    /// `LiveSchematic(socket_url, timeout_ms)`: an explicit endpoint (argument or
    /// KICAD_API_SOCKET) must be valid now; otherwise the instance is discovered lazily on Linux
    /// and Windows, and other hosts keep the default endpoint.
    pub fn new(socket_url: Option<String>, timeout_ms: u64) -> Result<Self, Error> {
        let socket_url = socket_url.filter(|s| !s.is_empty()).or_else(|| {
            std::env::var("KICAD_API_SOCKET")
                .ok()
                .filter(|s| !s.is_empty())
        });
        let explicit = socket_url.is_some();
        let discovers = cfg!(any(target_os = "linux", windows));
        let socket_url = socket_url.unwrap_or_else(discovery::default_socket_url);
        if explicit {
            discovery::local_socket_path(&socket_url, Platform::host())?;
        }
        Ok(Client {
            socket_url,
            socket_selected: explicit || !discovers,
            token: if explicit {
                std::env::var("KICAD_API_TOKEN").unwrap_or_default()
            } else {
                String::new()
            },
            client_name: format!("kicad.embeint-mcp-rs-{}", uuid::Uuid::new_v4()),
            timeout: Duration::from_millis(timeout_ms),
            received_reply: false,
            pending_header: None,
            last_revision: None,
            options: None,
            lease_seconds: DEFAULT_LEASE_S,
            last_operation: None,
        })
    }

    pub fn timeout_ms(&self) -> u128 {
        self.timeout.as_millis()
    }

    /// Sends one command and returns the reply message of type `response_type`.  Never retries:
    /// a timeout may follow a successfully applied edit.
    pub async fn send(
        &mut self,
        command: &DynamicMessage,
        response_type: &str,
    ) -> Result<DynamicMessage, Error> {
        if !self.socket_selected {
            // With no endpoint at all this fails at once (no_editor) and the next request looks
            // again, so a KiCad started later is still found.  Pin only an instance that was
            // found, never the default endpoint a KiCad would take.
            let (url, selection) = discovery::select_instance(None, None, true, true).await?;
            self.socket_url = url;
            self.socket_selected = selection != "default_missing";
        }
        let path = discovery::local_socket_path(&self.socket_url, Platform::host())?;
        if self.received_reply
            && discovery::socket_reachability(&self.socket_url).await? == "not_listening"
        {
            return Err(DiscoveryError::new(
                "instance_unavailable",
                "Selected KiCad instance is unavailable; \
                 no command was sent. Run diagnose.py --probe and restart MCP \
                 with the intended KICAD_API_SOCKET. Instance identity remains pinned.",
            )
            .into());
        }
        let mut header = json!({"kicad_token": self.token, "client_name": self.client_name});
        if let Some((expected_revision, operation_id)) = &self.pending_header {
            header["expected_revision"] = json!(expected_revision);
            header["operation_id"] = json!(operation_id);
        }
        let request = with_field(
            build("kiapi.common.ApiRequest", json!({"header": header})),
            "message",
            proto::pack(command),
        );
        let name = command.descriptor().name().to_owned();
        let raw = match nng::request(&path, &request.encode_to_vec(), self.timeout).await {
            Ok(raw) => raw,
            Err(TransportError::Timeout) => {
                return Err(Error::ReplyTimeout(format!(
                    "No reply from KiCad to {name} within {} ms; KiCad may still apply it",
                    self.timeout.as_millis()
                )))
            }
            Err(TransportError::Closed) => {
                return Err(Error::ReplyTimeout(format!(
                "KiCad closed the connection before replying to {name}; KiCad may still apply it"
            )))
            }
            Err(TransportError::Lost(detail)) => {
                return Err(Error::ReplyTimeout(format!(
                    "KiCad's reply to {name} could not be read ({detail}); KiCad may still apply it"
                )))
            }
            Err(TransportError::Protocol(detail)) => {
                return Err(Error::Runtime(format!(
                    "Cannot talk to KiCad at the selected endpoint: {detail}"
                )))
            }
        };
        let reply = DynamicMessage::decode(
            proto::descriptor("kiapi.common.ApiResponse"),
            raw.as_slice(),
        )
        .map_err(|_| {
            Error::Decode("Error parsing message with type 'kiapi.common.ApiResponse'".into())
        })?;
        let reply_header = message_field(&reply, "header");
        let revision = str_field(&reply_header, "document_revision");
        self.last_revision = (!revision.is_empty()).then_some(revision);
        let status = message_field(&reply, "status");
        let code = int_field(&status, "status");
        if code != 1 {
            let name = enum_name("kiapi.common.ApiStatusCode", code);
            let message = format!("{name}: {}", str_field(&status, "error_message"));
            return Err(match name.as_str() {
                "AS_REVISION_MISMATCH" => Error::RevisionMismatch(message),
                "AS_DUPLICATE_OPERATION" => Error::DuplicateOperation(message),
                _ => Error::Runtime(message),
            });
        }
        let response =
            proto::unpack(&message_field(&reply, "message"), response_type).ok_or_else(|| {
                Error::Runtime(format!(
                    "Unexpected reply to {}",
                    command.descriptor().full_name()
                ))
            })?;
        self.received_reply = true;
        if self.token.is_empty() {
            self.token = str_field(&reply_header, "kicad_token");
        }
        Ok(response)
    }

    // ---- Reads --------------------------------------------------------------------------------

    /// `documents()`: the running KiCad's version and its open schematics.
    pub async fn documents(&mut self) -> Result<Json, Error> {
        let version = self
            .send(
                &build(&format!("{C}GetVersion"), json!({})),
                &format!("{C}GetVersionResponse"),
            )
            .await?;
        let version = message_field(&version, "version");
        // These handlers first shipped on the 10.99 development branch for KiCad 11.
        if (int_field(&version, "major"), int_field(&version, "minor")) < (10, 99) {
            return Err(Error::Runtime(
                "Live schematic editing requires this KiCad development build (10.99/11+)".into(),
            ));
        }
        let documents = self.open_documents("DOCTYPE_SCHEMATIC").await?;
        Ok(json!({"version": reply_json(&version)?, "documents": documents}))
    }

    /// `boards()`: the boards open in the PCB editor (none when it is closed).
    pub async fn boards(&mut self) -> Result<Json, Error> {
        self.open_documents("DOCTYPE_PCB").await
    }

    async fn open_documents(&mut self, kind: &str) -> Result<Json, Error> {
        let request = build(&format!("{C}GetOpenDocuments"), json!({"type": kind}));
        match self
            .send(&request, &format!("{C}GetOpenDocumentsResponse"))
            .await
        {
            Ok(opened) => Ok(reply_json(&opened)?
                .get("documents")
                .cloned()
                .unwrap_or_else(|| json!([]))),
            // The project manager answers with no editor open
            Err(error)
                if error.is_runtime_error() && error.to_string().contains("AS_UNHANDLED") =>
            {
                Ok(json!([]))
            }
            Err(error) => Err(error),
        }
    }

    /// `hierarchy(doc)`.
    pub async fn hierarchy(&mut self, doc: &DynamicMessage) -> Result<Json, Error> {
        let request = with_field(
            build(&format!("{S}GetSchematicHierarchy"), json!({})),
            "document",
            doc.clone(),
        );
        let response = self
            .send(&request, &format!("{S}SchematicHierarchyResponse"))
            .await?;
        reply_json(&response)
    }

    /// `items(doc)`: every item on one sheet, unsaved changes included.
    pub async fn items(&mut self, doc: &DynamicMessage) -> Result<Json, Error> {
        let request = with_field(
            build(&format!("{C}GetItems"), json!({})),
            "header",
            item_header(doc),
        );
        let response = self.send(&request, &format!("{C}GetItemsResponse")).await?;
        let status = int_field(&response, "status");
        if status != 1 {
            return Err(Error::Runtime(format!(
                "GetItems failed: {}",
                enum_name(&format!("{T}ItemRequestStatus"), status)
            )));
        }
        reply_json(&response)
    }

    /// `document_revision(doc)`: the live revision, open API commits and unsaved state.
    pub async fn document_revision(&mut self, doc: &DynamicMessage) -> Result<Json, Error> {
        let request = with_field(
            build(&format!("{C}GetDocumentRevision"), json!({})),
            "document",
            doc.clone(),
        );
        let response = self
            .send(&request, &format!("{C}DocumentRevisionResponse"))
            .await?;
        Ok(json!({
            "revision": str_field(&response, "revision"),
            "open_commits": int_field(&response, "open_commits"),
            "modified": int_field(&response, "modified") != 0,
        }))
    }

    /// `operation_status(operation_id)`: what happened to a mutation; never resends anything.
    pub async fn operation_status(&mut self, operation_id: &str) -> Result<Json, Error> {
        if operation_id.is_empty() {
            return Err(Error::Value("operation_id is required".into()));
        }
        let request = build(
            &format!("{C}GetOperationStatus"),
            json!({"operation_id": operation_id}),
        );
        let response = self
            .send(&request, &format!("{C}OperationStatusResponse"))
            .await?;
        let mut result = reply_json(&response)?;
        result["state"] = json!(enum_name(
            &format!("{C}OperationState"),
            int_field(&response, "state")
        ));
        Ok(result)
    }

    // ---- Revisions and operation reconciliation (REVISIONS.md) ------------------------------

    async fn send_with(
        &mut self,
        command: &DynamicMessage,
        response_type: &str,
        expected_revision: Option<String>,
        operation_id: Option<String>,
    ) -> Result<DynamicMessage, Error> {
        self.pending_header = Some((
            expected_revision.unwrap_or_default(),
            operation_id.unwrap_or_default(),
        ));
        let result = self.send(command, response_type).await;
        self.pending_header = None;
        result
    }

    /// Starts `mutation_options(expected_revision, operation_id)` for one tool call.  The first
    /// mutation uses them as given; each later one expects the revision the previous one left and
    /// uses operation_id.2, .3 and so on.  [`Client::end_mutations`] returns what was done.
    pub fn begin_mutations(
        &mut self,
        expected_revision: Option<String>,
        operation_id: Option<String>,
    ) {
        self.options = Some(MutationOptions {
            expected_revision: expected_revision.filter(|s| !s.is_empty()),
            operation_id: operation_id.filter(|s| !s.is_empty()),
            operations: Vec::new(),
        });
    }

    /// Ends the tool call's mutation options and returns its operations.
    pub fn end_mutations(&mut self) -> Vec<Json> {
        self.options
            .take()
            .map(|o| o.operations)
            .unwrap_or_default()
    }

    fn next_mutation(&self) -> (Option<String>, String) {
        let Some(options) = &self.options else {
            return (None, new_operation_id());
        };
        let count = options.operations.len();
        let operation_id = match &options.operation_id {
            Some(base) if count == 0 => base.clone(),
            Some(base) => format!("{base}.{}", count + 1),
            None => new_operation_id(),
        };
        (options.expected_revision.clone(), operation_id)
    }

    fn mutation_done(&mut self, operation_id: &str, state: &str, reconciled: bool) {
        let mut entry = Map::new();
        entry.insert("operation_id".into(), json!(operation_id));
        entry.insert("state".into(), json!(state));
        entry.insert("revision".into(), json!(self.last_revision));
        if reconciled {
            entry.insert("reconciled".into(), json!(true));
        }
        let entry = Json::Object(entry);
        self.last_operation = Some(entry.clone());
        let last_revision = self.last_revision.clone();
        if let Some(options) = &mut self.options {
            options.operations.push(entry);
            if options.expected_revision.is_some() && state == "committed" {
                options.expected_revision = last_revision;
            }
        }
    }

    /// Asks KiCad once what became of a mutation whose reply was lost; never resends it.
    async fn reconcile(&mut self, operation_id: &str, error: &str) -> Result<Json, Error> {
        match self.operation_status(operation_id).await {
            Ok(status) => Ok(status),
            Err(query_error) => Err(Error::OutcomeUnknown {
                message: format!(
                    "{error}. The outcome of operation {operation_id} is unknown and could \
                     not be queried ({query_error}). Do not resend it; query \
                     live_operation_status later."
                ),
                operation_id: operation_id.to_owned(),
            }),
        }
    }

    /// One edit outside a commit, such as undo or settings. Reconcile a lost reply once;
    /// the original response cannot be recovered, so Python returns an empty response too.
    pub async fn send_mutation(
        &mut self,
        request: &DynamicMessage,
        response_type: &str,
    ) -> Result<DynamicMessage, Error> {
        let (expected, operation_id) = self.next_mutation();
        match self
            .send_with(request, response_type, expected, Some(operation_id.clone()))
            .await
        {
            Ok(response) => {
                self.mutation_done(&operation_id, "committed", false);
                Ok(response)
            }
            Err(Error::ReplyTimeout(error)) => {
                let status = self.reconcile(&operation_id, &error).await?;
                if status["state"] != "OPS_COMMITTED" {
                    return Err(Error::OutcomeUnknown { message: format!(
                        "{error}. Operation {operation_id} is {}; do not resend it. Query live_operation_status again.",
                        status["state"].as_str().unwrap_or("OPS_UNKNOWN")), operation_id });
                }
                self.mutation_done(&operation_id, "committed", true);
                Ok(DynamicMessage::new(proto::descriptor(response_type)))
            }
            Err(error) => Err(error),
        }
    }

    /// Execute only curated commands, preserving each command's file/edit/read semantics.
    pub async fn execute(&mut self, name: &str, payload: &Json) -> Result<Json, Error> {
        let command = crate::registry::resolve(name)?;
        let payload = crate::registry::request_payload(Some(payload));
        let request = pbjson::from_json(&payload, &proto::descriptor(&command.name))?;
        let response = if command.kind == "edit" && !command.project_edit {
            self.send_mutation(&request, &command.response).await?
        } else {
            self.send(&request, &command.response).await?
        };
        Ok(json!({"command": command.name, "kind": command.kind, "result": reply_json(&response)?}))
    }

    /// Validate every step before opening one commit for the batch.
    pub async fn batch(
        &mut self,
        doc: &DynamicMessage,
        steps: &[Json],
        description: &str,
    ) -> Result<Json, Error> {
        let steps = crate::batch::prepare(doc, steps)?;
        let header = item_header(doc);
        let (id, operation_id) = self.begin_transaction(&header).await?;
        let result = self.batch_steps(&steps).await;
        let outcome = result.as_ref().map(|_| ()).map_err(Clone::clone);
        self.finish_transaction(&header, &id, &operation_id, description, outcome)
            .await?;
        Ok(json!({"committed": true, "steps": result?}))
    }

    async fn batch_steps(&mut self, steps: &[crate::batch::Step]) -> Result<Vec<Json>, Error> {
        let mut results = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let response = self
                .send(&step.request, &step.command.response)
                .await
                .map_err(|error| {
                    Error::Runtime(format!(
                        "step {index} ({}) failed: {error}",
                        step.command.name
                    ))
                })?;
            crate::batch::check_result(index, step, &response)?;
            results.push(json!({"command": step.command.name, "result": reply_json(&response)?}));
        }
        Ok(results)
    }

    fn end_commit(
        header: &DynamicMessage,
        id: Option<&DynamicMessage>,
        action: &str,
        message: &str,
    ) -> DynamicMessage {
        let mut request = build(
            &format!("{C}EndCommit"),
            json!({"action": action, "message": message}),
        );
        request = with_field(request, "header", header.clone());
        if let Some(id) = id {
            request = with_field(request, "id", id.clone());
        }
        request
    }

    /// Opens a commit for one transaction (`transaction()` up to its `yield`): returns the commit
    /// id and the transaction's operation id.
    pub async fn begin_transaction(
        &mut self,
        header: &DynamicMessage,
    ) -> Result<(DynamicMessage, String), Error> {
        let (expected, operation_id) = self.next_mutation();
        let mut begin = with_field(
            build(&format!("{C}BeginCommit"), json!({})),
            "header",
            header.clone(),
        );
        if self.lease_seconds != 0 {
            begin.set_field_by_name("lease_seconds", Value::U32(self.lease_seconds));
        }
        match self
            .send_with(
                &begin,
                &format!("{C}BeginCommitResponse"),
                expected,
                Some(operation_id.clone()),
            )
            .await
        {
            Ok(started) => Ok((message_field(&started, "id"), operation_id)),
            Err(Error::ReplyTimeout(message)) => Err(self
                .abandon_lost_begin(header, &operation_id, &message)
                .await),
            Err(error) => Err(error),
        }
    }

    /// Ends a transaction: commits after `outcome` succeeded, drops (rolls back) after it failed.
    pub async fn finish_transaction(
        &mut self,
        header: &DynamicMessage,
        id: &DynamicMessage,
        operation_id: &str,
        description: &str,
        outcome: Result<(), Error>,
    ) -> Result<(), Error> {
        if let Err(original) = outcome {
            let drop = Self::end_commit(header, Some(id), "CMA_DROP", "");
            if let Err(rollback) = self.send(&drop, &format!("{C}EndCommitResponse")).await {
                return Err(Error::Runtime(format!(
                    "Edit failed ({original}); rollback unconfirmed ({rollback}). \
                     Operation {operation_id}: query live_operation_status and inspect \
                     the live editor before continuing."
                )));
            }
            self.mutation_done(operation_id, "rolled_back", false);
            return Err(original);
        }
        let commit = Self::end_commit(header, Some(id), "CMA_COMMIT", description);
        match self.send(&commit, &format!("{C}EndCommitResponse")).await {
            Ok(_) => {
                self.mutation_done(operation_id, "committed", false);
                Ok(())
            }
            Err(error @ Error::RevisionMismatch(_)) => {
                // KiCad rolled the commit back: the document changed while it was open
                self.mutation_done(operation_id, "rolled_back", false);
                Err(error)
            }
            Err(Error::ReplyTimeout(message)) => {
                let mut status = self.reconcile(operation_id, &message).await?;
                if status["state"] == "OPS_IN_PROGRESS" {
                    status = self
                        .settle_open_commit(header, operation_id, &message)
                        .await?;
                }
                let state = status["state"].as_str().unwrap_or_default().to_owned();
                match state.as_str() {
                    "OPS_COMMITTED" => {
                        self.last_revision = status
                            .get("revision_after")
                            .and_then(Json::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned);
                        self.mutation_done(operation_id, "committed", true);
                        Ok(())
                    }
                    "OPS_ROLLED_BACK" => {
                        self.mutation_done(operation_id, "rolled_back", true);
                        let reason = status
                            .get("message")
                            .and_then(Json::as_str)
                            .unwrap_or("rolled back");
                        Err(Error::Runtime(format!(
                            "Operation {operation_id} was not committed: {reason}"
                        )))
                    }
                    _ => Err(Error::OutcomeUnknown {
                        message: format!(
                            "{message}. Operation {operation_id} is {state}: it may \
                             still commit, or be rolled back when its lease expires. Do not \
                             resend it; query live_operation_status again."
                        ),
                        operation_id: operation_id.to_owned(),
                    }),
                }
            }
            Err(error) => Err(error),
        }
    }

    /// EndCommit's reply was lost and the commit still looks open: settle it with a drop.  KiCad
    /// answers requests in order, so a delivered EndCommit runs before this drop (which then
    /// finds no commit and changes nothing); an undelivered one leaves the commit open and the
    /// drop rolls it back.  A drop never applies changes, so this is not a resend.
    async fn settle_open_commit(
        &mut self,
        header: &DynamicMessage,
        operation_id: &str,
        error: &str,
    ) -> Result<Json, Error> {
        let drop = Self::end_commit(header, None, "CMA_DROP", "");
        match self.send(&drop, &format!("{C}EndCommitResponse")).await {
            Ok(_) => {}
            Err(Error::ReplyTimeout(_)) => return Ok(json!({"state": "OPS_IN_PROGRESS"})),
            // no commit left to drop: the delivered EndCommit settled it
            Err(e) if e.is_runtime_error() => {}
            Err(e) => return Err(e),
        }
        self.reconcile(operation_id, error).await
    }

    /// BeginCommit's reply was lost.  No edit was sent; close the commit if it opened.
    async fn abandon_lost_begin(
        &mut self,
        header: &DynamicMessage,
        operation_id: &str,
        error: &str,
    ) -> Error {
        let status = match self.reconcile(operation_id, error).await {
            Ok(status) => status,
            Err(e) => return e,
        };
        let state = status["state"].as_str().unwrap_or_default().to_owned();
        match state.as_str() {
            "OPS_IN_PROGRESS" => {
                // Drop needs no id ("a safety net in case the id was lost on the client side")
                let drop = Self::end_commit(header, None, "CMA_DROP", "");
                if let Err(e) = self.send(&drop, &format!("{C}EndCommitResponse")).await {
                    return e;
                }
                Error::Runtime(format!(
                    "{error}. The commit opened but its reply was lost; it was dropped and \
                     nothing was changed."
                ))
            }
            "OPS_REJECTED" => Error::RevisionMismatch(format!(
                "{error}. KiCad refused operation {operation_id}: {}",
                status.get("message").and_then(Json::as_str).unwrap_or("")
            )),
            _ => Error::OutcomeUnknown {
                message: format!(
                    "{error}. KiCad has not seen operation {operation_id} ({state}). \
                     No edit was sent, so nothing was changed; a late commit is rolled back \
                     when its lease expires."
                ),
                operation_id: operation_id.to_owned(),
            },
        }
    }

    // ---- The edit transaction -----------------------------------------------------------------

    /// `mutate(doc, items)`: creates schematic items as one undo step (one commit), rolled back
    /// if KiCad rejects any of them.  Returns the CreateItemsResponse as protobuf JSON.
    pub async fn create_items(
        &mut self,
        doc: &DynamicMessage,
        items: &[DynamicMessage],
    ) -> Result<Json, Error> {
        self.mutate_items(doc, items, false).await
    }

    /// Replace complete items, preserving the same batch and transaction guarantees as create.
    pub async fn update_items(
        &mut self,
        doc: &DynamicMessage,
        items: &[DynamicMessage],
    ) -> Result<Json, Error> {
        self.mutate_items(doc, items, true).await
    }

    async fn mutate_items(
        &mut self,
        doc: &DynamicMessage,
        items: &[DynamicMessage],
        update: bool,
    ) -> Result<Json, Error> {
        if items.is_empty() || items.len() > MAX_ITEMS {
            return Err(Error::Value(format!(
                "Supply between 1 and {MAX_ITEMS} items"
            )));
        }
        let header = item_header(doc);
        let command = if update { "UpdateItems" } else { "CreateItems" };
        let mut request = with_field(
            build(&format!("{C}{command}"), json!({})),
            "header",
            header.clone(),
        );
        request.set_field_by_name(
            "items",
            Value::List(
                items
                    .iter()
                    .map(|i| Value::Message(proto::pack(i)))
                    .collect(),
            ),
        );

        let description = if update {
            "MCP: update items"
        } else {
            "MCP: create items"
        };
        self.item_transaction(&header, &request, items.len(), command, description)
            .await
    }

    /// Delete UUIDs as one undo step; validate the whole input before opening a commit.
    pub async fn delete_items(
        &mut self,
        doc: &DynamicMessage,
        ids: &[String],
    ) -> Result<Json, Error> {
        if ids.is_empty() || ids.len() > MAX_ITEMS {
            return Err(Error::Value(format!(
                "Supply between 1 and {MAX_ITEMS} UUIDs"
            )));
        }
        let ids = ids
            .iter()
            .map(|id| canonical_uuid(id).map(|id| json!({"value": id})))
            .collect::<Result<Vec<_>, _>>()?;
        let header = item_header(doc);
        let request = with_field(
            build(&format!("{C}DeleteItems"), json!({"item_ids": ids})),
            "header",
            header.clone(),
        );
        self.item_transaction(
            &header,
            &request,
            ids.len(),
            "DeleteItems",
            "MCP: delete items",
        )
        .await
    }

    async fn item_transaction(
        &mut self,
        header: &DynamicMessage,
        request: &DynamicMessage,
        count: usize,
        command: &str,
        description: &str,
    ) -> Result<Json, Error> {
        let (id, operation_id) = self.begin_transaction(header).await?;
        let step = self.item_step(request, count, command).await;
        let (outcome, response) = match step {
            Ok(response) => (Ok(()), Some(response)),
            Err(error) => (Err(error), None),
        };
        self.finish_transaction(header, &id, &operation_id, description, outcome)
            .await?;
        reply_json(&response.expect("a committed transaction has a response"))
    }

    /// `run_edit(doc, request, response_type, description)`: one editing command (annotate, move,
    /// place, ...) as its own undo step, one commit dropped if the command fails.  Returns the
    /// command's reply.
    pub async fn run_edit(
        &mut self,
        doc: &DynamicMessage,
        request: &DynamicMessage,
        response_type: &str,
        description: &str,
    ) -> Result<DynamicMessage, Error> {
        let header = item_header(doc);
        let (id, operation_id) = self.begin_transaction(&header).await?;
        let (outcome, response) = match self.send(request, response_type).await {
            Ok(response) => (Ok(()), Some(response)),
            Err(error) => (Err(error), None),
        };
        self.finish_transaction(&header, &id, &operation_id, description, outcome)
            .await?;
        Ok(response.expect("a committed transaction has a response"))
    }

    async fn item_step(
        &mut self,
        request: &DynamicMessage,
        count: usize,
        command: &str,
    ) -> Result<DynamicMessage, Error> {
        let response = self.send(request, &format!("{C}{command}Response")).await?;
        let field = match command {
            "UpdateItems" => "updated_items",
            "DeleteItems" => "deleted_items",
            _ => "created_items",
        };
        let results = proto::list_field(&response, field);
        if command == "DeleteItems" {
            if int_field(&response, "status") != 1 || results.len() != count
                || results.iter().any(|value| !matches!(value, Value::Message(item) if int_field(item, "status") == 1))
            {
                return Err(Error::Runtime(format!("Deletion rejected: {}", pbjson::py_repr(&reply_json(&response)?))));
            }
            return Ok(response);
        }
        if int_field(&response, "status") != 1 || results.len() != count {
            return Err(Error::Runtime(
                "KiCad did not acknowledge the entire item batch".into(),
            ));
        }
        let mut errors = Vec::new();
        for result in &results {
            let Value::Message(result) = result else {
                continue;
            };
            let status = message_field(result, "status");
            let code = int_field(&status, "code");
            if code != 1 {
                let message = str_field(&status, "error_message");
                errors.push(if message.is_empty() {
                    enum_name(&format!("{C}ItemStatusCode"), code)
                } else {
                    message
                });
            }
        }
        if !errors.is_empty() {
            return Err(Error::Runtime(errors.join("; ")));
        }
        Ok(response)
    }
}

/// Python uuid.UUID's string forms, normalized before any editor request is sent.
pub fn canonical_uuid(value: &str) -> Result<String, Error> {
    let hex = value.replace("urn:", "").replace("uuid:", "");
    let hex = hex.trim_matches(['{', '}']).replace('-', "");
    if hex.chars().count() != 32 {
        return Err(Error::Value("badly formed hexadecimal UUID string".into()));
    }
    let number = u128::from_str_radix(&hex, 16).map_err(|_| {
        Error::Value(format!(
            "invalid literal for int() with base 16: {}",
            pbjson::py_repr(&json!(hex))
        ))
    })?;
    Ok(uuid::Uuid::from_u128(number).to_string())
}

/// `ItemHeader(document=doc)`.
pub fn item_header(doc: &DynamicMessage) -> DynamicMessage {
    with_field(
        build(&format!("{T}ItemHeader"), json!({})),
        "document",
        doc.clone(),
    )
}

// ---- Tool argument documents (client.document and friends) -----------------------------------

fn document_specifier(value: &Json) -> Result<DynamicMessage, Error> {
    Ok(pbjson::from_json(
        value,
        &proto::descriptor(&format!("{T}DocumentSpecifier")),
    )?)
}

fn doc_type(spec: &DynamicMessage) -> String {
    enum_name(&format!("{T}DocumentType"), int_field(spec, "type"))
}

/// `document(value)`: a schematic sheet document as live_schematic_documents returns it.
pub fn schematic_document(value: &Json) -> Result<DynamicMessage, Error> {
    let spec = document_specifier(value)?;
    if doc_type(&spec) != "DOCTYPE_SCHEMATIC" {
        return Err(Error::Value(
            "Use a schematic document returned by live_schematic_documents".into(),
        ));
    }
    let project_name = str_field(&message_field(&spec, "project"), "name");
    let sheet_path = proto::list_field(&message_field(&spec, "sheet_path"), "path");
    if project_name.is_empty() || sheet_path.is_empty() {
        return Err(Error::Value(
            "An explicit project and UUID sheet_path are required".into(),
        ));
    }
    Ok(spec)
}

/// `board_document(value)`.
pub fn board_document(value: &Json) -> Result<DynamicMessage, Error> {
    let spec = document_specifier(value)?;
    if doc_type(&spec) != "DOCTYPE_PCB" || str_field(&spec, "board_filename").is_empty() {
        return Err(Error::Value(
            "Use a board document returned by live_pcb_open or live_pcb_documents".into(),
        ));
    }
    Ok(spec)
}

fn library_document(value: &Json, kind: &str, message: &str) -> Result<DynamicMessage, Error> {
    let spec = document_specifier(value)?;
    if doc_type(&spec) != kind
        || str_field(&message_field(&spec, "lib_id"), "entry_name").is_empty()
    {
        return Err(Error::Value(message.into()));
    }
    Ok(spec)
}

/// `symbol_document(value)`: a DOCTYPE_SYMBOL document as live_symbol_new/edit return it.
pub fn symbol_document(value: &Json) -> Result<DynamicMessage, Error> {
    library_document(
        value,
        "DOCTYPE_SYMBOL",
        "Use the symbol document returned by live_symbol_new or live_symbol_edit",
    )
}

/// `footprint_document(value)`: a DOCTYPE_FOOTPRINT document as live_footprint_new/edit return it.
pub fn footprint_document(value: &Json) -> Result<DynamicMessage, Error> {
    library_document(
        value,
        "DOCTYPE_FOOTPRINT",
        "Use the footprint document returned by live_footprint_new or live_footprint_edit",
    )
}

/// `any_editor_document(spec)` from server.py: a footprint document when the raw `type` says so,
/// otherwise a symbol document.
pub fn any_editor_document(value: &Json) -> Result<DynamicMessage, Error> {
    match value.get("type").and_then(Json::as_str) {
        Some("DOCTYPE_FOOTPRINT") => footprint_document(value),
        _ => symbol_document(value),
    }
}

/// `editor_document(spec)` from server.py: a schematic, board, symbol or footprint document,
/// chosen by the raw `type` value as Python's `spec.get("type")` sees it.
pub fn editor_document(value: &Json) -> Result<DynamicMessage, Error> {
    match value.get("type").and_then(Json::as_str) {
        Some("DOCTYPE_PCB") => board_document(value),
        Some("DOCTYPE_FOOTPRINT") => footprint_document(value),
        Some("DOCTYPE_SYMBOL") => symbol_document(value),
        _ => schematic_document(value),
    }
}

/// `parse_items(values)`: protobuf-JSON Any objects of top-level schematic item types only.
pub fn parse_schematic_items(values: &[Json]) -> Result<Vec<DynamicMessage>, Error> {
    let file = proto::pool()
        .get_file_by_name("schematic/schematic_types.proto")
        .expect("schematic types in the protocol");
    let any = proto::descriptor("google.protobuf.Any");
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let packed = pbjson::from_json(value, &any)?;
        let type_name = proto::any_type_name(&packed);
        let name = type_name
            .strip_prefix("kiapi.schematic.types.")
            .unwrap_or(&type_name);
        let descriptor = file.messages().find(|m| m.name() == name);
        let Some(descriptor) = descriptor.filter(|d| d.full_name() == type_name) else {
            return Err(Error::Value(
                "Only schematic item types are accepted".into(),
            ));
        };
        let message = proto::unpack(&packed, descriptor.full_name())
            .ok_or_else(|| Error::Value("Invalid schematic item".into()))?;
        result.push(message);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Json {
        json!({"type": "DOCTYPE_SCHEMATIC", "project": {"name": "p"}, "sheet_path": {"path": [{"value": "u"}]}})
    }

    #[test]
    fn documents_are_checked_like_python() {
        assert!(schematic_document(&sheet()).is_ok());
        let error = schematic_document(&json!({"type": "DOCTYPE_SCHEMATIC"})).unwrap_err();
        assert_eq!(
            error,
            Error::Value("An explicit project and UUID sheet_path are required".into())
        );
        let error = editor_document(&json!({"type": "DOCTYPE_PCB"})).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Use a board document returned by live_pcb_open or live_pcb_documents"
        );
        let error =
            editor_document(&json!({"type": 3, "board_filename": "x.kicad_pcb"})).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Use a schematic document returned by live_schematic_documents"
        );
        assert!(
            editor_document(&json!({"type": "DOCTYPE_SYMBOL", "lib_id": {"entry_name": "R"}}))
                .is_ok()
        );
    }

    #[test]
    fn only_schematic_items_are_accepted() {
        let line = json!({"@type": "type.googleapis.com/kiapi.schematic.types.SchematicLine"});
        assert_eq!(parse_schematic_items(&[line]).unwrap().len(), 1);
        for other in [
            json!({"@type": "type.googleapis.com/kiapi.common.types.KIID"}),
            json!({}),
            json!({"@type": "type.googleapis.com/kiapi.board.types.Track"}),
        ] {
            assert_eq!(
                parse_schematic_items(&[other]).unwrap_err(),
                Error::Value("Only schematic item types are accepted".into())
            );
        }
    }

    #[test]
    fn mutation_options_number_later_operations() {
        let mut client = Client::new(Some(r"ipc:///tmp/unused/api.sock".into()), 10)
            .unwrap_or_else(|_| Client::new(Some(r"ipc://C:\unused\api.sock".into()), 10).unwrap());
        client.begin_mutations(Some("e-1".into()), Some("op".into()));
        assert_eq!(client.next_mutation(), (Some("e-1".into()), "op".into()));
        client.last_revision = Some("e-2".into());
        client.mutation_done("op", "committed", false);
        assert_eq!(client.next_mutation(), (Some("e-2".into()), "op.2".into()));
        let operations = client.end_mutations();
        assert_eq!(
            operations,
            [json!({"operation_id": "op", "state": "committed", "revision": "e-2"})]
        );
        assert!(client.next_mutation().1.starts_with("mcp-"));
    }
}
