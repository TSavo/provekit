// SPDX-License-Identifier: Apache-2.0
//
// Library surface for provekit-lsp-rust.
//
// Exposes `daemon_client` so the main provekit-lsp tower-lsp server can
// depend on this crate and call `connect_or_spawn` / `send_parse_file`
// without duplicating the implementation.
//
// The `[[bin]]` target (`src/main.rs`) speaks the per-language NDJSON
// plugin protocol (initialize / parse / shutdown).  This `[lib]` target
// exposes the daemon-client primitives for the LSP server that routes
// through `provekit-linkerd`.

pub mod daemon_client;
