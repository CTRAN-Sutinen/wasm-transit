//! transit-server — the local development server.
//!
//! Three jobs, nothing more:
//!
//! 1. serve the built frontend (default `www/dist`, produced by
//!    `npm run build` in `www/` — and, if you built one,
//!    `basemap.pmtiles` in `www/public/` is copied there) as static
//!    files with correct Range request support, which PMTiles needs;
//! 2. forward `/rt?url=<encoded>` to any GTFS-rt (or static) URL and
//!    return the bytes with CORS headers, so the browser can poll agency
//!    feeds that don't send `Access-Control-Allow-Origin`;
//! 3. bind to localhost only — it is a dev tool, not a deployment.
//!
//! No transit data processing happens here: parsing stays in the wasm
//! module in the page. Run from the workspace root:
//!
//!     cargo run -p transit-server

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let www_dir = std::env::var("TRANSIT_WWW").unwrap_or_else(|_| "www/dist".into());
    let addr = std::env::var("TRANSIT_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let client = reqwest::Client::builder()
        .user_agent("transit-server/0.1 (local development)")
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client");

    let app = Router::new()
        .route("/rt", get(proxy_rt))
        .fallback_service(ServeDir::new(&www_dir))
        .with_state(client);

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("transit-server: http://{addr}  (serving ./{www_dir})");
    println!("gtfs-rt proxy:  /rt?url=<encoded feed URL>");
    axum::serve(listener, app).await.expect("server run");
}

/// Stream upstream bytes back to the browser with CORS headers.
async fn proxy_rt(
    State(client): State<reqwest::Client>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(url) = params.get("url") else {
        return (StatusCode::BAD_REQUEST, "missing ?url= parameter").into_response();
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (StatusCode::BAD_REQUEST, "url must be http(s)://").into_response();
    }

    let upstream = match client.get(url).send().await {
        Ok(response) => response,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream fetch failed: {e}"),
            )
                .into_response()
        }
    };
    if !upstream.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            format!("upstream returned HTTP {}", upstream.status()),
        )
            .into_response();
    }

    let mut headers = HeaderMap::new();
    if let Some(content_type) = upstream.headers().get(reqwest::header::CONTENT_TYPE) {
        if let Ok(value) = HeaderValue::from_bytes(content_type.as_bytes()) {
            headers.insert(header::CONTENT_TYPE, value);
        }
    }
    // Realtime feeds must never be cached by the browser.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));

    (headers, Body::from_stream(upstream.bytes_stream())).into_response()
}
