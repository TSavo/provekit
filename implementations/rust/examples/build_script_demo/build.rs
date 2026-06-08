// SPDX-License-Identifier: Apache-2.0
//
// build.rs for the ProvekIt build-script integration demo.
//
// One line of glue: `sugar_build::run_verification()`. The function
// reads `[package.metadata.provekit]` from this crate's Cargo.toml,
// source-walks `src/`, mints a `<cid>.proof` under the build's
// per-crate `OUT_DIR`, and invokes Z3 (via `PROVEKIT_Z3_PATH` or `z3`
// on PATH) on each `#[provekit::verify]` body's call sites.
//
// In `strict = true` mode, an unsatisfied call site exits non-zero,
// which cargo treats as a build failure. With `strict = false` (the
// demo's default), the same finding surfaces as a `cargo:warning=`
// line that you can see in your build output.

fn main() {
    sugar_build::run_verification();
}
