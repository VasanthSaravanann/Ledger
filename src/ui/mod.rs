//! `ledger ui` — the local recruiter workspace (V5 Phase 4).
//!
//! Design constraints this module must not break:
//!
//! * **No outbound calls.** The server binds `127.0.0.1` only; nothing here dials
//!   out, and every asset is embedded with `include_str!`.
//! * **One operator per process.** The session is bound at launch from
//!   `--operator`, because identity today is a name plus a `0600` keyfile, not an
//!   authenticated account. Switching operators is a relaunch, not a dropdown.
//! * **The workspace lock is the correctness boundary.** Mutations take
//!   `Store::lock` (fs2) inside `spawn_blocking`; reads take a fresh snapshot and
//!   never block on the CLI holding the lock.
//! * **All business logic is delegated.** Handlers call `pipeline`, `disposition`,
//!   `verify`, `governance`; they never re-implement a rule.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};

use crate::approval::ArtefactStatus;
use crate::ats::Roster;
use crate::disposition::{
    AdminReason, Disposition, DispositionKind, Dispositions, FormSubmission, NewDisposition,
};
use crate::error::Error;
use crate::gate::{GateAnswer, GateKind, GateLibrary};
use crate::id::{ApplicationId, DecisionMakerId, RequisitionId, TemplateKind};
use crate::identity::{Operator, Registry, Role};
use crate::requisition::{Requisition, RequisitionBook};
use crate::store::Store;
use crate::template::{MergeContext, Templates, render};

const TOKENS_CSS: &str = include_str!("assets/tokens.css");
const APP_CSS: &str = include_str!("assets/app.css");
const APP_JS: &str = include_str!("assets/app.js");

/// How the server was launched.
#[derive(Debug, Clone)]
pub struct Config {
    /// Workspace directory.
    pub data: std::path::PathBuf,
    /// The operator this process acts as.
    pub operator: String,
    /// Loopback port.
    pub port: u16,
}

/// One process = one operator = one session.
#[derive(Debug)]
struct AppState {
    store: Store,
    operator: Operator,
    session: String,
    launch_token: String,
    token_used: AtomicBool,
    requisition: Requisition,
    library: GateLibrary,
}

/// An error that already knows its HTTP status.
#[derive(Debug)]
pub struct UiError {
    status: StatusCode,
    message: String,
}

impl UiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "no session; open the launch URL")
    }

    fn forbidden(role: Role, action: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, format!("{role} may not {action}"))
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl From<Error> for UiError {
    fn from(error: Error) -> Self {
        let status = match &error {
            Error::Locked { .. } => StatusCode::CONFLICT,
            Error::Forbidden { .. } => StatusCode::FORBIDDEN,
            Error::Invalid { .. }
            | Error::UnknownRequisition(_)
            | Error::UnknownApplication(_)
            | Error::DisallowedAnswer { .. }
            | Error::GateUnavailable { .. }
            | Error::GateNotInRequisition { .. }
            | Error::DuplicateDisposition { .. }
            | Error::AlreadySuperseded(_)
            | Error::SupersedeMismatch { .. } => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(status, error.to_string())
    }
}

impl IntoResponse for UiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "ok": false,
                "error": self.message,
                "status": self.status.as_u16(),
            })),
        )
            .into_response()
    }
}

