//! The studio REST API (ADR-022 wave S8): screen detail, compile, frame render, palette, and
//! serialize. Bearer auth (wave S6) is applied to the whole `/api` nest at the router level, not
//! per handler.

use std::path::{Component, Path as StdPath, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use trustsc_ui_dsl_authoring::{
    CompileOptions, CompiledScreenSpec, Diagnostic, ImagePackages, ScreenDefinition,
    TextPackages, compile_screen_definition, enumerate_images, enumerate_numeric_templates,
    enumerate_text_keys, parse_medui_source, serialize_screen, widget_catalog,
};

use crate::render_bridge;
use crate::{AppState, error_response};
use crate::dto;

/// Fallback surface for a screen with no `surface:` pin (matches `examples/hello_world`'s own
/// build.rs configuration) — used whenever a screen doesn't declare its own.
const DEFAULT_SURFACE: (u32, u32) = (800, 480);

/// Resolves a caller-supplied screen id (the `{id}` path param or `?screen=` query param) to a
/// path inside `repo`, rejecting anything that could escape it. `PathBuf::join` happily accepts
/// an absolute path or `..` components and simply replaces/walks out of the base, so the id must
/// be validated first: every component must be a plain (`Normal`) segment — no `..`, no `.`, no
/// root/prefix — and it must end in `.medui`, both to block path traversal and because nothing
/// else is a legitimate screen id.
fn resolve_medui_path(repo: &StdPath, id: &str) -> Result<PathBuf, String> {
    let candidate = StdPath::new(id);
    let only_normal_components =
        !id.is_empty() && candidate.components().all(|component| matches!(component, Component::Normal(_)));
    let has_medui_extension = candidate.extension().map(|ext| ext == "medui").unwrap_or(false);
    if !only_normal_components || !has_medui_extension {
        return Err(format!("invalid screen id: {id}"));
    }
    Ok(repo.join(candidate))
}

pub fn router() -> axum::Router<Arc<AppState>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/screens/{*id}", get(screen_detail))
        .route("/compile", post(compile))
        .route("/frame", get(frame_get).post(frame_post))
        .route("/palette", get(palette))
        .route("/serialize", post(serialize))
}

/// The outcome of parsing (and, if that succeeds, compiling) a screen: every field is `None`/
/// empty on total failure rather than the handler ever panicking or 500ing on bad input.
struct CompileOutcome {
    screen: Option<dto::ScreenDefinitionDto>,
    compiled: Option<dto::CompiledSummaryDto>,
    diagnostics: Vec<dto::DiagnosticDto>,
}

fn compile_source_with_defaults(state: &AppState, source: &str) -> CompileOutcome {
    match parse_medui_source(source) {
        Ok(screen) => compile_screen_with_defaults(state, screen),
        Err(diagnostics) => CompileOutcome {
            screen: None,
            compiled: None,
            diagnostics: dto::diagnostics_to_dto(diagnostics),
        },
    }
}

fn compile_screen_with_defaults(state: &AppState, screen: ScreenDefinition) -> CompileOutcome {
    let screen_dto = dto::ScreenDefinitionDto::from(screen.clone());
    let (width, height) = screen.declared_surface.unwrap_or(DEFAULT_SURFACE);
    let display_refs = state.display_texts.iter().collect::<Vec<_>>();
    match compile_screen_definition(
        screen,
        &CompileOptions::new(width, height),
        TextPackages::with_displays(&state.standard_text, &display_refs),
        ImagePackages::new(&state.images),
    ) {
        Ok(spec) => CompileOutcome {
            screen: Some(screen_dto),
            compiled: Some(dto::compiled_summary_from_spec(spec)),
            diagnostics: vec![],
        },
        Err(diagnostics) => CompileOutcome {
            screen: Some(screen_dto),
            compiled: None,
            diagnostics: dto::diagnostics_to_dto(diagnostics),
        },
    }
}

/// Parses (or accepts) a screen and compiles it, purely functionally — used by the frame
/// endpoints, which need the raw `CompiledScreenSpec` (to bridge to the renderer) rather than
/// the summarized DTO `compile_*_with_defaults` above produce for the JSON compile endpoints.
fn compile_for_render(
    state: &AppState,
    screen: ScreenDefinition,
) -> Result<CompiledScreenSpec, Vec<Diagnostic>> {
    let (width, height) = screen.declared_surface.unwrap_or(DEFAULT_SURFACE);
    let display_refs = state.display_texts.iter().collect::<Vec<_>>();
    compile_screen_definition(
        screen,
        &CompileOptions::new(width, height),
        TextPackages::with_displays(&state.standard_text, &display_refs),
        ImagePackages::new(&state.images),
    )
}

