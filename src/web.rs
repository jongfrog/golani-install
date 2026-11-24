use crate::{
    java::{find_system_java, managed_java},
    server::{load_metadata, recorded_running_pid, start, stop, update_server, rcon_settings},
    minecraft::{MinecraftApi, ServerKind},
};
use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;
use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;

#[derive(Clone)]
pub struct WebState {
    pub servers_dir: PathBuf,
    pub java_dir: PathBuf,
    pub default_server: Option<String>,
}

#[derive(Serialize, Clone)]
struct ServerView {
    name: String,
    kind: String,
    version: String,
    port: u16,
    ram_mb: u32,
    running: bool,
    needs_restart: bool,
    last_exit_status: Option<String>,
}

#[derive(Serialize)]
struct ServerDetail {
    name: String,
    kind: String,
    version: String,
    paper_build: Option<String>,
    ram_mb: u32,
    port: u16,
    running: bool,
    needs_restart: bool,
    created_at: u64,
    last_started_at: Option<u64>,
    last_exit_status: Option<String>,
    java_version: u8,
    rcon_port: u16,
    rcon_password: String,
    properties: serde_json::Value,
}

#[derive(Deserialize)]
struct CreateServerParams {
    name: String,
    kind: String,
    version: String,
    ram_mb: u32,
    port: u16,
    game_mode: String,
    difficulty: String,
    seed: Option<String>,
    motd: String,
}

#[derive(Deserialize)]
struct LogQuery {
    lines: Option<usize>,
    follow: Option<bool>,
    filter: Option<String>,
}

pub async fn serve(state: WebState) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/servers", get(servers))
        .route("/api/servers/{name}", get(server_detail))
        .route("/api/servers/{name}/start", post(start_server))
        .route("/api/servers/{name}/stop", post(stop_server))
        .route("/api/servers/{name}/restart", post(restart_server))
        .route("/api/servers/{name}/update", post(update_server_handler))
        .route("/api/servers/{name}/delete", post(delete_server))
        .route("/api/servers/{name}/logs", get(server_logs))
        .route("/api/servers/{name}/logs/ws", get(server_logs_ws))
        .route("/api/servers/{name}/console", get(console_page))
        .route("/api/servers/{name}/console/ws", get(console_ws))
        .route("/api/servers/create", post(create_server))
        .route("/api/versions/vanilla", get(vanilla_versions))
        .route("/api/versions/paper", get(paper_versions))
        .route("/api/ports/next", get(next_available_port))
        .route("/api/default-server", get(default_server))
        .with_state(Arc::new(state));
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8788").await?;
    axum::serve(listener, app).await
}

// Main page
async fn index() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

// Console page
async fn console_page(Path(name): Path<String>) -> Html<&'static str> {
    Html(include_str!("web/console.html"))
}

// Default server endpoint
async fn default_server(State(state): State<Arc<WebState>>) -> Json<Option<String>> {
    Json(state.default_server.clone())
}

// List servers
async fn servers(State(state): State<Arc<WebState>>) -> Json<Vec<ServerView>> {
    let views = std::fs::read_dir(&state.servers_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let m = load_metadata(&path).ok()?;
            let running = recorded_running_pid(&path).is_some();
            let needs_restart = m.last_exit_status.as_deref() == Some("updated");
            Some(ServerView {
                name: m.name.clone(),
                kind: format!("{:?}", m.kind),
                version: m.minecraft_version.clone(),
                port: m.port,
                ram_mb: m.ram_mb,
                running,
                needs_restart,
                last_exit_status: m.last_exit_status.clone(),
            })
        })
        .collect();
    Json(views)
}

