//! JoveWorks Hub is a deliberately small distribution backend.
//!
//! It stores immutable catalogue versions and published NodeBook snapshots.
//! A publication URL contains only a random identifier; formula bodies remain
//! in catalogue resources, never in a graph document.

use std::{env, net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{info, warn};

const PROTOCOL_VERSION: u8 = 1;
const ADMIN_TOKEN_HEADER: &str = "x-joveworks-admin-token";
const COURSE_TOKEN_HEADER: &str = "x-joveworks-course-token";
const WORKSPACE_TOKEN_HEADER: &str = "x-joveworks-workspace-token";

#[derive(Clone)]
struct AppState {
    database: SqlitePool,
    admin_token: Arc<str>,
    /// Restricted catalogues can never accidentally become public. Setting this
    /// is required before one may be downloaded.
    course_token: Option<Arc<str>>,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("the request body is not valid: {0}")]
    BadRequest(String),
    #[error("an item with these immutable coordinates already exists")]
    Conflict,
    #[error("the requested resource was not found")]
    NotFound,
    #[error("this resource requires course access")]
    Unauthorized,
    #[error("storage failed")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict => StatusCode::CONFLICT,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if matches!(self, Self::Database(_)) {
            warn!(error = %self, "database request failed");
        }
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Discovery {
    protocol_version: u8,
    api: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CourseInput {
    title: String,
    #[serde(default)]
    theme: Option<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CourseManifest {
    protocol_version: u8,
    slug: String,
    title: String,
    theme: Option<Value>,
    publications: Vec<PublicationSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicationSummary {
    id: String,
    title: String,
    mode: PublicationMode,
    published_at: String,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum PublicationMode {
    Viewer,
    Editor,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogueInput {
    content: Value,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogueRef {
    id: String,
    version: i64,
    hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicationInput {
    title: String,
    #[serde(default = "viewer_mode")]
    mode: PublicationMode,
    document: Value,
    catalogues: Vec<CatalogueRef>,
    #[serde(default)]
    courses: Vec<String>,
}

fn viewer_mode() -> PublicationMode {
    PublicationMode::Viewer
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Publication {
    protocol_version: u8,
    id: String,
    title: String,
    mode: PublicationMode,
    document: Value,
    catalogues: Vec<CatalogueRef>,
    published_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedPublication {
    id: String,
    href: String,
}

#[derive(Deserialize)]
struct WorkspaceInput {
    title: String,
    document: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedWorkspace {
    id: String,
    edit_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDocument {
    id: String,
    title: String,
    document: Value,
    updated_at: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_url = env::var("JOVEWORKS_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://joveworks-hub.sqlite?mode=rwc".to_owned());
    let admin_token = env::var("JOVEWORKS_ADMIN_TOKEN")
        .expect("JOVEWORKS_ADMIN_TOKEN must be set; refusing to start with unprotected writes");
    let bind = env::var("JOVEWORKS_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let address: SocketAddr = bind
        .parse()
        .expect("JOVEWORKS_BIND must be a socket address such as 127.0.0.1:8080");
    let database = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    migrate(&database).await?;

    let state = AppState {
        database,
        admin_token: Arc::from(admin_token),
        course_token: env::var("JOVEWORKS_COURSE_TOKEN").ok().map(Arc::from),
    };
    let listener = TcpListener::bind(address).await?;
    info!(%address, "JoveWorks Hub listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/.well-known/joveworks", get(discovery))
        .route("/api/v1/courses/{slug}", get(get_course).post(put_course))
        .route(
            "/api/v1/catalogues/{id}/{version}",
            get(get_catalogue).post(put_catalogue),
        )
        .route("/api/v1/publications", post(create_publication))
        .route("/api/v1/publications/{id}", get(get_publication))
        .route("/api/v1/workspaces", post(create_workspace))
        .route(
            "/api/v1/workspaces/{id}",
            get(get_workspace).put(replace_workspace),
        )
        .route("/p/{id}", get(publication_link))
        .layer(TraceLayer::new_for_http())
        // Same-origin hosting is the normal deployment. This permissive layer
        // lets the standalone editor connect too; put Hub behind HTTPS.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods([Method::GET, Method::POST, Method::PUT]),
        )
        .with_state(state)
}

async fn migrate(database: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS joveworks_schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(database)
    .await?;

    let mut transaction = database.begin().await?;
    for &(version, name, sql) in MIGRATIONS {
        let applied = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM joveworks_schema_migrations WHERE version = ?)",
        )
        .bind(version)
        .fetch_one(&mut *transaction)
        .await?;
        if applied != 0 {
            continue;
        }

        sqlx::raw_sql(sql).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO joveworks_schema_migrations (version, name) VALUES (?, ?)")
            .bind(version)
            .bind(name)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(())
}

const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "initial_schema",
        include_str!("../migrations/0001_initial_schema.sql"),
    ),
    (
        2,
        "student_workspaces",
        include_str!("../migrations/0002_student_workspaces.sql"),
    ),
];

async fn healthz() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn discovery() -> Json<Discovery> {
    Json(Discovery {
        protocol_version: PROTOCOL_VERSION,
        api: "/api/v1",
    })
}

async fn put_course(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(input): Json<CourseInput>,
) -> Result<StatusCode, ApiError> {
    require_admin(&headers, &state)?;
    valid_name(&slug, "course slug")?;
    valid_name(&input.title, "course title")?;
    let theme = input.theme.map(json_text).transpose()?;
    sqlx::query(
        "INSERT INTO courses (slug, title, theme_json) VALUES (?, ?, ?)
         ON CONFLICT(slug) DO UPDATE SET title = excluded.title, theme_json = excluded.theme_json, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(slug)
    .bind(input.title)
    .bind(theme)
    .execute(&state.database)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_course(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<CourseManifest>, ApiError> {
    let course = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT title, theme_json FROM courses WHERE slug = ?",
    )
    .bind(&slug)
    .fetch_optional(&state.database)
    .await?
    .ok_or(ApiError::NotFound)?;
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT p.id, p.title, p.mode, p.published_at
         FROM publications p JOIN course_publications cp ON cp.publication_id = p.id
         WHERE cp.course_slug = ? ORDER BY p.published_at DESC",
    )
    .bind(&slug)
    .fetch_all(&state.database)
    .await?;
    let publications = rows
        .into_iter()
        .map(|(id, title, mode, published_at)| {
            Ok(PublicationSummary {
                id,
                title,
                mode: parse_mode(&mode)?,
                published_at,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Json(CourseManifest {
        protocol_version: PROTOCOL_VERSION,
        slug,
        title: course.0,
        theme: course.1.map(|text| parse_json(&text)).transpose()?,
        publications,
    }))
}

async fn put_catalogue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, version)): Path<(String, i64)>,
    Json(input): Json<CatalogueInput>,
) -> Result<Json<CatalogueRef>, ApiError> {
    require_admin(&headers, &state)?;
    valid_name(&id, "catalogue id")?;
    if version < 1 {
        return Err(ApiError::BadRequest(
            "catalogue version must be positive".into(),
        ));
    }
    let object = input
        .content
        .as_object()
        .ok_or_else(|| ApiError::BadRequest("catalogue content must be a JSON object".into()))?;
    if object.get("id").and_then(Value::as_str) != Some(&id) {
        return Err(ApiError::BadRequest(
            "catalogue content.id must match the URL".into(),
        ));
    }
    if object
        .get("schemaVersion")
        .and_then(Value::as_i64)
        .is_none()
    {
        return Err(ApiError::BadRequest(
            "catalogue content.schemaVersion is required".into(),
        ));
    }
    let restricted = object
        .get("restricted")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            ApiError::BadRequest("catalogue content.restricted must be true or false".into())
        })?;
    let content = json_text(input.content)?;
    let hash = sha256(&content);
    let result = sqlx::query(
        "INSERT INTO catalogues (id, version, hash, restricted, content_json) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(version)
    .bind(&hash)
    .bind(restricted)
    .bind(&content)
    .execute(&state.database)
    .await;
    match result {
        Ok(_) => Ok(Json(CatalogueRef { id, version, hash })),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => Err(ApiError::Conflict),
        Err(error) => Err(ApiError::Database(error)),
    }
}

async fn get_catalogue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, version)): Path<(String, i64)>,
) -> Result<Response, ApiError> {
    let (hash, restricted, content) = sqlx::query_as::<_, (String, bool, String)>(
        "SELECT hash, restricted, content_json FROM catalogues WHERE id = ? AND version = ?",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(&state.database)
    .await?
    .ok_or(ApiError::NotFound)?;
    if restricted {
        require_course_access(&headers, &state)?;
    }
    let mut response = Json(parse_json(&content)?).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&format!("\"{hash}\"")).expect("SHA-256 is a valid HTTP header"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=0"),
    );
    Ok(response)
}

async fn create_publication(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PublicationInput>,
) -> Result<(StatusCode, Json<CreatedPublication>), ApiError> {
    require_admin(&headers, &state)?;
    valid_name(&input.title, "publication title")?;
    validate_document(&input.document)?;
    if input.catalogues.is_empty() {
        return Err(ApiError::BadRequest(
            "a publication must pin at least one catalogue".into(),
        ));
    }
    for catalogue in &input.catalogues {
        let actual = sqlx::query_scalar::<_, String>(
            "SELECT hash FROM catalogues WHERE id = ? AND version = ?",
        )
        .bind(&catalogue.id)
        .bind(catalogue.version)
        .fetch_optional(&state.database)
        .await?
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "catalogue {} version {} is not stored",
                catalogue.id, catalogue.version
            ))
        })?;
        if actual != catalogue.hash {
            return Err(ApiError::BadRequest(format!(
                "catalogue {} version {} has a different hash",
                catalogue.id, catalogue.version
            )));
        }
    }
    let mut transaction = state.database.begin().await?;
    for course in &input.courses {
        valid_name(course, "course slug")?;
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM courses WHERE slug = ?")
            .bind(course)
            .fetch_optional(&mut *transaction)
            .await?
            .is_some();
        if !exists {
            return Err(ApiError::BadRequest(format!(
                "course '{course}' does not exist"
            )));
        }
    }
    let id = next_publication_id();
    sqlx::query("INSERT INTO publications (id, title, mode, document_json, catalogues_json) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&input.title)
        .bind(mode_name(input.mode))
        .bind(json_text(input.document)?)
        .bind(json_text(serde_json::to_value(&input.catalogues).expect("catalogue refs serialize"))?)
        .execute(&mut *transaction)
        .await?;
    for course in &input.courses {
        sqlx::query("INSERT INTO course_publications (course_slug, publication_id) VALUES (?, ?)")
            .bind(course)
            .bind(&id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedPublication {
            href: format!("/api/v1/publications/{id}"),
            id,
        }),
    ))
}

async fn get_publication(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Publication>, ApiError> {
    let row = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT title, mode, document_json, catalogues_json, published_at FROM publications WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.database)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(Publication {
        protocol_version: PROTOCOL_VERSION,
        id,
        title: row.0,
        mode: parse_mode(&row.1)?,
        document: parse_json(&row.2)?,
        catalogues: serde_json::from_str(&row.3).map_err(|_| {
            ApiError::Database(sqlx::Error::Protocol(
                "stored catalogue refs are invalid".into(),
            ))
        })?,
        published_at: row.4,
    }))
}

