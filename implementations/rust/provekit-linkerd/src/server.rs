// SPDX-License-Identifier: Apache-2.0
//
// server.rs — JSON-RPC NDJSON dispatch loop.
//
// Binds a Unix domain socket, accepts clients, dispatches NDJSON
// JSON-RPC 2.0 messages to method handlers, and manages daemon lifecycle:
//
//   - Socket permissions: 0600 (owner-only) per R2.
//   - Idle timeout: shuts down after `idle_timeout` with zero clients per R4.
//     On test builds the caller supplies a short timeout.
//   - Snapshot persistence: writes cache to XDG_CACHE_HOME on shutdown per R14.
//   - Multi-client: concurrent connections share one `Arc<Mutex<ProjectState>>`.
//     The mutex serialises all link() calls (R8's conformance item 3).
//
// UID rejection (R2): On Linux and macOS we read the peer's UID via
// `UnixStream::peer_cred()` and disconnect if it doesn't match `getuid()`.
// On other platforms (Windows, BSD without peer_cred) we skip the check
// and document the gap.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, Notify};
use tracing::{debug, error, info, warn};

use crate::methods::{
    handle_flush_cache, handle_get_diagnostics, handle_parse_file, handle_project_status,
    rpc_error, shutdown_response, ERR_METHOD_NOT_FOUND,
};
use crate::state::ProjectState;
use crate::snapshot;

/// Configuration for the daemon server.
pub struct ServerConfig {
    /// Path of the Unix domain socket to bind.
    pub socket_path: PathBuf,
    /// Path to write the snapshot on shutdown.
    pub snapshot_path: PathBuf,
    /// Idle timeout: shut down if zero clients for this duration.
    pub idle_timeout: Duration,
    /// LRU cache capacity (R12).
    pub cache_cap: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path("default"),
            snapshot_path: default_snapshot_path("default"),
            idle_timeout: Duration::from_secs(300), // 5 min per R4
            cache_cap: 1024,
        }
    }
}

/// Compute the socket path for a given projectCid per R1.
pub fn default_socket_path(project_cid: &str) -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
    PathBuf::from(base)
        .join("provekit")
        .join(format!("linkerd-{project_cid}.sock"))
}

/// Compute the snapshot path for a given projectCid per R14.
pub fn default_snapshot_path(project_cid: &str) -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| {
        dirs_next_cache_home()
    });
    PathBuf::from(base)
        .join("provekit")
        .join("linkerd")
        .join(project_cid)
        .join("snapshot.bin")
}

fn dirs_next_cache_home() -> String {
    // Fallback: ~/.cache on Unix, %LOCALAPPDATA% on Windows.
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".cache");
        return p.to_string_lossy().into_owned();
    }
    std::env::temp_dir().to_string_lossy().into_owned()
}