// Server detail
async fn server_detail(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let path = state.servers_dir.join(&name);
    if !path.join(".golani.json").exists() {
        return (StatusCode::NOT_FOUND, "서버를 찾을 수 없습니다").into_response();
    }
    let metadata = match load_metadata(&path) {
        Ok(m) => m,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "메타데이터 읽기 실패").into_response(),
    };
    let running = recorded_running_pid(&path).is_some();
    let needs_restart = metadata.last_exit_status.as_deref() == Some("updated");
    
    // Read server.properties
    let props_text = std::fs::read_to_string(path.join("server.properties")).unwrap_or_default();
    let mut properties = serde_json::Map::new();
    for line in props_text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            properties.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    let rcon_port = properties.get("rcon.port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let rcon_password = properties.get("rcon.password").and_then(|v| v.as_str()).unwrap_or("").to_string();
    
    let detail = ServerDetail {
        name: metadata.name,
        kind: format!("{:?}", metadata.kind),
        version: metadata.minecraft_version,
        paper_build: metadata.paper_build,
        ram_mb: metadata.ram_mb,
        port: metadata.port,
        running,
        needs_restart,
        created_at: metadata.created_at_unix,
        last_started_at: metadata.last_started_at_unix,
        last_exit_status: metadata.last_exit_status,
        java_version: metadata.java_version,
        rcon_port,
        rcon_password,
        properties: serde_json::Value::Object(properties),
    };
    Json(detail).into_response()
}