/// Create a mutable student-owned graph. The public workspace id is safe to
/// share for loading; only the separately returned edit token can replace it.
async fn create_workspace(
    State(state): State<AppState>,
    Json(input): Json<WorkspaceInput>,
) -> Result<(StatusCode, Json<CreatedWorkspace>), ApiError> {
    valid_name(&input.title, "workspace title")?;
    validate_document(&input.document)?;
    let document = json_text(input.document)?;

    // A collision is extremely unlikely, but the primary key remains the
    // authority and lets us safely retry rather than relying on probability.
    for _ in 0..3 {
        let id = next_workspace_id();
        let edit_token = next_workspace_token();
        let token_hash = sha256(&edit_token);
        let result = sqlx::query(
            "INSERT INTO workspaces (id, edit_token_hash, title, document_json) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&token_hash)
        .bind(&input.title)
        .bind(&document)
        .execute(&state.database)
        .await;
        match result {
            Ok(_) => {
                return Ok((
                    StatusCode::CREATED,
                    Json(CreatedWorkspace { id, edit_token }),
                ));
            }
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => continue,
            Err(error) => return Err(ApiError::Database(error)),
        }
    }
    Err(ApiError::Database(sqlx::Error::Protocol(
        "could not allocate a workspace id".into(),
    )))
}

async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceDocument>, ApiError> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT title, document_json, updated_at FROM workspaces WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.database)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(WorkspaceDocument {
        id,
        title: row.0,
        document: parse_json(&row.1)?,
        updated_at: row.2,
    }))
}

