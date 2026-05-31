// SPDX-License-Identifier: Apache-2.0
//
// ra_host.rs: resident, warm rust-analyzer host inside the persistent daemon.
//
// Specs #1705/#1706/#1707 and 2026-05-30-callee-resolution-tiers §2.T2b.
//
// THE PROBLEM this solves: the implication lifter used to COLD-SPAWN
// rust-analyzer inside `lift_implications` on EVERY mint. A cold index of the
// cli workspace takes ~260s, so every mint paid a 4+ minute tax. The machinery
// (the LSP client in `provekit_walk::ra_oracle`) is correct; the only fault was
// re-spawn + re-index per mint.
//
// THE FIX: keep ONE warm `RaOracle` session PER workspace root, indexed ONCE,
// alive across mints, here in the long-lived daemon.
//
// NON-BLOCKING READINESS is the load-bearing constraint (the refuse-floor is
// non-negotiable): `RaOracle::start` blocks ~260s through the cold index, so it
// MUST NOT run in the request path. Each session is therefore a dedicated OS
// thread that owns the `RaOracle`:
//
//   - The thread runs `RaOracle::start` (blocking through the cold index), then
//     flips a `Phase` flag (Spawning -> Ready | Failed) and loops servicing
//     `ResolveBatch` commands sent over a channel.
//   - The async daemon handler reads the phase flag (cheap, non-blocking). Not
//     Ready -> answer immediately so the caller refuses to Tier 1/2a. Ready ->
//     send the batch and await the result. The first cold mint thus never hangs
//     for the index; the second (warm) mint resolves.
//
// The cache (resolve_cache.rs) sits IN FRONT of this in the request handler:
// cache hits never reach RA, so even a cold daemon answers cached files with no
// spawn. ra_host is consulted only for cache MISSES on a Ready session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use provekit_walk::ra_oracle::{RaOracle, ResolveQuery};
use tracing::{info, warn};

/// Readiness phase of a resident RA session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// rust-analyzer is spawning / indexing the workspace. Resolution refuses.
    Spawning,
    /// rust-analyzer is quiescent: resolution is serviceable and reproducible.
    Ready,
    /// The session could not start (oracle off, binary missing, handshake
    /// failed, or RA never reached quiescence). Resolution permanently refuses
    /// for this session; the caller falls back to the syntactic tiers.
    Failed,
}

/// The classified outcome of resolving one position against the warm session.
///   Resolved { krate, type_stem } -> a definite crate, plus the best-effort
///                                    receiver-type stem (None when the type
///                                    could not be disambiguated). The type stem
///                                    is what lets a panic-site `x.unwrap()` key
///                                    on the rust-std shim's disambiguated
///                                    partial (`option_unwrap`) instead of the
///                                    ambiguous bare leaf.
///   Refused  -> deterministic refuse (null/unmappable/ambiguous)
///   NotReady -> ContentModified budget exhausted (RA still moving)
#[derive(Debug, Clone)]
pub enum PosResult {
    Resolved {
        krate: String,
        type_stem: Option<String>,
        definition_files: Vec<PathBuf>,
    },
    Refused,
    NotReady,
}

/// A batch command sent to a session thread: queries plus a one-shot reply.
struct BatchCmd {
    queries: Vec<ResolveQuery>,
    reply: Sender<Vec<PosResult>>,
}

/// One resident rust-analyzer session for a single workspace root.
pub struct RaSession {
    phase: Arc<Mutex<Phase>>,
    cmd_tx: Sender<BatchCmd>,
    _thread: JoinHandle<()>,
}

impl RaSession {
    /// Spawn a session thread for `workspace_root`. Returns immediately; the
    /// thread runs the (blocking) `RaOracle::start` in the background and flips
    /// the phase when it settles. The phase begins `Spawning`.
    fn spawn(workspace_root: PathBuf) -> RaSession {
        let phase = Arc::new(Mutex::new(Phase::Spawning));
        let (cmd_tx, cmd_rx): (Sender<BatchCmd>, Receiver<BatchCmd>) = std::sync::mpsc::channel();
        let phase_thread = phase.clone();
        let thread = std::thread::Builder::new()
            .name(format!("ra-session:{}", workspace_root.display()))
            .spawn(move || session_loop(workspace_root, phase_thread, cmd_rx))
            .expect("spawn ra-session thread");
        RaSession {
            phase,
            cmd_tx,
            _thread: thread,
        }
    }

