// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-reqwest-rust: reqwest's @sugar shim.
//
// The Rust kit owns this Rust/library-specific evidence and resolves the
// packaged proof through Cargo. The CLI sees normalized proof content only
// through kit RPC.

pub const PROVEKIT_PROOF_BYTES: &[u8] = include_bytes!(
    "../blake3-512:4d37644b18a007b91e6e8ac0502104dd63f2fd0802ff4425eef750f22cf64046084521f987c9f51aa782c377daaa821dfcf69e15a6698761f3616b9de6ed1e0d.proof"
);

#[provekit::sugar(
    concept = "concept:http-request",
    library = "reqwest",
    version = "0.12",
    family = "concept:family:http",
    loss = ["sync-vs-async"],
)]
pub async fn http_request_status(url: &str) -> i64 {
    let response = reqwest::get(url).await.unwrap();
    response.status().as_u16() as i64
}
