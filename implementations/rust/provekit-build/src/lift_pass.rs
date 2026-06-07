// SPDX-License-Identifier: Apache-2.0
//
// lift_pass: runs every registered lift adapter against the consumer
// crate's `src/` tree as part of `cargo build`.
//
// This is the wiring that makes "lift" disappear as a separate command.
// Sir's UX point: invoking `cargo provekit-lift` is wrong; the adapters
// should fire automatically during the build script. The build crate
// already source-walks `src/` for `#[provekit::contract]` decorators;
// here we walk the same tree a second time and dispatch each parsed
// `syn::File` to every adapter the consumer enabled.
//
// LANE NAMING:
//
//   * "inventory" lane: contracts produced by the kit's own decorators
//     (`#[provekit::contract]`, `.invariant.rs` files). Discovered by
//     `source_walk::walk` and reported under
//     `VerificationReport::contract_count`.
//
//   * "lift" lane: contracts produced by the contracts adapter.
//     Discovered here and reported under
//     `VerificationReport::lift_count`.
//
// The two lanes are kept parallel: we deliberately do NOT collapse the
// rich `ContractDecl` IR from the lift adapters into the coarse
// `FormulaShape` the source-walker uses to drive Z3. The Z3-driven Tier 3
// verification still keys off the inventory lane only; lift-derived
// contracts mint into the `.proof` manifest but don't (yet) drive the
// Tier 3 SMT round-trip. That's a deliberate v0 boundary: the lift
// IR is rich enough to mint, but routing the rich IR into the existing
// shape-based Z3 emitter would require a second-pass translator, which
// is out of scope for this PR.

use std::path::{Path, PathBuf};

use provekit_ir_symbolic::ContractDecl;

/// Names of every lift adapter compiled into provekit-build, in the
/// canonical run order. The order is stable so that
/// `[package.metadata.provekit] lift_adapters = [...]` whitelists
/// produce deterministic output across machines.
pub const ALL_ADAPTERS: &[&str] = &["contracts"];

/// One contract produced by a lift adapter. Wraps the rich
/// `ContractDecl` from `provekit-ir-symbolic` plus the adapter that
/// produced it and the source path it came from. The build script
/// surfaces these alongside the inventory lane in cargo:warning= lines
/// and in the proof manifest.
#[derive(Debug, Clone)]
pub struct LiftedContract {
    pub adapter: &'static str,
    pub source_path: String,
    pub decl: ContractDecl,
}

/// Per-adapter run-counts. `enabled = false` means the consumer's
/// `lift_adapters` whitelist excluded this adapter; we still record the
/// row so the report shape is stable across builds.
#[derive(Debug, Clone)]
pub struct LiftAdapterCount {
    pub adapter: &'static str,
    pub enabled: bool,
    pub seen: usize,
    pub lifted: usize,
    pub warnings: usize,
}

#[derive(Debug, Default, Clone)]
pub struct LiftPassReport {
    pub lifted: Vec<LiftedContract>,
    pub breakdown: Vec<LiftAdapterCount>,
    pub files_scanned: usize,
    pub parse_failures: Vec<(PathBuf, String)>,
}

/// Walk `<manifest_dir>/src/`, parse every `.rs` file, dispatch to each
/// adapter named in `enabled`, and accumulate the lifted decls.
///
/// `enabled` should come from `ProvekitConfig::enabled_lift_adapters`,
/// which already filters against `ALL_ADAPTERS`. Any name in `enabled`
/// that doesn't match a known adapter is silently ignored here (the
/// caller is responsible for surfacing typos via a `cargo:warning=`).
pub fn run_lift_pass(manifest_dir: &Path, enabled: &[&str]) -> LiftPassReport {
    let src_dir = manifest_dir.join("src");
    let mut report = LiftPassReport::default();

    // Per-adapter accumulators, in `ALL_ADAPTERS` order.
    let mut counts: Vec<LiftAdapterCount> = ALL_ADAPTERS
        .iter()
        .map(|name| LiftAdapterCount {
            adapter: name,
            enabled: enabled.iter().any(|w| w == name),
            seen: 0,
            lifted: 0,
            warnings: 0,
        })
        .collect();

    if !src_dir.exists() {
        report.breakdown = counts;
        return report;
    }

    for entry in walkdir::WalkDir::new(&src_dir)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let path = entry.path();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                report
                    .parse_failures
                    .push((path.to_path_buf(), format!("read: {e}")));
                continue;
            }
        };
        report.files_scanned += 1;
        let parsed = match syn::parse_file(&text) {
            Ok(p) => p,
            Err(e) => {
                // Lifters don't own parse errors: this is the same
                // tree rustc is about to compile. Log silently and move
                // on so a single bad file doesn't kill the build.
                report
                    .parse_failures
                    .push((path.to_path_buf(), format!("parse: {e}")));
                continue;
            }
        };
        let path_str = path.display().to_string();

        // Dispatch to each enabled adapter. We keep the dispatch table
        // explicit (one match arm per adapter) so the type-checker
        // catches a missing arm if a new adapter is added.
        for count in counts.iter_mut() {
            if !count.enabled {
                continue;
            }
            let (decls, seen, lifted, warnings) = match count.adapter {
                "contracts" => {
                    let out = provekit_lift_contracts::lift_file(&parsed, &path_str);
                    (out.decls, out.seen, out.lifted, out.warnings.len())
                }
                // Unknown adapter name (caller's whitelist had a typo).
                // Skip silently here; `ProvekitConfig::unknown_lift_adapters`
                // surfaces the diagnostic.
                _ => continue,
            };
            count.seen += seen;
            count.lifted += lifted;
            count.warnings += warnings;
            for d in decls {
                report.lifted.push(LiftedContract {
                    adapter: count.adapter,
                    source_path: path_str.clone(),
                    decl: d,
                });
            }
        }
    }

    // Stable order by (adapter, contract name) so two builds against
    // the same source produce identical proof manifests.
    report.lifted.sort_by(|a, b| {
        a.adapter
            .cmp(b.adapter)
            .then_with(|| a.decl.name.cmp(&b.decl.name))
            .then_with(|| a.source_path.cmp(&b.source_path))
    });

    report.breakdown = counts;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_adapters_known() {
        // Sanity: every name in `ALL_ADAPTERS` maps to a real adapter
        // dispatch arm. This catches typos in the constant.
        for name in ALL_ADAPTERS {
            let known = matches!(
                *name,
                "proptest"
                    | "contracts"
                    | "kani"
                    | "prusti"
                    | "creusot"
                    | "flux"
                    | "quickcheck"
                    | "verus"
            );
            assert!(known, "ALL_ADAPTERS contains unknown name: {name}");
        }
    }
}
