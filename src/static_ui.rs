use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct UiAssets;

const BUILD_HEADER: HeaderName = HeaderName::from_static("x-twit-rank-build-id");
const BUILD_EPOCH_HEADER: HeaderName = HeaderName::from_static("x-twit-rank-build-epoch");

fn build_id() -> &'static str {
    option_env!("TWIT_RANK_BUILD_ID").unwrap_or("unknown")
}

fn build_epoch() -> &'static str {
    option_env!("TWIT_RANK_BUILD_EPOCH").unwrap_or("unknown")
}

pub async fn index() -> Response {
    serve_path("index.html").unwrap_or_else(not_found)
}

pub async fn asset(Path(path): Path<String>) -> Response {
    let path = path.trim_start_matches('/').to_string();

    if path.is_empty() {
        return serve_path("index.html").unwrap_or_else(not_found);
    }

    if let Some(resp) = serve_path(&path) {
        return resp;
    }

    // SPA fallback for client-side routes (no extension).
    if !path.contains('.') {
        return serve_path("index.html").unwrap_or_else(not_found);
    }

    not_found()
}

fn serve_path(path: &str) -> Option<Response> {
    let file = UiAssets::get(path)?;
    let mut resp = (StatusCode::OK, Body::from(file.data.into_owned())).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(path)),
    );
    if path == "index.html" {
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        resp.headers_mut().insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
    }
    if let Ok(v) = HeaderValue::from_str(build_id()) {
        resp.headers_mut().insert(BUILD_HEADER, v);
    }
    if let Ok(v) = HeaderValue::from_str(build_epoch()) {
        resp.headers_mut().insert(BUILD_EPOCH_HEADER, v);
    }
    Some(resp)
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

fn content_type(path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("css") => "text/css; charset=utf-8",
        Some("gif") => "image/gif",
        Some("htm" | "html") => "text/html; charset=utf-8",
        Some("ico") => "image/x-icon",
        Some("jpeg" | "jpg") => "image/jpeg",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("map") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("ttf") => "font/ttf",
        Some("webp") => "image/webp",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
