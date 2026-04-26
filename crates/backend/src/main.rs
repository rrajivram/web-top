use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use shared::{CpuCore, MemStats, MetricsSnapshot, ProcessInfo};
use std::{sync::Arc, time::Duration};
use sysinfo::{System, Users};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, services::ServeDir};

type SharedState = Arc<RwLock<MetricsSnapshot>>;

async fn metrics_handler(State(state): State<SharedState>) -> Json<MetricsSnapshot> {
    Json(state.read().await.clone())
}

async fn health() -> &'static str {
    "ok"
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| ws_connection(socket, state))
}

async fn ws_connection(mut socket: WebSocket, state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let json = {
            let snap = state.read().await;
            serde_json::to_string(&*snap).unwrap_or_default()
        };
        if socket.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
}

async fn kill_process(Path(pid): Path<u32>) -> StatusCode {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if result == 0 {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::FORBIDDEN
    }
}

fn collect_snapshot(sys: &mut System, users: &Users) -> MetricsSnapshot {
    sys.refresh_all();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let cpu_cores: Vec<CpuCore> = sys
        .cpus()
        .iter()
        .enumerate()
        .map(|(i, cpu)| CpuCore {
            id: i,
            usage: cpu.cpu_usage(),
        })
        .collect();

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| {
            let user = p
                .user_id()
                .and_then(|uid| users.get_user_by_id(uid))
                .map(|u| u.name().to_string())
                .unwrap_or_default();
            ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                user,
                cpu_usage: p.cpu_usage(),
                memory_bytes: p.memory(),
                gpu_usage: None,
            }
        })
        .collect();

    processes.sort_by(|a, b| {
        b.cpu_usage
            .partial_cmp(&a.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    MetricsSnapshot {
        timestamp,
        cpu_cores,
        gpu_cores: vec![],
        memory: collect_memory(sys),
        processes,
    }
}

fn collect_memory(sys: &System) -> MemStats {
    let (wired, cached) = macos_vm_info();
    MemStats {
        total: sys.total_memory(),
        used: sys.used_memory(),
        available: sys.available_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
        wired,
        cached,
    }
}

#[cfg(target_os = "macos")]
fn macos_vm_info() -> (u64, u64) {
    let output = match std::process::Command::new("vm_stat").output() {
        Ok(o) => o,
        Err(_) => return (0, 0),
    };
    let text = String::from_utf8_lossy(&output.stdout);

    let mut page_size: u64 = 4096;
    let mut wired_pages: u64 = 0;
    let mut file_backed: u64 = 0;
    let mut purgeable: u64 = 0;

    for line in text.lines() {
        if line.starts_with("Mach Virtual Memory Statistics") {
            if let Some(pos) = line.find("page size of ") {
                let rest = &line[pos + 13..];
                if let Some(end) = rest.find(' ') {
                    page_size = rest[..end].parse().unwrap_or(4096);
                }
            }
        } else if line.starts_with("Pages wired down:") {
            wired_pages = parse_vm_stat_value(line);
        } else if line.starts_with("File-backed pages:") {
            file_backed = parse_vm_stat_value(line);
        } else if line.starts_with("Pages purgeable:") {
            purgeable = parse_vm_stat_value(line);
        }
    }

    (wired_pages * page_size, (file_backed + purgeable) * page_size)
}

fn parse_vm_stat_value(line: &str) -> u64 {
    line.split(':')
        .nth(1)
        .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
        .unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
fn macos_vm_info() -> (u64, u64) {
    (0, 0)
}

#[tokio::main]
async fn main() {
    let dist_path = std::env::var("FRONTEND_DIST")
        .unwrap_or_else(|_| "crates/frontend/dist".to_string());

    let mut sys = System::new_all();
    let users = Users::new_with_refreshed_list();

    let initial = collect_snapshot(&mut sys, &users);
    let shared_state: SharedState = Arc::new(RwLock::new(initial));

    let state_clone = shared_state.clone();
    tokio::spawn(async move {
        let mut sys = sys;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let users = Users::new_with_refreshed_list();
            let snapshot = collect_snapshot(&mut sys, &users);
            *state_clone.write().await = snapshot;
        }
    });

    let app = Router::new()
        .route("/api/metrics", get(metrics_handler))
        .route("/api/health", get(health))
        .route("/api/process/{pid}", delete(kill_process))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(shared_state)
        .fallback_service(ServeDir::new(dist_path));

    let addr: std::net::SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("web-top listening on http://{addr}");
    println!("Access from phone: http://<your-mac-ip>:8080");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