async fn replace_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<WorkspaceInput>,
) -> Result<Json<WorkspaceDocument>, ApiError> {
    valid_name(&input.title, "workspace title")?;
    validate_document(&input.document)?;
    let token = workspace_token(&headers)?;
    let document = json_text(input.document)?;
    let result = sqlx::query(
        "UPDATE workspaces
         SET title = ?, document_json = ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ? AND edit_token_hash = ?",
    )
    .bind(&input.title)
    .bind(&document)
    .bind(&id)
    .bind(sha256(token))
    .execute(&state.database)
    .await?;
    if result.rows_affected() == 0 {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM workspaces WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.database)
            .await?
            .is_some();
        return Err(if exists {
            ApiError::Unauthorized
        } else {
            ApiError::NotFound
        });
    }
    let updated_at = sqlx::query_scalar("SELECT updated_at FROM workspaces WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.database)
        .await?;
    Ok(Json(WorkspaceDocument {
        id,
        title: input.title,
        document: parse_json(&document)?,
        updated_at,
    }))
}

/// The human-facing, intentionally short publication URL. It will become the
/// NodeBook-viewer route once the editor consumes Hub's API. Until then, it
/// still resolves to the immutable publication JSON rather than an encoded
/// document URL.
async fn publication_link(Path(id): Path<String>) -> Redirect {
    Redirect::temporary(&format!("/api/v1/publications/{id}"))
}

