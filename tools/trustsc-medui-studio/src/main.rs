//! `trustsc-medui-studio` — the hosted MedUI Studio server (ADR-022, epic #9 wave S6).
//!
//! Host tooling only: this binary is a workspace member under `tools/` (ADR-005 trust zones)
//! and is never linked into any `crates/` or `adapters/` code that ships to a device. It serves
//! a browser frontend and a small JSON API over an on-disk `.medui` repository checkout.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

/// The frontend's built assets, embedded into the binary so the server has no runtime file
/// dependency. For this wave it is a single placeholder `index.html` (real assets land in S9).
static FRONTEND_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");

#[derive(Debug, Parser)]
#[command(name = "trustsc-medui-studio", version)]
struct Cli {
    /// Checkout containing `.medui` files to serve.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
}

struct AppState {
    repo: PathBuf,
    /// Shared bearer token from `TRUSTSC_STUDIO_TOKEN` (ADR-022 v1 auth). `None` means every
    /// `/api/*` request is accepted unauthenticated — TLS/SSO is delegated to a reverse proxy
    /// (see the crate README), this is not meant to face the open internet directly.
    token: Option<String>,
}

fn build_router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/screens", get(list_screens))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_bearer_token));

    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .fallback(serve_frontend)
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let token = std::env::var("TRUSTSC_STUDIO_TOKEN").ok().filter(|value| !value.is_empty());
    let state = Arc::new(AppState { repo: cli.repo, token });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(cli.listen)
        .await
        .unwrap_or_else(|error| panic!("failed to bind {}: {error}", cli.listen));
    println!("trustsc-medui-studio listening on http://{}", cli.listen);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server failed");
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn healthz() -> impl IntoResponse {
    format!("trustsc-medui-studio {}", env!("CARGO_PKG_VERSION"))
}

async fn require_bearer_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    if let Some(expected) = &state.token {
        let provided = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if provided != Some(expected.as_str()) {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    next.run(request).await
}

#[derive(Debug, Serialize, Deserialize)]
struct ScreenEntry {
    id: String,
    path: String,
    screen_name: String,
}

async fn list_screens(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut entries = find_medui_files(&state.repo)
        .into_iter()
        .map(|absolute| {
            let relative = absolute
                .strip_prefix(&state.repo)
                .unwrap_or(&absolute)
                .to_string_lossy()
                .replace('\\', "/");
            let screen_name = std::fs::read_to_string(&absolute)
                .ok()
                .and_then(|source| trustsc_ui_dsl_authoring::parse_medui_source(&source).ok())
                .map(|screen| screen.id)
                .unwrap_or_else(|| "<unparsed>".to_string());
            ScreenEntry {
                id: relative.clone(),
                path: relative,
                screen_name,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Json(entries)
}

/// Recursively collects every `*.medui` file under `root`, skipping `target/` and dot
/// directories (`.git/` and similar) so a full repo checkout scans quickly.
fn find_medui_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            files.extend(find_medui_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "medui") {
            files.push(path);
        }
    }
    files
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

async fn serve_frontend(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match FRONTEND_DIST.get_file(path) {
        Some(file) => (
            [(header::CONTENT_TYPE, content_type_for(path))],
            Body::from(file.contents().to_vec()),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request as HttpRequest;
    use std::fs;
    use tower::ServiceExt;

    fn state_for_repo(repo: PathBuf, token: Option<&str>) -> Arc<AppState> {
        Arc::new(AppState {
            repo,
            token: token.map(str::to_string),
        })
    }

    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR is tools/trustsc-medui-studio; the repo root (with examples/) is
        // two levels up.
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[tokio::test]
    async fn healthz_returns_200_with_a_version_string() {
        let app = build_router(state_for_repo(repo_root(), None));
        let response = app
            .oneshot(HttpRequest::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).starts_with("trustsc-medui-studio "));
    }

    #[tokio::test]
    async fn root_serves_the_placeholder_frontend() {
        let app = build_router(state_for_repo(repo_root(), None));
        let response = app
            .oneshot(HttpRequest::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("TrustSC MedUI Studio"));
    }

    #[tokio::test]
    async fn api_screens_lists_the_example_medui_files_without_a_token() {
        let app = build_router(state_for_repo(repo_root(), None));
        let response = app
            .oneshot(HttpRequest::get("/api/screens").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let entries: Vec<ScreenEntry> = serde_json::from_slice(&body).unwrap();
        assert!(entries.iter().any(|entry| entry.path.ends_with("hello_world.medui")
            && entry.screen_name == "HelloWorld"));
        assert!(entries.iter().any(|entry| entry.path.ends_with("neurosense.medui")
            && entry.screen_name == "NeuroSense500"));
    }

    #[tokio::test]
    async fn api_screens_rejects_missing_or_wrong_bearer_token_when_configured() {
        let app = build_router(state_for_repo(repo_root(), Some("secret")));

        let unauthenticated = app
            .clone()
            .oneshot(HttpRequest::get("/api/screens").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let wrong_token = app
            .clone()
            .oneshot(
                HttpRequest::get("/api/screens")
                    .header(header::AUTHORIZATION, "Bearer nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong_token.status(), StatusCode::UNAUTHORIZED);

        let correct_token = app
            .oneshot(
                HttpRequest::get("/api/screens")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(correct_token.status(), StatusCode::OK);
    }

    #[test]
    fn find_medui_files_skips_target_and_dot_directories() {
        let temp = std::env::temp_dir().join(format!(
            "trustsc-medui-studio-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("target")).unwrap();
        fs::create_dir_all(temp.join(".git")).unwrap();
        fs::create_dir_all(temp.join("screens")).unwrap();
        fs::write(temp.join("target/ignored.medui"), "").unwrap();
        fs::write(temp.join(".git/ignored.medui"), "").unwrap();
        fs::write(temp.join("screens/kept.medui"), "").unwrap();

        let found = find_medui_files(&temp);
        assert_eq!(found, vec![temp.join("screens/kept.medui")]);

        fs::remove_dir_all(&temp).unwrap();
    }
}
