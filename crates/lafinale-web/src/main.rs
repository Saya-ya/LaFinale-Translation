use axum::{
    extract::{Form, State},
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::get,
    Json, Router,
};
use lafinale_core::{
    check_capacity,
    exe_patch::{self, PatchError},
    workspace::{BuildError, Workspace, DIRECTORIES},
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

const PAGE: &str = include_str!("page.html");

#[derive(Deserialize)]
struct PreviewForm {
    text: String,
}

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // From the repository root by default; a launcher can set LAFINALE_HOME.
    // No original file is opened, written, copied or patched on startup.
    let root = std::env::var_os("LAFINALE_HOME")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let workspace = Workspace::initialize(root)?;
    let app = Router::new()
        .route("/", get(index).post(preview))
        .route("/health", get(|| async { "ok" }))
        .route("/api/health", get(|| async { Json("ok") }))
        .route("/api/status", get(api_status))
        .route("/api/exe/build", axum::routing::post(build_exe))
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

fn api_error(code: StatusCode, error: String) -> (StatusCode, Json<ApiError>) {
    (code, Json(ApiError { error }))
}

async fn api_status(State(workspace): State<Workspace>) -> Json<ApiStatus> {
    let status = workspace.status();
    Json(ApiStatus {
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
    })
}

async fn index() -> Html<String> {
    render("", "Pega una frase para comprobar su capacidad CP932.")
}

async fn preview(Form(form): Form<PreviewForm>) -> Html<String> {
    let result = match check_capacity(&form.text) {
        Ok(capacity) => format!(
            "{} bytes CP932; tramo de prueba: {}/{}; slot físico: {}/{}. {}",
            capacity.encoded_bytes,
            capacity.encoded_bytes,
            capacity.preview_limit_bytes,
            capacity.encoded_bytes,
            capacity.slot_payload_bytes,
            if capacity.fits_preview() {
                "Cabe en un tramo de prueba; aún requiere revisión visual."
            } else if capacity.fits_slot() {
                "Supera el margen de 48 bytes; puede necesitar un salto."
            } else {
                "Excede un slot: NO se debe emitir como una sola línea."
            }
        ),
        Err(error) => error.to_string(),
    };
    render(&form.text, &result)
}

fn render(input: &str, result: &str) -> Html<String> {
    Html(
        PAGE.replace("{{TEXT}}", &escape_html(input))
            .replace("{{RESULT}}", &escape_html(result)),
    )
}

fn escape_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        result.push_str(match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                result.push(c);
                continue;
            }
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_html_is_escaped() {
        let page = render("<script>alert('x')</script>", "<unsafe>");
        assert!(page.0.contains("&lt;script&gt;"));
        assert!(!page.0.contains("<script>"));
        assert!(page.0.contains("&lt;unsafe&gt;"));
    }
}