// Start server
async fn start_server(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let server_dir = state.servers_dir.join(&name);
    let m = match load_metadata(&server_dir) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let java = find_system_java(m.java_version)
        .or_else(|| managed_java(
            &crate::paths::AppPaths::new(
                state.java_dir.parent().unwrap_or(&state.java_dir).to_path_buf(),
            ),
            m.java_version,
        ))
        .ok_or_else(|| crate::error::Error::ServerCreation("호환되는 Java를 찾을 수 없습니다".into()))
        .map(|j| j.executable);
    match java {
        Ok(java_path) => {
            match start(&server_dir, &java_path).await {
                Ok(_) => StatusCode::NO_CONTENT,
                Err(_) => StatusCode::BAD_REQUEST,
            }
        }
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

// Stop server
async fn stop_server(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let path = state.servers_dir.join(name);
    match stop(&path).await {
        Ok(()) => StatusCode::NO_CONTENT,
        _ => StatusCode::BAD_REQUEST,
    }
}

// Restart server
async fn restart_server(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let server_dir = state.servers_dir.join(&name);
    let m = match load_metadata(&server_dir) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let java = find_system_java(m.java_version)
        .or_else(|| managed_java(
            &crate::paths::AppPaths::new(
                state.java_dir.parent().unwrap_or(&state.java_dir).to_path_buf(),
            ),
            m.java_version,
        ))
        .ok_or_else(|| crate::error::Error::ServerCreation("호환되는 Java를 찾을 수 없습니다".into()))
        .map(|j| j.executable);
    match java {
        Ok(java_path) => {
            match crate::server::restart(&server_dir, &java_path).await {
                Ok(_) => StatusCode::NO_CONTENT,
                Err(_) => StatusCode::BAD_REQUEST,
            }
        }
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

// Update server
async fn update_server_handler(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    let path = state.servers_dir.join(name);
    match update_server(&path).await {
        Ok(()) => StatusCode::NO_CONTENT,
        _ => StatusCode::BAD_REQUEST,
    }
}

// Delete server (2-step: confirmation in body)
async fn delete_server(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let confirm = body.get("confirm").and_then(|v| v.as_str()).unwrap_or("");
    if confirm != "삭제" {
        return (StatusCode::BAD_REQUEST, "확인어 '삭제'를 입력해야 합니다").into_response();
    }
    let path = state.servers_dir.join(name);
    if recorded_running_pid(&path).is_some() {
        return (StatusCode::BAD_REQUEST, "실행 중인 서버는 삭제할 수 없습니다").into_response();
    }
    match crate::server::delete(&path, "삭제") {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// Server logs
async fn server_logs(
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
    Query(params): Query<LogQuery>,
) -> impl IntoResponse {
    let path = state.servers_dir.join(&name);
    let log_path = path.join("logs").join("latest.log");
    let lines = params.lines.unwrap_or(500);
    let filter = params.filter.unwrap_or_default();
    
    if !log_path.exists() {
        return (StatusCode::NOT_FOUND, "로그 파일이 없습니다").into_response();
    }
    
    let content = std::fs::read_to_string(&log_path).unwrap_or_default();
    let mut logs: Vec<&str> = content.lines().collect();
    let start = logs.len().saturating_sub(lines);
    logs = logs.drain(start..).collect();
    
    if !filter.is_empty() {
        logs.retain(|l| l.to_lowercase().contains(&filter.to_lowercase()));
    }
    
    Json(logs).into_response()
}

// WebSocket for real-time logs
async fn server_logs_ws(
    ws: WebSocketUpgrade,
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> Response {
    ws.on_upgrade(move |socket: WebSocket| async move {
        let path = state.servers_dir.join(&name).join("logs").join("latest.log");
        if !path.exists() { return; }
        
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        
        // Spawn log follower
        tokio::spawn(async move {
            use tokio::fs::File;
            use tokio::io::{AsyncBufReadExt, BufReader};
            let file = File::open(&path).await.ok();
            if let Some(file) = file {
                let mut reader = BufReader::new(file);
                let mut line = String::new();
                // Send existing content
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let _ = tx.send(line.trim_end().to_string());
                    line.clear();
                }
                // Follow new lines
                loop {
                    match reader.read_line(&mut line).await {
                        Ok(0) => { tokio::time::sleep(Duration::from_millis(500)).await; }
                        Ok(_) => { let _ = tx.send(line.trim_end().to_string()); line.clear(); }
                        Err(_) => break,
                    }
                }
            }
        });
        
        let mut sender = socket;
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() { break; }
        }
    })
}

// Console WebSocket
async fn console_ws(
    ws: WebSocketUpgrade,
    Path(name): Path<String>,
    State(state): State<Arc<WebState>>,
) -> Response {
    ws.on_upgrade(move |mut socket: WebSocket| async move {
        let server_dir = state.servers_dir.join(&name);
        let (rcon_port, rcon_password) = match rcon_settings(&server_dir) {
            Some(p) => p,
            None => return,
        };
        
        // Send initial welcome
        let _ = socket.send(Message::Text("콘솔 연결됨. ESC로 분리 (서버 계속 실행)".into())).await;
        
        // Channels for communication
        let (tx_out, mut rx_out) = mpsc::unbounded_channel::<String>(); // Messages to send to client
        let (tx_in, mut rx_in) = mpsc::unbounded_channel::<String>();  // Commands from client
        
        // Forward RCON output to WS (poll log file) -> sends to tx_out
        let tx_out_clone = tx_out.clone();
        let server_dir_clone = server_dir.clone();
        tokio::spawn(async move {
            let log_path = server_dir_clone.join("logs").join("latest.log");
            if log_path.exists() {
                use tokio::fs::File;
                use tokio::io::{AsyncBufReadExt, BufReader};
                let file = File::open(&log_path).await.ok();
                if let Some(file) = file {
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                        let _ = tx_out_clone.send(line.trim_end().to_string());
                        line.clear();
                    }
                    loop {
                        match reader.read_line(&mut line).await {
                            Ok(0) => { tokio::time::sleep(Duration::from_millis(500)).await; }
                            Ok(_) => { let _ = tx_out_clone.send(line.trim_end().to_string()); line.clear(); }
                            Err(_) => break,
                        }
                    }
                }
            }
        });
        
        // Handle incoming commands from WS -> receives from rx_in
        let rcon_addr = format!("127.0.0.1:{}", rcon_port);
        let rcon_pass = rcon_password.clone();
        tokio::spawn(async move {
            while let Some(text) = rx_in.recv().await {
                if text.trim() == "exit" { break; }
                if !text.trim().is_empty() {
                    let _ = crate::rcon::command(&rcon_addr, &rcon_pass, &text).await;
                }
            }
        });
        
        // Main loop: handle WebSocket I/O
        let mut ws = socket;
        loop {
            tokio::select! {
                // Receive messages from WebSocket
                msg = ws.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            let text_str = text.to_string();
                            if text_str.trim() == "exit" { break; }
                            if !text_str.trim().is_empty() {
                                let _ = tx_in.send(text_str);
                            }
                        }
                        Some(Ok(Message::Close(_))) => break,
                        Some(Err(_)) => break,
                        None => break,
                        _ => {}
                    }
                }
                // Send messages to WebSocket
                msg = rx_out.recv() => {
                    match msg {
                        Some(text) => {
                            if ws.send(Message::Text(text.into())).await.is_err() { break; }
                        }
                        None => break,
                    }
                }
            }
        }
    })
}

// Create server
async fn create_server(
    State(state): State<Arc<WebState>>,
    Json(params): Json<CreateServerParams>,
) -> impl IntoResponse {
    let kind = match params.kind.as_str() {
        "vanilla" => ServerKind::Vanilla,
        "paper" => ServerKind::Paper,
        _ => return (StatusCode::BAD_REQUEST, "서버 종류는 vanilla 또는 paper여야 합니다").into_response(),
    };
    
    // Validate name
    if params.name.is_empty() || !params.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return (StatusCode::BAD_REQUEST, "서버 이름은 영문, 숫자, 하이픈, 밑줄만 사용 가능").into_response();
    }
    
    let server_dir = state.servers_dir.join(&params.name);
    if server_dir.exists() {
        return (StatusCode::BAD_REQUEST, "같은 이름의 서버가 이미 있습니다").into_response();
    }
    
    // Create server
    let api = match MinecraftApi::new() { Ok(a) => a, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() };
    let download = match kind {
        ServerKind::Vanilla => api.vanilla_download(&params.version).await,
        ServerKind::Paper => api.paper_download(&params.version).await,
    };
    let download = match download { Ok(d) => d, Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() };
    
    // Create directory and download
    std::fs::create_dir_all(server_dir.join("logs")).ok();
    if let Err(e) = api.download_verified(&download, &server_dir.join("server.jar")).await {
        let _ = std::fs::remove_dir_all(&server_dir);
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }
    
    // Generate RCON password
    use rand::{distr::Alphanumeric, Rng};
    let password: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    
    // Write server.properties
    let rcon_port = params.port.saturating_add(10);
    crate::minecraft::write_server_properties(
        &server_dir.join("server.properties"),
        params.port,
        &params.motd,
        &params.game_mode,
        &params.difficulty,
        params.seed.as_deref(),
        &password,
    ).ok();
    
    // Write EULA
    std::fs::write(server_dir.join("eula.txt"), "eula=true\n").ok();
    std::fs::write(server_dir.join("logs").join("golani.log"), "eula.txt auto-accepted\n").ok();
    
    // Write metadata
    let version = params.version.clone();
    let metadata = crate::minecraft::ServerMetadata {
        name: params.name.clone(),
        kind,
        minecraft_version: version.clone(),
        paper_build: download.paper_build,
        ram_mb: params.ram_mb,
        port: params.port,
        created_at_unix: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        last_started_at_unix: None,
        last_exit_status: None,
        java_version: crate::minecraft::java_version_for(&version),
    };
    std::fs::write(server_dir.join(".golani.json"), serde_json::to_vec_pretty(&metadata).unwrap()).ok();
    
    Json(serde_json::json!({"success": true, "name": params.name})).into_response()
}

// Vanilla versions
async fn vanilla_versions() -> impl IntoResponse {
    let api = MinecraftApi::new();
    match api {
        Ok(api) => {
            match api.vanilla_versions().await {
                Ok(versions) => Json(versions).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// Paper versions
async fn paper_versions() -> impl IntoResponse {
    let api = MinecraftApi::new();
    match api {
        Ok(api) => {
            match api.paper_versions().await {
                Ok(versions) => Json(versions).into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// Next available port
async fn next_available_port() -> impl IntoResponse {
    let mut port = 25565;
    for _ in 0..100 {
        let in_use = std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_err();
        if !in_use { break; }
        port += 1;
    }
    Json(serde_json::json!({"port": port})).into_response()
}