    /// Current phase (cheap, non-blocking).
    pub fn phase(&self) -> Phase {
        *self.phase.lock().unwrap()
    }

    /// Resolve a batch against the warm session. ONLY call when phase is Ready;
    /// returns one `PosResult` per query in order. If the session thread has
    /// died, returns `NotReady` for every query (refuse).
    pub fn resolve(&self, queries: Vec<ResolveQuery>) -> Vec<PosResult> {
        let n = queries.len();
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if self
            .cmd_tx
            .send(BatchCmd {
                queries,
                reply: reply_tx,
            })
            .is_err()
        {
            return vec_not_ready(n);
        }
        match reply_rx.recv() {
            Ok(results) => results,
            Err(_) => vec_not_ready(n),
        }
    }
}

fn vec_not_ready(n: usize) -> Vec<PosResult> {
    (0..n).map(|_| PosResult::NotReady).collect()
}

/// The session thread body: start RA (blocking), flip phase, then service
/// batches until the command channel closes (daemon shutdown).
fn session_loop(workspace_root: PathBuf, phase: Arc<Mutex<Phase>>, cmd_rx: Receiver<BatchCmd>) {
    info!(
        workspace = %workspace_root.display(),
        "ra-host: starting resident rust-analyzer session (indexing once, in background)"
    );
    // This blocks through the cold index (~260s on the cli workspace). It runs
    // here, off the request path, so the daemon stays responsive and the first
    // cold mint refuses cleanly instead of hanging.
    let mut oracle = match RaOracle::start(&workspace_root) {
        Some(o) => o,
        None => {
            // Oracle off, binary missing, or handshake failed. RaOracle::start
            // already logged the specific cause.
            *phase.lock().unwrap() = Phase::Failed;
            warn!(
                workspace = %workspace_root.display(),
                "ra-host: rust-analyzer session failed to start; all resolutions refuse"
            );
            return;
        }
    };
    // RaOracle::start returns Some even if quiescence was not reached (it logs a
    // warning and resolves best-effort). For the resident host we treat that as
    // Ready: a warm session that reached quiescence resolves fully, and a
    // best-effort one still classifies NotReady per query via the
    // ContentModified path, so the refuse-floor holds either way.
    *phase.lock().unwrap() = Phase::Ready;
    info!(
        workspace = %workspace_root.display(),
        "ra-host: resident rust-analyzer session READY (warm, reused across mints)"
    );

    while let Ok(cmd) = cmd_rx.recv() {
        let mut results = Vec::with_capacity(cmd.queries.len());
        for q in &cmd.queries {
            // Resolve BOTH crate and receiver-type stem in one definition
            // round-trip; the stem disambiguates the panic partial downstream.
            let r = match oracle.resolve_typed_classified(q) {
                Ok(Some(tr)) => PosResult::Resolved {
                    krate: tr.krate,
                    type_stem: tr.type_stem,
                    definition_files: tr.definition_files,
                },
                Ok(None) => PosResult::Refused,
                Err(()) => PosResult::NotReady,
            };
            results.push(r);
        }
        // A dropped receiver (caller gave up) is fine: just move on.
        let _ = cmd.reply.send(results);
    }
    info!(
        workspace = %workspace_root.display(),
        "ra-host: command channel closed; resident rust-analyzer session ending"
    );
}

/// The host: owns one resident RA session per absolute workspace root.
///
/// Lives behind an `Arc` shared across daemon clients. Sessions are created
/// lazily on the first resolve request for a workspace and reused forever after
/// (until daemon shutdown / idle timeout). Keyed by the canonicalized absolute
/// workspace path so `.`/symlink variants of one root share a session.
#[derive(Default)]
pub struct RaHost {
    sessions: Mutex<HashMap<PathBuf, Arc<RaSession>>>,
}

impl RaHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the session for `workspace_root`, lazily spawning it (non-blocking)
    /// on first use. The returned session may still be `Spawning`.
    pub fn session_for(&self, workspace_root: &Path) -> Arc<RaSession> {
        let key =
            std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(s) = sessions.get(&key) {
            return s.clone();
        }
        let session = Arc::new(RaSession::spawn(key.clone()));
        sessions.insert(key, session.clone());
        session
    }
}