/// Runs the server until the process is stopped.
///
/// # Errors
///
/// Returns [`Error::Invalid`] for an unknown operator, and [`Error::Io`] when the
/// port cannot be bound.
pub fn run(config: Config) -> crate::error::Result<()> {
    let store = Store::open(&config.data)?;
    let registry = Registry::load(&store)?;
    let operator = crate::identity::active(&registry, Some(&config.operator))?;
    // Touch the key at startup: a session that cannot sign is not a session.
    registry.signing_key(&store, &operator.name)?;

    let book = RequisitionBook::load(&store)?;
    let requisition = book
        .iter()
        .next()
        .cloned()
        .ok_or_else(|| Error::invalid("workspace", "no requisition to work on"))?;
    let library: GateLibrary = store
        .load(crate::store::GATE_LIBRARY)?
        .ok_or_else(|| Error::invalid("workspace", "no gate library"))?;

    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random)
        .map_err(|error| Error::invalid("session", error.to_string()))?;
    let session = crate::identity::to_hex(&random);
    let mut token_bytes = [0u8; 32];
    getrandom::getrandom(&mut token_bytes)
        .map_err(|error| Error::invalid("session token", error.to_string()))?;
    let launch_token = crate::identity::to_hex(&token_bytes);

    let state = Arc::new(AppState {
        store,
        operator,
        session: session.clone(),
        launch_token: launch_token.clone(),
        token_used: AtomicBool::new(false),
        requisition,
        library,
    });

    let app = axum::Router::new()
        .route("/", get(index))
        .route("/governance", get(governance))
        .route("/audit", get(audit))
        .route("/api/preview", post(preview))
        .route("/api/dispose", post(dispose_one))
        .route("/api/dispose/batch", post(dispose_batch))
        .with_state(Arc::clone(&state));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::invalid("runtime", error.to_string()))?;
    runtime.block_on(async move {
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], config.port));
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| Error::Io {
                path: std::path::PathBuf::from(address.to_string()),
                source: error,
            })?;
        println!(
            "ledger ui: http://127.0.0.1:{}/?token={}",
            config.port, launch_token
        );
        println!(
            "operator: {} ({})  workspace: {}",
            state.operator.name,
            state.operator.role,
            state.store.root().display()
        );
        println!("open the URL once; it exchanges the token for a session cookie");
        axum::serve(listener, app).await.map_err(|error| Error::Io {
            path: std::path::PathBuf::from("127.0.0.1"),
            source: error,
        })
    })
}

// ---------------------------------------------------------------- auth & guard

/// Reads the session cookie.
fn session_from(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == "ledger_session").then(|| value.to_owned())
    })
}

fn guard(state: &AppState, headers: &HeaderMap) -> Result<(), UiError> {
    match session_from(headers) {
        Some(session) if session == state.session => Ok(()),
        _ => Err(UiError::unauthorized()),
    }
}

fn require(state: &AppState, action: &str) -> Result<(), UiError> {
    if state.operator.role.permits(action) {
        Ok(())
    } else {
        Err(UiError::forbidden(state.operator.role, action))
    }
}

// ------------------------------------------------------------------- mutation

/// Runs `work` with the workspace lock held, on the blocking pool.
async fn locked<T, F>(state: Arc<AppState>, work: F) -> Result<T, UiError>
where
    F: FnOnce(&Store) -> crate::error::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let _lock = state.store.lock()?;
        work(&state.store)
    })
    .await
    .map_err(|error| UiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(UiError::from)
}

// ---------------------------------------------------------------------- pages