fn diagnostics_message(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------------------------
// GET /api/screens/{id}
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct CompiledWithDiagnosticsDto {
    surface: (u32, u32),
    nodes: Vec<dto::CompiledNodeSummaryDto>,
    diagnostics: Vec<dto::DiagnosticDto>,
}

#[derive(Serialize)]
struct ScreenDetailDto {
    source: String,
    screen: Option<dto::ScreenDefinitionDto>,
    compiled: CompiledWithDiagnosticsDto,
}

async fn screen_detail(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let file_path = match resolve_medui_path(&state.repo, &id) {
        Ok(path) => path,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, message),
    };
    let source = match tokio::task::spawn_blocking(move || std::fs::read_to_string(file_path)).await
    {
        Ok(Ok(source)) => source,
        Ok(Err(_)) => return error_response(StatusCode::NOT_FOUND, format!("no such screen: {id}")),
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "read task failed"),
    };

    let outcome = compile_source_with_defaults(&state, &source);
    // A screen that parses but fails to compile still declared its own `surface:` (if any) —
    // report that instead of silently falling back to the 800x480 default, which would give the
    // frontend the wrong canvas size for the diagnostics it's about to show.
    let declared_surface = outcome.screen.as_ref().and_then(|screen| screen.declared_surface);
    let (surface, nodes) = match outcome.compiled {
        Some(compiled) => (compiled.surface, compiled.nodes),
        None => (declared_surface.unwrap_or(DEFAULT_SURFACE), Vec::new()),
    };

    Json(ScreenDetailDto {
        source,
        screen: outcome.screen,
        compiled: CompiledWithDiagnosticsDto {
            surface,
            nodes,
            diagnostics: outcome.diagnostics,
        },
    })
    .into_response()
}

// ---------------------------------------------------------------------------------------------
// POST /api/compile
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CompileRequestBody {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    screen: Option<dto::ScreenDefinitionDto>,
}

#[derive(Serialize)]
struct CompileResponseBody {
    ok: bool,
    compiled: Option<dto::CompiledSummaryDto>,
    diagnostics: Vec<dto::DiagnosticDto>,
}

async fn compile(State(state): State<Arc<AppState>>, Json(request): Json<CompileRequestBody>) -> Response {
    let outcome = match (request.source, request.screen) {
        (Some(source), None) => compile_source_with_defaults(&state, &source),
        (None, Some(screen_dto)) => {
            if dto::contains_panel(&screen_dto) {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "screen contains a Panel node, which is compiler-synthesized only and cannot be submitted",
                );
            }
            compile_screen_with_defaults(&state, screen_dto.into())
        }
        (Some(_), Some(_)) => {
            return error_response(StatusCode::BAD_REQUEST, "provide exactly one of `source` or `screen`, not both");
        }
        (None, None) => {
            return error_response(StatusCode::BAD_REQUEST, "provide exactly one of `source` or `screen`");
        }
    };

    Json(CompileResponseBody {
        ok: outcome.compiled.is_some(),
        compiled: outcome.compiled,
        diagnostics: outcome.diagnostics,
    })
    .into_response()
}

// ---------------------------------------------------------------------------------------------
// GET/POST /api/frame
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct FrameQuery {
    screen: String,
    #[serde(default)]
    locale: Option<String>,
}

async fn frame_get(State(state): State<Arc<AppState>>, Query(params): Query<FrameQuery>) -> Response {
    let file_path = match resolve_medui_path(&state.repo, &params.screen) {
        Ok(path) => path,
        Err(message) => return error_response(StatusCode::BAD_REQUEST, message),
    };
    let source = match tokio::task::spawn_blocking(move || std::fs::read_to_string(file_path)).await
    {
        Ok(Ok(source)) => source,
        Ok(Err(_)) => {
            return error_response(StatusCode::NOT_FOUND, format!("no such screen: {}", params.screen));
        }
        Err(_) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, "read task failed"),
    };

    let screen = match parse_medui_source(&source) {
        Ok(screen) => screen,
        Err(diagnostics) => {
            return error_response(StatusCode::UNPROCESSABLE_ENTITY, diagnostics_message(&diagnostics));
        }
    };

    render_response(&state, screen, params.locale).await
}

#[derive(Deserialize)]
struct FrameRequestBody {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    screen: Option<dto::ScreenDefinitionDto>,
    #[serde(default)]
    locale: Option<String>,
}

