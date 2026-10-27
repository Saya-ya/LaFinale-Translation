use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Html,
    routing::get,
    Json, Router,
};
use lafinale_core::{
    exe_patch::{self, PatchError},
    workspace::{
        BuildError, ImportError, PakBuildError, Workspace, DIRECTORIES, PAGE_PROBE_OUTPUT,
    },
};
use serde::Serialize;
use std::{net::SocketAddr, path::PathBuf};

const PAGE: &str = include_str!("page.html");
const CSS: &str = include_str!("style.css");
const JS: &str = include_str!("app.js");

#[derive(Serialize)]
struct DirectoryStatus {
    path: &'static str,
    exists: bool,
}

#[derive(Serialize)]
struct InputStatus {
    script_pak_present: bool,
    exe_present: bool,
    validated: bool,
}

#[derive(Serialize)]
struct ApiStatus {
    directories: Vec<DirectoryStatus>,
    inputs: InputStatus,
    workspace: WorkspaceOverview,
}

#[derive(Serialize)]
struct WorkspaceOverview {
    translation_json_count: Option<usize>,
    filled_spanish_count: Option<usize>,
    diagnostic_exe_present: bool,
    output_pak_present: bool,
}

#[derive(Serialize)]
struct ExeResult {
    output: &'static str,
    sha256: &'static str,
    diagnostic_only: bool,
    note: &'static str,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Serialize)]
struct ImportResult {
    scripts: usize,
    strings: usize,
    destination: &'static str,
}

#[derive(Serialize)]
struct PakResult {
    output: &'static str,
    sha256: String,
    translated_strings: usize,
    wrapped_strings: usize,
    normalized_characters: usize,
    maximum_rows: usize,
    diagnostic_only: bool,
}

#[derive(Serialize)]
struct PageProbeResult {
    output: &'static str,
    sha256: String,
    script: &'static str,
    string_index: usize,
    before: &'static str,
    after: &'static str,
    diagnostic_only: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // From the repository root by default; a launcher can set LAFINALE_HOME.
    // No original file is opened, written, copied or patched on startup.
    let root = std::env::var_os("LAFINALE_HOME")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let workspace = Workspace::initialize(root)?;
    let app = Router::new()
        .route("/", get(index))
        .route("/importar", get(index))
        .route("/parchear", get(index))
        .route("/assets/app.css", get(styles))
        .route("/assets/app.js", get(script))
        .route("/health", get(|| async { "ok" }))
        .route("/api/health", get(|| async { Json("ok") }))
        .route("/api/status", get(api_status))
        .route("/api/exe/build", axum::routing::post(build_exe))
        .route("/api/import", axum::routing::post(import_pak))
        .route("/api/pak/build", axum::routing::post(build_pak))
        .route("/api/pak/page-probe", axum::routing::post(page_probe))
        .with_state(workspace);
    let address: SocketAddr = "127.0.0.1:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Web local: http://{address} (GET /api/status)");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn build_exe(
    State(workspace): State<Workspace>,
    headers: HeaderMap,
) -> Result<Json<ExeResult>, (StatusCode, Json<ApiError>)> {
    local_write_only(&headers)?;
    let result = tokio::task::spawn_blocking(move || workspace.build_exe_diagnostic())
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    match result {
        Ok(_) => Ok(Json(ExeResult {
            output: "output/SolfegeLF_DIAGNOSTIC.exe",
            sha256: exe_patch::DIAGNOSTIC_SHA256,
            diagnostic_only: true,
            note: "52 bytes CP932/slot; ancho visual, menú y paginación pendientes de validar",
        })),
        Err(error) => {
            let code = match &error {
                BuildError::MissingInput => StatusCode::NOT_FOUND,
                BuildError::OutputExists => StatusCode::CONFLICT,
                BuildError::UnsafeInput
                | BuildError::Patch(PatchError::UnsupportedExe(_))
                | BuildError::Patch(PatchError::InvalidPe(_))
                | BuildError::Patch(PatchError::Mismatch(_)) => StatusCode::UNPROCESSABLE_ENTITY,
                BuildError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(api_error(code, error.to_string()))
        }
    }
}

async fn import_pak(
    State(workspace): State<Workspace>,
    headers: HeaderMap,
) -> Result<Json<ImportResult>, (StatusCode, Json<ApiError>)> {
    local_write_only(&headers)?;
    let result = tokio::task::spawn_blocking(move || workspace.import_translations())
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    match result {
        Ok(summary) => Ok(Json(ImportResult {
            scripts: summary.scripts,
            strings: summary.strings,
            destination: "work/translations/",
        })),
        Err(error) => {
            let code = match &error {
                ImportError::MissingInput => StatusCode::NOT_FOUND,
                ImportError::Occupied => StatusCode::CONFLICT,
                ImportError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
                ImportError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(api_error(code, error.to_string()))
        }
    }
}

async fn build_pak(
    State(workspace): State<Workspace>,
    headers: HeaderMap,
) -> Result<Json<PakResult>, (StatusCode, Json<ApiError>)> {
    local_write_only(&headers)?;
    let result = tokio::task::spawn_blocking(move || workspace.build_pak_diagnostic())
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    match result {
        Ok(summary) => Ok(Json(PakResult {
            output: "output/Script.pak",
            sha256: summary.sha256,
            translated_strings: summary.report.translated_strings,
            wrapped_strings: summary.report.wrapped_strings,
            normalized_characters: summary.report.normalized_characters,
            maximum_rows: summary.report.maximum_rows,
            diagnostic_only: true,
        })),
        Err(error) => {
            let code = match &error {
                PakBuildError::MissingInput | PakBuildError::ProbeMissing => StatusCode::NOT_FOUND,
                PakBuildError::OutputExists | PakBuildError::ProbeExists => StatusCode::CONFLICT,
                PakBuildError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
                PakBuildError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(api_error(code, error.to_string()))
        }
    }
}

async fn page_probe(
    State(workspace): State<Workspace>,
    headers: HeaderMap,
) -> Result<Json<PageProbeResult>, (StatusCode, Json<ApiError>)> {
    local_write_only(&headers)?;
    let result = tokio::task::spawn_blocking(move || workspace.build_page_probe())
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    match result {
        Ok(sha256) => Ok(Json(PageProbeResult {
            output: PAGE_PROBE_OUTPUT,
            sha256,
            script: "scr00001.kgo",
            string_index: 0,
            before: lafinale_core::pak_probe::FIRST_BEFORE,
            after: lafinale_core::pak_probe::FIRST_AFTER,
            diagnostic_only: true,
        })),
        Err(error) => {
            let code = match &error {
                PakBuildError::MissingInput | PakBuildError::ProbeMissing => StatusCode::NOT_FOUND,
                PakBuildError::OutputExists | PakBuildError::ProbeExists => StatusCode::CONFLICT,
                PakBuildError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
                PakBuildError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err(api_error(code, error.to_string()))
        }
    }
}

fn local_write_only(headers: &HeaderMap) -> Result<(), (StatusCode, Json<ApiError>)> {
    // A page on another origin must not trigger a write via a simple HTML form.
    if headers.get("host").and_then(|v| v.to_str().ok()) != Some("127.0.0.1:8080")
        || headers
            .get("origin")
            .is_some_and(|v| v != "http://127.0.0.1:8080")
        || headers
            .get("sec-fetch-site")
            .is_some_and(|v| v == "cross-site")
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Solo se aceptan solicitudes locales del mismo origen".into(),
        ));
    }
    Ok(())
}

fn api_error(code: StatusCode, error: String) -> (StatusCode, Json<ApiError>) {
    (code, Json(ApiError { error }))
}

async fn api_status(State(workspace): State<Workspace>) -> Result<Json<ApiStatus>, StatusCode> {
    let status = tokio::task::spawn_blocking(move || workspace.status())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiStatus {
        directories: DIRECTORIES
            .into_iter()
            .zip(status.directories)
            .map(|(path, exists)| DirectoryStatus { path, exists })
            .collect(),
        inputs: InputStatus {
            script_pak_present: status.has_script_pak,
            exe_present: status.has_exe,
            validated: false, // Detection is not a PE/HyPack signature or hash check.
        },
        workspace: WorkspaceOverview {
            translation_json_count: status.translation_json_count,
            filled_spanish_count: status.filled_spanish_count,
            diagnostic_exe_present: status.diagnostic_exe_present,
            output_pak_present: status.output_pak_present,
        },
    }))
}

async fn index() -> Html<&'static str> {
    Html(PAGE)
}

async fn styles() -> ([(header::HeaderName, &'static str); 1], &'static str) {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS)
}

async fn script() -> ([(header::HeaderName, &'static str); 1], &'static str) {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        JS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_views_and_assets_are_embedded() {
        assert!(PAGE.contains("href=\"/importar\""));
        assert!(PAGE.contains("href=\"/parchear\""));
        assert!(PAGE.contains("/assets/app.js"));
        assert!(JS.contains("/api/exe/build"));
        assert!(JS.contains("/api/pak/build"));
    }
}
