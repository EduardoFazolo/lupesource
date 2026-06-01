use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{Response, StatusCode, header},
    response::{IntoResponse, sse::{Event, KeepAlive, Sse}},
    routing::get,
};
use clap::Parser;
use futures::stream::Stream;
use lupe_core::{DiffQuery, Store, compute_web_diff};
use mime_guess;
use rust_embed::RustEmbed;
use serde_json::Value;
use std::{convert::Infallible, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

// ── Embedded web assets ───────────────────────────────────────────────────────

#[derive(RustEmbed)]
#[folder = "../web/dist"]
struct WebDist;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "lupe-server")]
#[command(about = "Lupe HTTP API server")]
struct Cli {
    #[arg(long, env = "LUPE_HOME")]
    home: Option<PathBuf>,

    #[arg(long, default_value = "3001")]
    port: u16,
}

// ── State ─────────────────────────────────────────────────────────────────────

struct AppState {
    store: Store,
    tx: broadcast::Sender<String>,
}

// ── HEAD watcher ──────────────────────────────────────────────────────────────

async fn watch_tick(home: PathBuf, tx: broadcast::Sender<String>) {
    let tick_path = home.join("tick");
    let mut last = String::new();
    loop {
        if let Ok(content) = tokio::fs::read_to_string(&tick_path).await {
            let content = content.trim().to_string();
            if !content.is_empty() && content != last {
                last = content.clone();
                let _ = tx.send(content);
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_graph(State(state): State<Arc<AppState>>) -> Response<Body> {
    match state.store.build_web_graph_data(true, false).await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("error: {e}")).into_response(),
    }
}

async fn handle_diff(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DiffQuery>,
) -> Response<Body> {
    let to_id = match Uuid::parse_str(&params.to) {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid 'to' uuid").into_response(),
    };
    let from_id = params.from.as_deref().and_then(|s| Uuid::parse_str(s).ok());

    match compute_web_diff(&state.store.pool, &state.store.object_dir, from_id, to_id).await {
        Ok(diffs) => Json(diffs).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("error: {e}")).into_response(),
    }
}

async fn handle_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|msg| msg.ok())
        .map(|checkpoint_id| {
            Ok(Event::default()
                .event("checkpoint")
                .data(checkpoint_id))
        });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn handle_health() -> Json<Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn handle_static(uri: axum::http::Uri) -> Response<Body> {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match WebDist::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(file.data))
                .unwrap()
        }
        None => Response::builder()
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(
                WebDist::get("index.html").map(|f| f.data).unwrap_or_default(),
            ))
            .unwrap(),
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let store = Store::open(cli.home).await?;
    println!("lupe-server: home={}", store.home.display());

    let (tx, _) = broadcast::channel::<String>(32);
    let home = store.home.clone();
    tokio::spawn(watch_tick(home, tx.clone()));

    let state = Arc::new(AppState { store, tx });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/graph", get(handle_graph))
        .route("/api/diff", get(handle_diff))
        .route("/api/events", get(handle_events))
        .route("/health", get(handle_health))
        .fallback(handle_static)
        .layer(cors)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", cli.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("lupe-server listening on http://localhost:{}", cli.port);
    axum::serve(listener, app).await?;
    Ok(())
}