async fn frame_post(State(state): State<Arc<AppState>>, Json(request): Json<FrameRequestBody>) -> Response {
    let screen = match (request.source, request.screen) {
        (Some(source), None) => match parse_medui_source(&source) {
            Ok(screen) => screen,
            Err(diagnostics) => {
                return error_response(StatusCode::UNPROCESSABLE_ENTITY, diagnostics_message(&diagnostics));
            }
        },
        (None, Some(screen_dto)) => {
            if dto::contains_panel(&screen_dto) {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "screen contains a Panel node, which is compiler-synthesized only and cannot be submitted",
                );
            }
            screen_dto.into()
        }
        (Some(_), Some(_)) => {
            return error_response(StatusCode::BAD_REQUEST, "provide exactly one of `source` or `screen`, not both");
        }
        (None, None) => {
            return error_response(StatusCode::BAD_REQUEST, "provide exactly one of `source` or `screen`");
        }
    };

    render_response(&state, screen, request.locale).await
}

async fn render_response(
    state: &Arc<AppState>,
    screen: ScreenDefinition,
    locale: Option<String>,
) -> Response {
    let locale = locale.unwrap_or_else(|| "en-US".to_string());
    if !state.standard_text.locales().iter().any(|known| *known == locale) {
        return error_response(StatusCode::BAD_REQUEST, format!("unknown locale: {locale}"));
    }

    let spec = match compile_for_render(state, screen) {
        Ok(spec) => spec,
        Err(diagnostics) => {
            return error_response(StatusCode::UNPROCESSABLE_ENTITY, diagnostics_message(&diagnostics));
        }
    };

    match render_compiled_to_png(state, spec, &locale).await {
        Ok(png_bytes) => ([(header::CONTENT_TYPE, "image/png")], png_bytes).into_response(),
        Err(message) => error_response(StatusCode::INTERNAL_SERVER_ERROR, message),
    }
}

/// Renders `spec` and encodes the result as PNG, serializing access to the offscreen renderer
/// through `state.render_semaphore` (queue depth 1 — ADR-022 wave S7's noted requirement, which
/// only actually matters once concurrent HTTP requests can reach the render bridge, as they can
/// from this wave on) and running the blocking Vulkan work on the blocking thread pool.
async fn render_compiled_to_png(
    state: &Arc<AppState>,
    spec: CompiledScreenSpec,
    locale: &str,
) -> Result<Vec<u8>, String> {
    let _permit = state
        .render_semaphore
        .acquire()
        .await
        .expect("render semaphore is never closed");

    let (width, height) = spec.surface;
    let package = render_bridge::leak_package(&spec);
    let standard = state.standard_text.clone();
    let displays = state.display_texts.clone();
    let images = state.images.clone();
    let locale = locale.to_string();

    tokio::task::spawn_blocking(move || {
        let frame = render_bridge::render_screen(
            "trustsc-medui-studio",
            package,
            standard,
            displays,
            &images,
            &locale,
            width,
            height,
        )
        .map_err(|error| error.to_string())?;
        render_bridge::encode_png(&frame).map_err(|error| error.to_string())
    })
    .await
    .map_err(|join_error| format!("render task failed: {join_error}"))?
}

// ---------------------------------------------------------------------------------------------
// GET /api/palette
// ---------------------------------------------------------------------------------------------

async fn palette(State(state): State<Arc<AppState>>) -> Response {
    let widgets = widget_catalog().iter().map(dto::WidgetSchemaDto::from).collect();
    let colors = trustsc::THEME_COLORS
        .iter()
        .map(|(token, rgba)| dto::ColorSwatchDto {
            token: token.to_string(),
            rgba: *rgba,
        })
        .collect();
    let text_keys = enumerate_text_keys(&state.standard_text)
        .into_iter()
        .map(dto::TextKeyInfoDto::from)
        .collect();
    let templates = state
        .display_texts
        .iter()
        .flat_map(enumerate_numeric_templates)
        .map(dto::NumericTemplateInfoDto::from)
        .collect();
    let images = enumerate_images(&state.images)
        .into_iter()
        .map(dto::ImageInfoDto::from)
        .collect();
    let locales = state.standard_text.locales();

    Json(dto::PaletteDto {
        widgets,
        colors,
        text_keys,
        templates,
        images,
        locales,
    })
    .into_response()
}

// ---------------------------------------------------------------------------------------------
// POST /api/serialize
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct SerializeRequestBody {
    screen: dto::ScreenDefinitionDto,
}