fn layout(state: &AppState, title: &str, current: &str, body: &str) -> String {
    let link = |path: &str, label: &str| {
        let current = if path == current {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!("<a href=\"{path}\"{current}>{label}</a>")
    };
    format!(
        "<!doctype html><html lang=\"en-AU\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><title>Ledger — {title}</title>\
         <style>{TOKENS_CSS}{APP_CSS}</style></head><body><div class=\"app\">\
         <nav class=\"side\"><p class=\"brand\">Ledger</p>{}{}{}{}\
         <p class=\"role\"><b>{}</b><br>{} · local only</p></nav>\
         <main>{body}</main></div><script>{APP_JS}</script></body></html>",
        link("/", "Triage"),
        link("/governance", "Governance"),
        link("/audit", "Audit"),
        link("/audit", "Import (CLI)"),
        escape(&state.operator.name),
        state.operator.role,
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Derives the queue state of one applicant.
fn queue_state(
    dispositions: &Dispositions,
    requisition: &RequisitionId,
    application: &ApplicationId,
) -> String {
    match dispositions.head(requisition, application) {
        None => "pending".to_owned(),
        Some(record) => record.form.disposition.kind().code().to_lowercase(),
    }
}

fn tag_for(state: &str) -> String {
    let class = match state {
        "gate" => "tag-gate",
        "field" => "tag-field",
        "admin" => "tag-admin",
        _ => "tag-pending",
    };
    let label = match state {
        "pending" => "Pending",
        "gate" => "GATE",
        "field" => "FIELD",
        "admin" => "ADMIN",
        other => other,
    };
    format!("<span class=\"tag {class}\">{label}</span>")
}

async fn index(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    // Token exchange: one GET carries the token, then the cookie takes over.
    if let Some(token) = query.get("token") {
        if *token != state.launch_token || state.token_used.swap(true, Ordering::SeqCst) {
            return UiError::new(
                StatusCode::UNAUTHORIZED,
                "launch token already used or wrong",
            )
            .into_response();
        }
        let cookie = format!(
            "ledger_session={}; HttpOnly; SameSite=Strict; Path=/",
            state.session
        );
        return (
            StatusCode::SEE_OTHER,
            [
                (header::SET_COOKIE, cookie),
                (header::LOCATION, "/".to_owned()),
            ],
        )
            .into_response();
    }
    if let Err(error) = guard(&state, &headers) {
        return error.into_response();
    }

    let Ok(roster) = Roster::load(&state.store, &state.requisition.id) else {
        return UiError::new(StatusCode::INTERNAL_SERVER_ERROR, "roster unreadable")
            .into_response();
    };
    let Some(roster) = roster else {
        return Html(layout(
            &state,
            "Triage",
            "/",
            "<h1>No roster imported</h1><p class=\"sub\">Run <code>ledger ats import</code> first.</p>",
        ))
        .into_response();
    };
    let Ok(dispositions) = Dispositions::open(&state.store) else {
        return UiError::new(StatusCode::INTERNAL_SERVER_ERROR, "dispositions unreadable")
            .into_response();
    };

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = String::new();
    for applicant in &roster.applicants {
        let queue = queue_state(
            &dispositions,
            &state.requisition.id,
            &applicant.application_id,
        );
        *counts.entry(queue.clone()).or_insert(0) += 1;
        rows.push_str(&format!(
            "<tr data-status=\"{queue}\" data-app=\"{app}\">\
             <td><input type=\"checkbox\" data-app=\"{app}\" aria-label=\"select {app}\"></td>\
             <td class=\"mono\">{app}</td><td>{name}</td><td class=\"mono\">{email}</td>\
             <td>{tag}</td><td class=\"mono\">{dm}</td></tr>",
            app = escape(applicant.application_id.as_str()),
            name = escape(&applicant.first_name),
            email = escape(applicant.email.as_str()),
            tag = tag_for(&queue),
            dm = escape(
                dispositions
                    .head(&state.requisition.id, &applicant.application_id)
                    .map(|record| record.form.decision_maker.to_string())
                    .unwrap_or_else(|| "—".to_owned())
                    .as_str()
            ),
        ));
    }

    let total = roster.applicants.len();
    let pending = counts.get("pending").copied().unwrap_or(0);
    let gate = counts.get("gate").copied().unwrap_or(0);
    let field = counts.get("field").copied().unwrap_or(0);
    let admin = counts.get("admin").copied().unwrap_or(0);
    let pct = (pending.saturating_mul(100))
        .checked_div(total)
        .unwrap_or(0);

    let body = format!(
        "<h1>Triage</h1><p class=\"sub\">{req} · {title} · {sent} {excluded}</p>\
         <section class=\"kpis\">\
         <div class=\"kpi\"><h2>Pending triage</h2><p class=\"n\">{pending}</p><p class=\"d\">{pct}% of {total}</p></div>\
         <div class=\"kpi\"><h2>Disposed</h2><p class=\"n\">{disposed}</p><p class=\"d\">GATE {gate} · FIELD {field} · ADMIN {admin}</p></div>\
         <div class=\"kpi\"><h2>Dispatch queue</h2><p class=\"n\">{sends}</p><p class=\"d\">recorded sends</p></div>\
         <div class=\"kpi\"><h2>Governance</h2><p class=\"n\">{valid}/7</p><p class=\"d\">artefacts valid</p></div>\
         </section>\
         <section class=\"card\"><div class=\"toolbar\">\
         <button class=\"pill\" data-filter=\"all\" aria-pressed=\"true\">All</button>\
         <button class=\"pill\" data-filter=\"pending\" aria-pressed=\"false\">Pending</button>\
         <button class=\"pill\" data-filter=\"gate\" aria-pressed=\"false\">GATE</button>\
         <button class=\"pill\" data-filter=\"field\" aria-pressed=\"false\">FIELD</button>\
         <button class=\"pill\" data-filter=\"admin\" aria-pressed=\"false\">ADMIN</button>\
         </div><table data-triage-table><thead><tr><th></th><th>Application</th><th>Name</th>\
         <th>Email</th><th>State</th><th>Decision maker</th></tr></thead><tbody>{rows}</tbody></table></section>\
         <div class=\"bulk\" data-bulk data-open=\"false\">\
         <span data-bulk-count>0 selected</span>\
         <button class=\"ghost\" disabled>Export preview</button>\
         <button class=\"primary\" disabled>Bulk dispose</button></div>",
        req = escape(state.requisition.id.as_str()),
        title = escape(&state.requisition.title),
        sent = roster.applicants.len(),
        excluded = if roster.excluded_not_initial_screen == 0 {
            String::new()
        } else {
            format!(
                "· {} excluded at import",
                roster.excluded_not_initial_screen
            )
        },
        disposed = gate + field + admin,
        sends = crate::log::SendLog::open(&state.store)
            .map(|log| log.len())
            .unwrap_or(0),
        valid = crate::verify::verify_workspace(&state.store)
            .map(|report| report
                .artefacts
                .iter()
                .filter(|a| a.status.is_valid())
                .count())
            .unwrap_or(0),
    );
    Html(layout(&state, "Triage", "/", &body)).into_response()
}

async fn governance(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(error) = guard(&state, &headers) {
        return error.into_response();
    }
    let Ok(report) = crate::verify::verify_workspace(&state.store) else {
        return UiError::new(StatusCode::INTERNAL_SERVER_ERROR, "verification failed")
            .into_response();
    };
    let mut rows = String::new();
    for artefact in &report.artefacts {
        let (class, label) = match &artefact.status {
            ArtefactStatus::Valid => ("tag-valid", "Valid".to_owned()),
            ArtefactStatus::Stale { reasons } => {
                ("tag-stale", format!("Stale — {}", reasons.join("; ")))
            }
            ArtefactStatus::Unsigned => ("tag-unsigned", "Unsigned".to_owned()),
            ArtefactStatus::Invalid { roles } => {
                ("tag-invalid", format!("Invalid — {}", roles.join(", ")))
            }
        };
        rows.push_str(&format!(
            "<tr><td class=\"mono\">{}</td><td><span class=\"tag {class}\">{}</span></td></tr>",
            escape(&artefact.label),
            escape(&label)
        ));
    }
    let checklist = crate::checklist::Checklist::load(&state.store)
        .map(|checklist| checklist.progress())
        .unwrap_or((0, 0));
    let body = format!(
        "<h1>Governance</h1><p class=\"sub\">Sign-offs are bound to the copy they approved; \
         editing copy makes them stale rather than silently valid.</p>\
         <section class=\"card\"><table><thead><tr><th>Artefact</th><th>Status</th></tr></thead>\
         <tbody>{rows}</tbody></table></section>\
         <h2 style=\"font-size:1rem;margin:22px 0 8px\">Operational checklist</h2>\
         <p class=\"sub\">{done}/{total} attested — these are unsigned attestations, not keyed \
         sign-offs.</p>",
        done = checklist.0,
        total = checklist.1,
    );
    Html(layout(&state, "Governance", "/governance", &body)).into_response()
}

async fn audit(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(error) = guard(&state, &headers) {
        return error.into_response();
    }
    let report = crate::verify::verify_workspace(&state.store);
    let chain = crate::log::SendLog::open(&state.store).and_then(|log| log.verification());
    let (chain_line, chain_class) = match &chain {
        Ok(verification) => (
            format!(
                "{} entries · head {}",
                verification.entries,
                &verification.head[0..16.min(verification.head.len())]
            ),
            "tag-valid",
        ),
        Err(error) => (error.to_string(), "tag-invalid"),
    };
    let notes = crate::operator::OperatorLog::open(&state.store)
        .all()
        .unwrap_or_default();
    let mut activity = String::new();
    for note in notes.iter().rev().take(15) {
        activity.push_str(&format!(
            "<tr><td class=\"mono\">{}</td><td>{}</td><td>{}</td></tr>",
            escape(&note.at.to_rfc3339()),
            escape(&note.operator),
            escape(&note.command)
        ));
    }
    let body = format!(
        "<h1>Audit</h1><p class=\"sub\">Chain and signatures, verified on every request.</p>\
         <section class=\"card\"><table><tbody>\
         <tr><th>Send log chain</th><td><span class=\"tag {chain_class}\">{chain_line}</span></td></tr>\
         <tr><th>Signatures</th><td>{sig}</td></tr></tbody></table></section>\
         <h2 style=\"font-size:1rem;margin:22px 0 8px\">Operator activity</h2>\
         <section class=\"card\"><table><thead><tr><th>When</th><th>Who</th><th>Command</th></tr></thead>\
         <tbody>{activity}</tbody></table></section>",
        sig = match &report {
            Ok(report) if report.is_clean() => "all valid".to_owned(),
            Ok(_) => "see governance".to_owned(),
            Err(error) => format!("verification failed: {error}"),
        },
    );
    Html(layout(&state, "Audit", "/audit", &body)).into_response()
}

// ------------------------------------------------------------------ JSON API

#[derive(Debug, Deserialize)]
struct DispositionInput {
    application_id: String,
    disposition: String,
    #[serde(default)]
    gate: Option<String>,
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    decision_maker: Option<String>,
    /// A correction names the current record it replaces.
    #[serde(default)]
    supersedes: Option<String>,
}

impl DispositionInput {
    fn to_disposition(&self) -> Result<Disposition, UiError> {
        match self.disposition.trim().to_uppercase().as_str() {
            "GATE" => {
                let gate = self
                    .gate
                    .as_deref()
                    .ok_or_else(|| UiError::bad_request("GATE needs gate"))?
                    .parse::<GateKind>()?;
                let answer = self
                    .answer
                    .as_deref()
                    .ok_or_else(|| UiError::bad_request("GATE needs answer"))?
                    .parse::<GateAnswer>()?;
                Ok(Disposition::Gate { gate, answer })
            }
            "FIELD" => Ok(Disposition::Field),
            "ADMIN" => {
                let reason = self
                    .reason
                    .as_deref()
                    .ok_or_else(|| UiError::bad_request("ADMIN needs reason"))?
                    .parse::<AdminReason>()?;
                Ok(Disposition::Admin { reason })
            }
            other => Err(UiError::bad_request(format!(
                "unknown disposition {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Serialize)]
struct PreviewResponse {
    subject: String,
    body: String,
    text_digest: String,
}

async fn preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<DispositionInput>,
) -> Result<Json<PreviewResponse>, UiError> {
    guard(&state, &headers)?;
    let disposition = input.to_disposition()?;
    let application = ApplicationId::try_from(input.application_id.as_str())?;
    let roster = Roster::load(&state.store, &state.requisition.id)?
        .ok_or_else(|| UiError::bad_request("no roster imported"))?;
    let applicant = roster.require(&application)?;
    let templates: Templates = state
        .store
        .load(crate::store::TEMPLATES)?
        .ok_or_else(|| UiError::bad_request("no templates"))?;
    let gate_text = match &disposition {
        Disposition::Gate { gate, .. } => Some(
            state
                .library
                .require(*gate)?
                .render(&state.requisition.facts)?,
        ),
        _ => None,
    };
    let template = templates.require(disposition.kind().template_kind())?;
    let rendered = render(
        template,
        &MergeContext {
            candidate_first_name: &applicant.first_name,
            requisition: &state.requisition,
            disposition: &disposition,
            gate_text: gate_text.as_ref(),
        },
    )?;
    Ok(Json(PreviewResponse {
        text_digest: rendered.digest(),
        subject: rendered.subject,
        body: rendered.body,
    }))
}

#[derive(Debug, Serialize)]
struct DisposeResponse {
    ok: bool,
    records: Vec<String>,
}

async fn dispose_one(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<DispositionInput>,
) -> Result<Json<DisposeResponse>, UiError> {
    dispose(state, headers, vec![input]).await
}

async fn dispose_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(inputs): Json<Vec<DispositionInput>>,
) -> Result<Json<DisposeResponse>, UiError> {
    dispose(state, headers, inputs).await
}

async fn dispose(
    state: Arc<AppState>,
    headers: HeaderMap,
    inputs: Vec<DispositionInput>,
) -> Result<Json<DisposeResponse>, UiError> {
    guard(&state, &headers)?;
    require(&state, "disposition")?;
    if inputs.is_empty() {
        return Err(UiError::bad_request("no dispositions submitted"));
    }

    let operator = state.operator.name.clone();
    let registry = Registry::load(&state.store)?;
    let requisition = state.requisition.clone();
    let library = state.library.clone();
    let inputs_owned = inputs;

    let records = locked(state, move |store| {
        let roster = Roster::load(store, &requisition.id)?
            .ok_or_else(|| Error::invalid("roster", "no roster imported"))?;
        let mut forms = Vec::with_capacity(inputs_owned.len());
        for input in &inputs_owned {
            let application = ApplicationId::try_from(input.application_id.as_str())?;
            let applicant = roster.require(&application)?;
            let form = FormSubmission {
                application_id: applicant.application_id.clone(),
                candidate_first_name: applicant.first_name.clone(),
                candidate_email: applicant.email.clone(),
                recruiter: crate::id::RecruiterId::try_from(operator.as_str())?,
                decision_maker: DecisionMakerId::try_from(
                    input.decision_maker.as_deref().unwrap_or(operator.as_str()),
                )?,
                disposition: input
                    .to_disposition()
                    .map_err(|error| Error::invalid("disposition", error.message))?,
                supersedes: input
                    .supersedes
                    .as_deref()
                    .map(crate::id::DispositionId::try_from)
                    .transpose()?,
            };
            let captured_at = chrono::Utc::now();
            let new = NewDisposition::try_new(&requisition, &library, form, captured_at)?;
            forms.push(new.signed(&registry, store, &operator)?);
        }
        let mut dispositions = Dispositions::open(store)?;
        let stored = dispositions.record_batch(forms)?;
        Ok(stored
            .into_iter()
            .map(|record| record.id.to_string())
            .collect::<Vec<String>>())
    })
    .await?;

    Ok(Json(DisposeResponse { ok: true, records }))
}

/// A one-line description of the queue, for tests and the CLI banner.
pub fn queue_summary(state: &str) -> &'static str {
    match state {
        "gate" => "GATE",
        "field" => "FIELD",
        "admin" => "ADMIN",
        _ => "Pending",
    }
}

/// The template a disposition kind renders with.
pub fn template_for(kind: DispositionKind) -> TemplateKind {
    kind.template_kind()
}

/// Exposed so a future asset route can serve the same bytes the pages inline.
pub fn assets() -> [(&'static str, &'static str); 3] {
    [
        ("tokens.css", TOKENS_CSS),
        ("app.css", APP_CSS),
        ("app.js", APP_JS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_the_html_metacharacters() {
        assert_eq!(escape("<a&b\"c'd>"), "&lt;a&amp;b&quot;c&#39;d&gt;");
    }

    #[test]
    fn queue_summary_labels_are_stable() {
        assert_eq!(queue_summary("pending"), "Pending");
        assert_eq!(queue_summary("gate"), "GATE");
        assert_eq!(queue_summary("weird"), "Pending");
    }

    #[test]
    fn assets_are_embedded() {
        let assets = assets();
        assert_eq!(assets.len(), 3);
        assert!(assets.iter().all(|(_, body)| !body.is_empty()));
    }
}