/// Run the daemon with the given config.
///
/// Loads snapshot if available, binds socket, accepts connections,
/// and shuts down cleanly on idle timeout or `shutdown` RPC.
pub async fn run(config: ServerConfig) -> anyhow::Result<()> {
    // Load snapshot if available (R14).
    let state = match snapshot::load(&config.snapshot_path) {
        Ok(Some(s)) => {
            info!("warm-start: loaded snapshot from {}", config.snapshot_path.display());
            Arc::new(Mutex::new(s))
        }
        Ok(None) => {
            info!("cold-start: no snapshot found");
            Arc::new(Mutex::new(ProjectState::new(config.cache_cap)))
        }
        Err(e) => {
            warn!("snapshot load failed ({e}); starting cold");
            Arc::new(Mutex::new(ProjectState::new(config.cache_cap)))
        }
    };

    // Remove stale socket if present.
    let _ = std::fs::remove_file(&config.socket_path);

    // Create parent directories.
    if let Some(parent) = config.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Set umask so the socket is created 0600.
    // We do this by setting the socket perms after bind on platforms
    // where umask manipulation is undesirable.
    let listener = UnixListener::bind(&config.socket_path)?;

    // Set socket file permissions to 0600 (R2).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &config.socket_path,
            std::fs::Permissions::from_mode(0o600),
        )?;
    }

    info!("listening on {}", config.socket_path.display());

    let client_count = Arc::new(AtomicUsize::new(0));
    let shutdown_notify = Arc::new(Notify::new());

    // Idle-timeout watcher task.
    {
        let client_count = client_count.clone();
        let shutdown_notify = shutdown_notify.clone();
        let idle_timeout = config.idle_timeout;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(idle_timeout).await;
                if client_count.load(Ordering::SeqCst) == 0 {
                    info!("idle timeout — shutting down");
                    shutdown_notify.notify_one();
                    return;
                }
            }
        });
    }

    let snapshot_path = config.snapshot_path.clone();
    let socket_path = config.socket_path.clone();

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        // Enforce owner-only connection (R2, R16).
                        #[cfg(any(target_os = "linux", target_os = "macos"))]
                        {
                            match stream.peer_cred() {
                                Ok(cred) if cred.uid() != unsafe { libc::getuid() } => {
                                    warn!("rejected connection from uid {}", cred.uid());
                                    continue;
                                }
                                Err(e) => {
                                    warn!("peer_cred() failed: {e}; rejecting connection");
                                    continue;
                                }
                                Ok(_) => {} // same uid — allow
                            }
                        }

                        let state = state.clone();
                        let client_count = client_count.clone();
                        let shutdown_notify = shutdown_notify.clone();
                        let snapshot_path = snapshot_path.clone();
                        let socket_path = socket_path.clone();

                        client_count.fetch_add(1, Ordering::SeqCst);
                        tokio::spawn(async move {
                            handle_client(
                                stream,
                                state,
                                shutdown_notify,
                                snapshot_path,
                                socket_path,
                            )
                            .await;
                            client_count.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                    Err(e) => {
                        error!("accept error: {e}");
                    }
                }
            }
            _ = shutdown_notify.notified() => {
                info!("shutdown signal received — writing snapshot and exiting");
                {
                    let st = state.lock().await;
                    if let Err(e) = snapshot::save(&config.snapshot_path, &st) {
                        warn!("snapshot write failed: {e}");
                    }
                }
                // Remove socket file.
                let _ = std::fs::remove_file(&config.socket_path);
                return Ok(());
            }
        }
    }
}

/// Handle a single client connection: read NDJSON requests, dispatch, write responses.
async fn handle_client(
    stream: UnixStream,
    state: Arc<Mutex<ProjectState>>,
    shutdown_notify: Arc<Notify>,
    snapshot_path: PathBuf,
    _socket_path: PathBuf,
) {
    let (reader_half, mut writer_half) = stream.into_split();
    let mut lines = BufReader::new(reader_half).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Json = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                });
                let _ = write_response(&mut writer_half, &err_resp).await;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Json::Null);
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let params = request
            .get("params")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        debug!("method={method} id={id}");

        let response = match method.as_str() {
            "parseFile" => handle_parse_file(state.clone(), &params, &id).await,
            "getDiagnostics" => handle_get_diagnostics(state.clone(), &params, &id).await,
            "projectStatus" => handle_project_status(state.clone(), &params, &id).await,
            "flushCache" => handle_flush_cache(state.clone(), &params, &id).await,
            "shutdown" => {
                // Write snapshot, then signal the accept loop to exit.
                {
                    let st = state.lock().await;
                    if let Err(e) = snapshot::save(&snapshot_path, &st) {
                        warn!("snapshot write on shutdown: {e}");
                    }
                }
                let resp = shutdown_response(&id);
                let _ = write_response(&mut writer_half, &resp).await;
                shutdown_notify.notify_one();
                return;
            }
            _ => rpc_error(ERR_METHOD_NOT_FOUND, &format!("method not found: {method}"), &id),
        };

        if let Err(e) = write_response(&mut writer_half, &response).await {
            warn!("write response error: {e}");
            return;
        }
    }
}

async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    value: &Json,
) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).unwrap_or_default();
    bytes.push(b'\n');
    writer.write_all(&bytes).await
}

// Re-export Json for server.rs-internal use.
use serde_json::Value as Json;

// -------------------------------------------------------------------
// Convenience re-exports used by tests.
// -------------------------------------------------------------------

#[cfg(unix)]
extern crate libc;