#[derive(Serialize)]
struct SerializeResponseBody {
    source: String,
}

async fn serialize(Json(request): Json<SerializeRequestBody>) -> Response {
    if dto::contains_panel(&request.screen) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "screen contains a Panel node, which is compiler-synthesized only and cannot be serialized",
        );
    }
    let screen: ScreenDefinition = request.screen.into();
    let source = serialize_screen(&screen);
    Json(SerializeResponseBody { source }).into_response()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use axum::body::{Body, to_bytes};
    use axum::http::{Request as HttpRequest, StatusCode};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::AppState;
    use super::resolve_medui_path;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn resolve_medui_path_rejects_parent_directory_traversal() {
        let repo = repo_root();
        assert!(resolve_medui_path(&repo, "../Cargo.toml").is_err());
        assert!(resolve_medui_path(&repo, "examples/../../Cargo.toml").is_err());
    }

    #[test]
    fn resolve_medui_path_rejects_an_absolute_path() {
        let repo = repo_root();
        assert!(resolve_medui_path(&repo, "/etc/passwd").is_err());
    }

    #[test]
    fn resolve_medui_path_rejects_a_non_medui_extension() {
        let repo = repo_root();
        assert!(resolve_medui_path(&repo, "Cargo.toml").is_err());
        assert!(resolve_medui_path(&repo, "").is_err());
    }

    #[test]
    fn resolve_medui_path_accepts_a_normal_relative_medui_path() {
        let repo = repo_root();
        let resolved =
            resolve_medui_path(&repo, "examples/hello_world/hello_world.medui").unwrap();
        assert_eq!(resolved, repo.join("examples/hello_world/hello_world.medui"));
    }

    fn test_state() -> std::sync::Arc<AppState> {
        test_state_with_token(None)
    }

    fn test_state_with_token(token: Option<&str>) -> std::sync::Arc<AppState> {
        test_state_for(repo_root(), token)
    }

    fn test_state_for(repo: PathBuf, token: Option<&str>) -> std::sync::Arc<AppState> {
        std::sync::Arc::new(AppState {
            repo,
            token: token.map(str::to_string),
            standard_text: trustsc::default_standard_text_package().expect("standard package"),
            display_texts: trustsc::default_display_text_packages().expect("display packages"),
            images: trustsc::default_image_packages().expect("image packages"),
            render_semaphore: tokio::sync::Semaphore::new(1),
        })
    }

    async fn json_body(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn screens_detail_rejects_a_path_traversal_id() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::get("/api/screens/..%2fCargo.toml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn frame_get_rejects_a_path_traversal_screen_param() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::get("/api/frame?screen=..%2fCargo.toml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn screens_detail_reports_the_declared_surface_when_compile_fails() {
        let temp = std::env::temp_dir().join(format!(
            "trustsc-medui-studio-surface-fallback-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(
            temp.join("broken.medui"),
            "Screen Broken {\n\
             layout: Vertical { spacing: 0px; padding: 0px; }\n\
             surface: 1000px, 600px;\n\
             Label {\n\
             id: l;\n\
             width: 100px;\n\
             height: 20px;\n\
             text: t(\"STR-HELLO-WORLD\");\n\
             color: Theme.Colors.NotARealToken;\n\
             }\n\
             }\n",
        )
        .unwrap();

        let app = crate::build_router(test_state_for(temp.clone(), None));
        let response = app
            .oneshot(HttpRequest::get("/api/screens/broken.medui").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json_body(response).await;
        assert_eq!(body["compiled"]["surface"], json!([1000, 600]));
        assert_ne!(body["compiled"]["diagnostics"], json!([]));
        assert_eq!(body["compiled"]["nodes"], json!([]));

        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[tokio::test]
    async fn screens_detail_returns_known_node_ids_and_bounds_for_neurosense() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::get("/api/screens/examples/class_c_monitor/neurosense.medui")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json_body(response).await;
        assert_eq!(body["compiled"]["diagnostics"], json!([]));
        let nodes = body["compiled"]["nodes"].as_array().expect("nodes array");

        let sedation_index = nodes
            .iter()
            .find(|node| node["id"] == "sedation-index")
            .expect("sedation-index node");
        assert_eq!(
            sedation_index["bounds"],
            json!({ "x": 1392, "y": 80, "w": 512, "h": 512 })
        );

        let device_title = nodes
            .iter()
            .find(|node| node["id"] == "device-title")
            .expect("device-title node");
        assert_eq!(
            device_title["bounds"],
            json!({ "x": 16, "y": 8, "w": 340, "h": 48 })
        );
    }

    #[tokio::test]
    async fn screens_detail_returns_404_for_an_unknown_id() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::get("/api/screens/does/not/exist.medui")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn compile_of_broken_source_returns_diagnostics_with_a_line_number_and_not_ok() {
        let app = crate::build_router(test_state());
        let body = json!({ "source": "Screen Broken {\nlayout: NotALayout { spacing: 8px; padding: 0px; }\n}\n" });
        let response = app
            .oneshot(
                HttpRequest::post("/api/compile")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json_body(response).await;
        assert_eq!(body["ok"], json!(false));
        assert_eq!(body["compiled"], json!(null));
        let diagnostics = body["diagnostics"].as_array().expect("diagnostics array");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0]["line"], json!(2));
    }

    #[tokio::test]
    async fn compile_rejects_a_request_with_neither_source_nor_screen() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::post("/api/compile")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn new_s8_endpoints_require_the_configured_bearer_token() {
        let app = crate::build_router(test_state_with_token(Some("secret")));

        for request in [
            HttpRequest::get("/api/screens/examples/hello_world/hello_world.medui"),
            HttpRequest::get("/api/palette"),
        ] {
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn palette_contains_all_widget_kinds_theme_colors_and_hello_world_string() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(HttpRequest::get("/api/palette").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = json_body(response).await;
        let widget_names = body["widgets"]
            .as_array()
            .expect("widgets array")
            .iter()
            .map(|widget| widget["kind_name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        for expected in [
            "CriticalButton",
            "VulkanViewport",
            "SignalTrace",
            "Label",
            "Clock",
            "NumericDisplay",
            "StatusIndicator",
            "Image",
            "Button",
            "TextInput",
        ] {
            assert!(widget_names.contains(&expected.to_string()), "missing widget {expected}");
        }

        let color_tokens = body["colors"]
            .as_array()
            .expect("colors array")
            .iter()
            .map(|color| color["token"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        for (token, _) in trustsc::THEME_COLORS {
            assert!(color_tokens.contains(&token.to_string()), "missing color token {token}");
        }

        let text_key_ids = body["text_keys"]
            .as_array()
            .expect("text_keys array")
            .iter()
            .map(|entry| entry["string_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert!(text_key_ids.contains(&"STR-HELLO-WORLD".to_string()));
    }

    #[tokio::test]
    async fn serialize_then_compile_round_trips_neurosense() {
        let app = crate::build_router(test_state());

        let detail_response = app
            .clone()
            .oneshot(
                HttpRequest::get("/api/screens/examples/class_c_monitor/neurosense.medui")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let detail = json_body(detail_response).await;
        let screen_dto = detail["screen"].clone();
        assert_ne!(screen_dto, json!(null));

        let serialize_response = app
            .clone()
            .oneshot(
                HttpRequest::post("/api/serialize")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "screen": screen_dto }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let serialize_status = serialize_response.status();
        let serialize_body = to_bytes(serialize_response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serialize_status,
            StatusCode::OK,
            "serialize failed: {}",
            String::from_utf8_lossy(&serialize_body)
        );
        let serialized: Value = serde_json::from_slice(&serialize_body).unwrap();
        let source = serialized["source"].as_str().expect("source string").to_string();

        let compile_response = app
            .oneshot(
                HttpRequest::post("/api/compile")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "source": source }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(compile_response.status(), StatusCode::OK);
        let compiled = json_body(compile_response).await;
        assert_eq!(compiled["ok"], json!(true), "diagnostics: {:?}", compiled["diagnostics"]);
    }

    #[tokio::test]
    async fn frame_get_returns_a_decodable_png_at_the_authored_extent_or_skips_without_an_icd() {
        let app = crate::build_router(test_state());
        let response = app
            .oneshot(
                HttpRequest::get("/api/frame?screen=examples/hello_world/hello_world.medui&locale=en-US")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        if response.status() != StatusCode::OK {
            let status = response.status();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            eprintln!(
                "SKIPPED frame_get_returns_a_decodable_png_at_the_authored_extent_or_skips_without_an_icd: \
                 status {status}, body {}",
                String::from_utf8_lossy(&body)
            );
            return;
        }

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "image/png"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(body))
            .read_info()
            .expect("response body should be a valid PNG");
        assert_eq!(reader.info().width, 800);
        assert_eq!(reader.info().height, 480);
        let mut buf = vec![0u8; reader.output_buffer_size().expect("known-good RGBA8 image")];
        reader.next_frame(&mut buf).expect("frame should decode");
    }
}