fn require_admin(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let given = headers
        .get(ADMIN_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    if given == Some(state.admin_token.as_ref()) {
        Ok(())
    } else {
        Err(ApiError::Unauthorized)
    }
}

fn require_course_access(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let given = headers
        .get(COURSE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    match state.course_token.as_deref() {
        Some(expected) if given == Some(expected) => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
}

fn workspace_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(WORKSPACE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::Unauthorized)
}

fn valid_name(value: &str, field: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() || value.len() > 200 {
        Err(ApiError::BadRequest(format!(
            "{field} must be between 1 and 200 characters"
        )))
    } else {
        Ok(())
    }
}

fn validate_document(document: &Value) -> Result<(), ApiError> {
    let object = document
        .as_object()
        .ok_or_else(|| ApiError::BadRequest("document must be a JSON object".into()))?;
    if object
        .get("schemaVersion")
        .and_then(Value::as_i64)
        .is_none()
        || object.get("id").and_then(Value::as_str).is_none()
    {
        return Err(ApiError::BadRequest(
            "document must have schemaVersion and id".into(),
        ));
    }
    Ok(())
}

fn json_text(value: Value) -> Result<String, ApiError> {
    serde_json::to_string(&value).map_err(|error| ApiError::BadRequest(error.to_string()))
}
fn parse_json(text: &str) -> Result<Value, ApiError> {
    serde_json::from_str(text)
        .map_err(|_| ApiError::Database(sqlx::Error::Protocol("stored JSON is invalid".into())))
}
fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn next_publication_id() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(12)
        .map(char::from)
        .collect()
}
fn next_workspace_id() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(12)
        .map(char::from)
        .collect()
}
fn next_workspace_token() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}
fn mode_name(mode: PublicationMode) -> &'static str {
    match mode {
        PublicationMode::Viewer => "viewer",
        PublicationMode::Editor => "editor",
    }
}
fn parse_mode(mode: &str) -> Result<PublicationMode, ApiError> {
    match mode {
        "viewer" => Ok(PublicationMode::Viewer),
        "editor" => Ok(PublicationMode::Editor),
        _ => Err(ApiError::Database(sqlx::Error::Protocol(
            "stored publication mode is invalid".into(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn test_app() -> Router {
        let database = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&database).await.unwrap();
        app(AppState {
            database,
            admin_token: Arc::from("admin-test-token"),
            course_token: Some(Arc::from("course-test-token")),
        })
    }

    #[tokio::test]
    async fn migrations_apply_once_to_fresh_database() {
        let database = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        migrate(&database).await.unwrap();
        migrate(&database).await.unwrap();

        let applied: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM joveworks_schema_migrations WHERE version = 1",
        )
        .fetch_one(&database)
        .await
        .unwrap();
        assert_eq!(applied, 1);
        for table in [
            "catalogues",
            "courses",
            "publications",
            "course_publications",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
            )
            .bind(table)
            .fetch_one(&database)
            .await
            .unwrap();
            assert_eq!(exists, 1, "table {table} should exist");
        }
    }

    #[tokio::test]
    async fn migration_adopts_database_created_by_inline_setup() {
        let database = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"CREATE TABLE catalogues (
                id TEXT NOT NULL,
                version INTEGER NOT NULL,
                hash TEXT NOT NULL,
                restricted INTEGER NOT NULL,
                content_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (id, version)
            );
            INSERT INTO catalogues (id, version, hash, restricted, content_json)
            VALUES ('existing', 1, 'hash', 0, '{}');
            CREATE TABLE courses (
                slug TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                theme_json TEXT,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO courses (slug, title, theme_json)
            VALUES ('legacy-course', 'Legacy course', '{"theme":"blue"}');
            CREATE TABLE publications (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                mode TEXT NOT NULL,
                document_json TEXT NOT NULL,
                catalogues_json TEXT NOT NULL,
                published_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO publications (id, title, mode, document_json, catalogues_json)
            VALUES ('legacy-publication', 'Legacy publication', 'viewer', '{}',
                    '[{"id":"existing","version":1,"hash":"hash"}]');
            CREATE TABLE course_publications (
                course_slug TEXT NOT NULL REFERENCES courses(slug),
                publication_id TEXT NOT NULL REFERENCES publications(id),
                PRIMARY KEY (course_slug, publication_id)
            );
            INSERT INTO course_publications (course_slug, publication_id)
            VALUES ('legacy-course', 'legacy-publication');"#,
        )
        .execute(&database)
        .await
        .unwrap();

        migrate(&database).await.unwrap();

        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM catalogues WHERE id = 'existing' AND version = 1",
        )
        .fetch_one(&database)
        .await
        .unwrap();
        assert_eq!(existing, 1);

        let preserved_course: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM courses WHERE slug = 'legacy-course' AND title = 'Legacy course'",
        )
        .fetch_one(&database)
        .await
        .unwrap();
        assert_eq!(preserved_course, 1);

        let preserved_publication: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM publications WHERE id = 'legacy-publication' AND title = 'Legacy publication'",
        )
        .fetch_one(&database)
        .await
        .unwrap();
        assert_eq!(preserved_publication, 1);

        let preserved_link: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM course_publications
             WHERE course_slug = 'legacy-course' AND publication_id = 'legacy-publication'",
        )
        .fetch_one(&database)
        .await
        .unwrap();
        assert_eq!(preserved_link, 1);
    }

    fn json_request(uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header(ADMIN_TOKEN_HEADER, "admin-test-token")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn json_body(response: Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn public_catalogue_is_immutable_and_retrievable() {
        let app = test_app().await;
        let catalogue = json!({
            "schemaVersion": 1,
            "id": "public-example",
            "name": "Public example",
            "restricted": false,
            "formulas": []
        });
        let response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/catalogues/public-example/1",
                json!({ "content": catalogue }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let reference = json_body(response).await;
        assert!(reference["hash"].as_str().is_some());

        let response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/courses/machine-design-2026",
                json!({ "title": "Machine design 2026" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/publications",
                json!({
                    "title": "A published NodeBook",
                    "document": { "schemaVersion": 1, "id": "published-nodebook" },
                    "catalogues": [reference],
                    "courses": ["machine-design-2026"]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let publication_id = json_body(response).await["id"].as_str().unwrap().to_owned();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/publications/{publication_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["document"]["id"],
            "published-nodebook"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/p/{publication_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[header::LOCATION],
            format!("/api/v1/publications/{publication_id}")
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/catalogues/public-example/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["id"], "public-example");

        let response = app
            .oneshot(json_request("/api/v1/catalogues/public-example/1", json!({ "content": { "schemaVersion": 1, "id": "public-example", "restricted": false, "formulas": [] } })))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_workspace_loads_publicly_but_requires_its_edit_token_to_save() {
        let app = test_app().await;
        let document = json!({ "schemaVersion": 1, "id": "student-belt-study" });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/workspaces")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "title": "Belt study", "document": document }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let created = json_body(response).await;
        let id = created["id"].as_str().unwrap();
        let token = created["editToken"].as_str().unwrap();
        assert_eq!(id.len(), 12);
        assert_eq!(token.len(), 32);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/workspaces/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["title"], "Belt study");

        let replacement = json!({ "schemaVersion": 1, "id": "student-belt-study-v2" });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/workspaces/{id}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "title": "Belt study v2", "document": replacement }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/workspaces/{id}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(WORKSPACE_TOKEN_HEADER, token)
                    .body(Body::from(
                        json!({ "title": "Belt study v2", "document": replacement }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let saved = json_body(response).await;
        assert_eq!(saved["title"], "Belt study v2");
        assert_eq!(saved["document"]["id"], "student-belt-study-v2");
        assert!(saved.get("editToken").is_none());
    }

    #[tokio::test]
    async fn restricted_catalogue_requires_the_course_token() {
        let app = test_app().await;
        let catalogue = json!({
            "schemaVersion": 1,
            "id": "restricted-example",
            "name": "Restricted example",
            "restricted": true,
            "formulas": []
        });
        let response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/catalogues/restricted-example/1",
                json!({ "content": catalogue }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/catalogues/restricted-example/1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/catalogues/restricted-example/1")
                    .header(COURSE_TOKEN_HEADER, "course-test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
