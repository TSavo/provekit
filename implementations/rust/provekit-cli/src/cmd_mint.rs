// SPDX-License-Identifier: Apache-2.0
//
// `provekit mint`: the lift-plugin protocol dispatcher.
//
// Architecture (substrate-as-only-mint-pipeline):
//
//   One Rust CLI; N language kits. The CLI is the sole mint pipeline for
//   every kit: including the rust kit itself. Rust is NOT special-cased.
//   Every kit exposes a lifter binary that speaks the lift-protocol RPC
//   (`initialize` + `lift`). The CLI drives that RPC, receives a
//   `proof-envelope` response, and then:
//
//     1. Writes the `.proof` file to the output directory (same as before).
//     2. Signs a self-contracts attestation (letter-envelope format, per
//        spec #94 / `protocol/specs/2026-05-02-bundle-attestation-protocol.md`)
//        and writes it to
//        `<repo-root>/.provekit/self-contracts-attestations/<kit>.json`.
//
//   The dogfood invariant: ProvekIt's `prove` verifies each kit satisfies
//   the canonical contracts minted by the rust kit. The substrate proves
//   the kits; the kits prove the substrate.
//
//   The lift protocol (`initialize` + `lift`) is distinct from the LSP
//   parse protocol (`initialize` + `parse`). The former is for mint; the
//   latter is for editor diagnostics. This dispatcher calls the lifter,
//   NOT the LSP.
//
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md (draft for v1.2.0).
//       protocol/specs/2026-05-02-bundle-attestation-protocol.md
//       spec #94 §2 (contractSetCid in signed body)
//
// Response shapes: `proof-envelope` and `ir-document` are supported in v1.
// Shape (b) `signed-mementos` is spec'd but unimplemented; adding it is
// additive, requires no client breakage.
//
// Missing-lifter behavior: when a manifest declares a binary that does
// not exist yet (ENOENT on spawn), mint produces a well-formed
// attestation with contractSetCid = EMPTY_SET_CID (the BLAKE3-512 of
// JCS(`[]`)). This surfaces the gap at the per-kit lifter level without
// failing the substrate pipeline. Any other spawn failure (wrong
// permissions, exit > 0) is a hard error.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use serde_json::{json, Value};

use libprovekit::core::{
    address, Boundary, Cid, Dialect, Domain, DomainClaim, DomainKind, FunctionContractDomain,
    HashMapInputCatalog, Input, InputCatalog, Kit, KitError, Path as CorePath, PathAlgebra,
    PathDocument, Term, Verb, Verdict,
};
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_claim_envelope::{
    compute_contract_set_cid, contract_cid, mint_authority, mint_bridge, mint_contract,
    mint_implication, Authoring, MintAuthorityArgs, MintBridgeArgs, MintContractArgs,
    MintImplicationArgs,
};
use provekit_ir_types::Sort;
use provekit_mint_amp as algebraic_mint;
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed,
    ProofEnvelopeInput,
};

use crate::lift_plugin::{self, LiftPluginError, LiftPluginOptions};
use crate::project_config::{read_project_config, read_user_config, PluginEntry};
use crate::OutputFlags;
use crate::{EXIT_OK, EXIT_USER_ERROR, EXIT_VERIFY_FAIL};

// ---------------------------------------------------------------------------
// Foundation signing constants
// ---------------------------------------------------------------------------

/// The v0 foundation seed. PUBLICLY KNOWN. Same seed used by foundation-keygen.
const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

/// Pinned `declaredAt` for self-contracts attestations minted under the
/// unified pipeline. Matches the v1.6.0 catalog declared_at for consistency.
const SELF_CONTRACTS_DECLARED_AT: &str = "2026-05-05T18:00:00Z";

/// Canonical mapping from `--kit=<name>` to (project_subdir, lift_surface, lang_key).
///
/// * `project_subdir`: path segment under `implementations/` (the project root passed to the lifter)
/// * `lift_surface`: subdirectory name under `.provekit/lift/<surface>/` (the manifest to load)
/// * `lang_key`: the `lang` field in the signed attestation JSON (and the
///   key for the `.provekit/self-contracts-attestations/<lang>.json` filename)
///
/// Naming diverges for several kits:
///   `ts`     → project dir `typescript`,  surface `typescript`,             lang `ts`
///   `csharp` → project dir `csharp`,      surface `csharp`,                 lang `csharp`
///   `clr-bytecode` → project dir `csharp`, surface `clr-bytecode`,           lang `clr-bytecode`
///   `evm-bytecode` → project dir `rust`,  surface `evm-bytecode`,            lang `evm-bytecode`
///   `rust`   → project dir `rust`,        surface `rust-self-contracts`,    lang `rust`
///   `go`     → project dir `go`,          surface `go-self-contracts`,      lang `go`
///
/// `--kit=rust` and `--kit=go` route to their self-contracts surfaces (which
/// invoke the slab-walking mint binaries) rather than the generic
/// workspace lifters (`provekit-lift` for rust, the test-fixture lifter for go).
/// Without this, `make mint-rust` / `make mint-go` walk the wrong source and
/// produce content-empty CIDs. The `--project=implementations/<lang>
/// --surface=<lang>` form still reaches the workspace/test-fixture lifters
/// for tooling that needs them.
/// Fix: issue #176 Tier 1, option (c): route every kit to its
/// self-contracts lifter (PR #180 for go, PR #183 for rust).
pub(crate) const KIT_TABLE: &[(&str, &str, &str, &str)] = &[
    // (kit_alias, project_subdir, lift_surface,           lang_key)
    ("rust", "rust", "rust-self-contracts", "rust"),
    ("go", "go", "go-self-contracts", "go"),
    ("cpp", "cpp", "cpp-self-contracts", "cpp"),
    ("ts", "typescript", "typescript-self-contracts", "ts"),
    ("csharp", "csharp", "csharp", "csharp"),
    ("clr-bytecode", "csharp", "clr-bytecode", "clr-bytecode"),
    ("evm-bytecode", "rust", "evm-bytecode", "evm-bytecode"),
    ("swift", "swift", "swift-self-contracts", "swift"),
    ("java", "java", "java-self-contracts", "java"),
    ("python", "python", "python-self-contracts", "python"),
    ("ruby", "ruby", "ruby-self-contracts", "ruby"),
    ("zig", "zig", "zig-self-contracts", "zig"),
    ("c", "c", "c-self-contracts", "c"),
    ("php", "php", "php-self-contracts", "php"),
];

/// Resolve `--kit=<name>` to the canonical project path, lift surface, and lang key.
/// Returns `(project_path, surface, lang_key)` relative to the CWD at
/// which `provekit` is invoked (expected to be repo root).
pub(crate) fn resolve_kit(kit: &str) -> Option<(PathBuf, String, String)> {
    KIT_TABLE
        .iter()
        .find(|(alias, _, _, _)| *alias == kit)
        .map(|(_, subdir, surface, lang)| {
            (
                PathBuf::from("implementations").join(subdir),
                surface.to_string(),
                lang.to_string(),
            )
        })
}

/// Result of a successful mint transform.
#[derive(Debug, Clone)]
struct DispatchResult {
    filename_cid: String,
    contract_set_cid: String,
    bytes_written: usize,
    proof_file: Option<PathBuf>,
    lift_result: Value,
}

/// One per-plugin response collected during multi-plugin dispatch. The
/// `surface` is carried for diagnostics; the `response` is the raw
/// JSON-RPC result the plugin returned (either `kind: "ir-document"` or
/// `kind: "proof-envelope"` per the lift-plugin protocol).
#[derive(Debug, Clone)]
struct PerPluginDispatch {
    surface: String,
    response: Value,
}

/// Merge N per-plugin lift responses into one canonical `kind:
/// "ir-document"` value. The union concatenates each plugin's `ir`
/// array; diagnostics likewise. Every plugin in a multi-plugin path
/// MUST emit `kind: "ir-document"` — proof-envelope responses are
/// already self-signed bundles and can't be folded into a fresh mint.
/// The substrate-honest failure is to reject the mix loudly.
///
/// Cross-plugin name collisions are deduplicated by name. With
/// content-addressed names (CID-suffixed by each plugin's lifter),
/// a name collision means byte-identical canonical IR, which is
/// safe to dedup — same identity, same content, same minted memento
/// downstream. The same primitive `mint_proof` uses internally.
fn merge_ir_document_responses(per_plugin: Vec<PerPluginDispatch>) -> Result<Value, String> {
    let mut merged_ir: Vec<Value> = Vec::new();
    let mut merged_diagnostics: Vec<Value> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in per_plugin {
        let kind = entry
            .response
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if kind != "ir-document" {
            return Err(format!(
                "multi-plugin mint requires every lift plugin to emit `kind: \"ir-document\"`; \
                 plugin for surface `{}` emitted `kind: \"{}\"`",
                entry.surface, kind
            ));
        }
        if let Some(arr) = entry.response.get("ir").and_then(|v| v.as_array()) {
            for item in arr {
                // Entries with a `name` field are deduped by it
                // (content-addressed by construction). Entries without
                // a `name` — refusal-memento, bind-lift-entry — pass
                // through unfiltered since their identity is structural.
                let dedup_key: Option<String> = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                match dedup_key {
                    Some(key) => {
                        if seen_names.insert(key) {
                            merged_ir.push(item.clone());
                        }
                    }
                    None => merged_ir.push(item.clone()),
                }
            }
        }
        if let Some(arr) = entry.response.get("diagnostics").and_then(|v| v.as_array()) {
            merged_diagnostics.extend(arr.iter().cloned());
        }
    }
    Ok(json!({
        "kind": "ir-document",
        "ir": merged_ir,
        "diagnostics": merged_diagnostics,
    }))
}

#[derive(Debug, Clone, Default)]
struct MintKit {
    inputs: HashMapInputCatalog,
}

#[derive(Debug, Clone)]
struct MintSession {
    claim: DomainClaim,
    result: DispatchResult,
    surface: String,
    out_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct MintPathInput {
    input: Input,
    inputs: HashMapInputCatalog,
}

impl MintKit {
    fn new(inputs: HashMapInputCatalog) -> Self {
        Self { inputs }
    }

    fn path<'a>(&self, input: &'a Input) -> Result<&'a CorePath, KitError> {
        let Input::Path(path) = input else {
            return Err(KitError::UnsupportedInput {
                dialect: self.dialect(),
                message: "mint expects Input::Path containing the composed mint algebra"
                    .to_string(),
            });
        };
        Ok(path.as_ref())
    }

    fn transform_session(&self, input: &Input) -> Result<MintSession, KitError> {
        let path = self.path(input)?;
        let ordered_steps = path
            .ordered_steps()
            .map_err(|error| KitError::Transformation(error.to_string()))?;
        let mint_step = path
            .terminal_steps()
            .into_iter()
            .find(|step| step.name == "mint" || step.kit == "provekit-mint")
            .ok_or_else(|| {
                KitError::Transformation("mint path missing terminal `mint` step".to_string())
            })?;
        // Collect ALL lift-plugin predecessors of the mint step. The path
        // executor handles arbitrary dependency fan-in; the substrate's
        // multi-plugin orchestration is just N lift steps + 1 mint step,
        // with `depends_on` carrying the dependency structure. Each lift
        // step represents one `[[plugins]]` entry from config.toml.
        let lift_steps: Vec<&PathAlgebra> = ordered_steps
            .iter()
            .copied()
            .filter(|step| {
                mint_step.depends_on.iter().any(|name| name == &step.name)
                    && step.kit.starts_with("lift-plugin:")
            })
            .collect();
        if lift_steps.is_empty() {
            return Err(KitError::Transformation(
                "mint path terminal step must depend on at least one lift-plugin step".to_string(),
            ));
        }

        let mint_request = self.path_step_spec(mint_step, "mint path mint step")?;
        // The project root (where `.provekit/` lives) is the canonical
        // location for manifest discovery, regardless of any per-plugin
        // workspace_override. Read it from the mint_request so it stays
        // stable across all lift steps in the path.
        let project_root_for_manifests = PathBuf::from(
            required_str(&mint_request, "projectRoot", "mint path mint step")
                .map_err(KitError::Transformation)?,
        );
        let out_dir = PathBuf::from(
            required_str(&mint_request, "outDir", "mint path mint step")
                .map_err(KitError::Transformation)?,
        );
        let quiet = mint_request
            .get("options")
            .and_then(|options| options.get("quiet"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Per-plugin dispatch. Each lift step contributes its own
        // lift_request (workspace_root, surface, options) and its own
        // RPC dispatch. Responses are accumulated and (when N > 1)
        // merged into a single canonical ir-document before being
        // minted into ONE envelope by the existing minter.
        let mut per_plugin: Vec<PerPluginDispatch> = Vec::with_capacity(lift_steps.len());
        let mut combined_lift_claim: Option<DomainClaim> = None;
        let mut surface_for_session: Option<String> = None;
        for lift_step in &lift_steps {
            let lift_request = self.path_step_spec(lift_step, "mint path lift step")?;
            let surface = required_str(&lift_request, "surface", "mint path lift step")
                .map_err(KitError::Transformation)?
                .to_string();
            // Reconstruct the per-plugin LiftPluginOptions from the
            // lift_request. workspace_override is carried in
            // `options.workspaceOverride` (set by build_lift_params on
            // initial construction). Restoring it lets dispatch_lift's
            // internal build_lift_params re-derive the same
            // workspace_root the plugin originally received, while
            // find_manifest correctly uses project_root_for_manifests
            // for the .provekit/ lookup.
            let lift_options = LiftPluginOptions {
                identify_only: lift_request
                    .get("options")
                    .and_then(|options| options.get("identifyOnly"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                library_bindings: lift_request
                    .get("options")
                    .and_then(|options| options.get("layer"))
                    .and_then(Value::as_str)
                    .is_some_and(|layer| layer == "library-bindings"),
                workspace_override: lift_request
                    .get("options")
                    .and_then(|options| options.get("workspaceOverride"))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                emit: lift_request
                    .get("options")
                    .and_then(|options| options.get("emit"))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                layer: lift_request
                    .get("options")
                    .and_then(|options| options.get("layer"))
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
            };

            // The first lift step's surface becomes the session's
            // canonical (used for the self-contracts attestation file).
            if surface_for_session.is_none() {
                surface_for_session = Some(surface.clone());
            }

            let session = match lift_plugin::dispatch_lift(
                &project_root_for_manifests,
                &surface,
                lift_options,
                quiet,
            ) {
                Ok(session) => session,
                Err(LiftPluginError::MissingBinary { binary }) => {
                    if !quiet {
                        println!(
                            "{}: lifter binary `{}` not found: producing empty-set attestation",
                            "warn".yellow().bold(),
                            binary
                        );
                    }
                    let empty_cid = compute_contract_set_cid(vec![]);
                    let result = DispatchResult {
                        filename_cid: String::new(),
                        contract_set_cid: empty_cid,
                        bytes_written: 0,
                        proof_file: None,
                        lift_result: json!({
                            "kind": "empty-set",
                            "reason": "lifter binary not found",
                            "binary": binary,
                        }),
                    };
                    let claim = mint_result_claim(input, None, &result)?;
                    return Ok(MintSession {
                        claim,
                        result,
                        surface,
                        out_dir,
                    });
                }
                Err(LiftPluginError::Refused(refusal)) => {
                    return Err(KitError::Transformation(format!(
                        "{}: {}",
                        refusal.header.failure_kind, refusal.header.failure_detail
                    )))
                }
                Err(LiftPluginError::Failed(error)) => return Err(KitError::Transformation(error)),
            };

            let response = session.response().clone();
            // Carry forward the first plugin's lift_claim as the
            // session's lift claim. (Future: aggregate claims into a
            // composite — out of scope for the multi-plugin landing.)
            if combined_lift_claim.is_none() {
                combined_lift_claim = Some(session.claim);
            }
            per_plugin.push(PerPluginDispatch { surface, response });
        }

        let merged_lift_response = if per_plugin.len() == 1 {
            // Single-plugin path: pass the response through unchanged so
            // proof-envelope and ir-document both work as before.
            per_plugin.into_iter().next().unwrap().response
        } else {
            // Multi-plugin path: every plugin MUST emit `kind:
            // "ir-document"`. proof-envelope responses can't be merged
            // (they're already self-signed bundles); the substrate-honest
            // failure is to reject the mix loudly.
            merge_ir_document_responses(per_plugin).map_err(KitError::Transformation)?
        };
        let result = mint_lift_response(
            &project_root_for_manifests,
            &out_dir,
            quiet,
            merged_lift_response,
        )
        .map_err(KitError::Transformation)?;
        let claim = mint_result_claim(input, combined_lift_claim.as_ref(), &result)?;
        Ok(MintSession {
            claim,
            result,
            surface: surface_for_session.expect("invariant: at least one lift step dispatched"),
            out_dir,
        })
    }

    fn path_step_spec(&self, step: &PathAlgebra, context: &str) -> Result<Value, KitError> {
        let cid = step
            .inputs
            .first()
            .ok_or_else(|| KitError::UnsupportedInput {
                dialect: Dialect::Other(step.kit.clone()),
                message: format!("{context} must carry at least one input CID"),
            })?;
        match self.inputs.get_input(cid) {
            Some(Input::Spec(value)) => Ok(value.clone()),
            Some(_) => Err(KitError::UnsupportedInput {
                dialect: Dialect::Other(step.kit.clone()),
                message: format!("{context} input `{cid}` must resolve to Input::Spec"),
            }),
            None => Err(KitError::UnsupportedInput {
                dialect: Dialect::Other(step.kit.clone()),
                message: format!("{context} input `{cid}` is not materialized"),
            }),
        }
    }
}

impl Kit for MintKit {
    fn dialect(&self) -> Dialect {
        Dialect::Other("provekit-mint".to_string())
    }

    fn transform(&self, input: &Input) -> Result<DomainClaim, KitError> {
        self.transform_session(input).map(|session| session.claim)
    }

    fn prove(&self, claim: DomainClaim) -> Result<DomainClaim, KitError> {
        Ok(claim)
    }

    fn parse(&self, input: &Input) -> Result<Term, KitError> {
        let session = self.transform_session(input)?;
        Ok(Term::Const {
            value: dispatch_result_to_value(&session.result),
            sort: Sort::Primitive {
                name: "MintResult".to_string(),
            },
        })
    }

    fn serialize(&self, term: &Term) -> Result<Input, KitError> {
        Ok(Input::Term(term.clone()))
    }
}

fn dispatch(
    project_root: &Path,
    surface: &str,
    out_dir: &Path,
    quiet: bool,
    library_bindings: bool,
) -> Result<MintSession, String> {
    let mint_input = mint_input(project_root, surface, out_dir, quiet, library_bindings);
    MintKit::new(mint_input.inputs)
        .transform_session(&mint_input.input)
        .map_err(|error| error.to_string())
}

/// Multi-plugin dispatch: builds a fan-in mint path with N lift steps
/// (one per declared `[[plugins]]` entry) feeding into one mint terminal
/// step. Delegates to the same `MintKit::transform_session` as
/// single-plugin dispatch — the substrate's path executor and the
/// MintKit's predecessor-fan-in logic handle the rest. The user-facing
/// wrapper for projects whose `.provekit/config.toml` declares
/// `[[plugins]]`.
fn dispatch_multi(
    project_root: &Path,
    plugins: &[PluginEntry],
    out_dir: &Path,
    quiet: bool,
    library_bindings: bool,
) -> Result<MintSession, String> {
    let mint_input = mint_input_multi(project_root, plugins, out_dir, quiet, library_bindings);
    MintKit::new(mint_input.inputs)
        .transform_session(&mint_input.input)
        .map_err(|error| error.to_string())
}

fn dispatch_path(project_root: &Path, path_file: &Path) -> Result<MintSession, String> {
    let path = path_under(project_root, path_file);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read mint path document {}: {error}", path.display()))?;
    let document: PathDocument = serde_json::from_str(&text)
        .map_err(|error| format!("parse mint path document {}: {error}", path.display()))?;
    let mut inputs = HashMapInputCatalog::default();
    for (cid, input) in document
        .materialized_inputs()
        .map_err(|error| format!("invalid mint path document {}: {error}", path.display()))?
    {
        inputs.put(cid, input);
    }
    MintKit::new(inputs)
        .transform_session(&Input::Path(Box::new(document.path)))
        .map_err(|error| error.to_string())
}

fn path_under(project_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn mint_input(
    project_root: &Path,
    surface: &str,
    out_dir: &Path,
    quiet: bool,
    library_bindings: bool,
) -> MintPathInput {
    let entry = PluginEntry {
        name: None,
        surface: surface.to_string(),
        workspace_override: None,
        emit: None,
        layer: None,
    };
    mint_input_multi(
        project_root,
        std::slice::from_ref(&entry),
        out_dir,
        quiet,
        library_bindings,
    )
}

/// Build a mint path with N lift steps (one per declared `[[plugins]]`
/// entry from config.toml) feeding into a single mint terminal step.
/// The path executor walks each lift step's `Kit::transform(Input) ->
/// DomainClaim` independently; mint depends on all of them by name and
/// collects/merges their outputs at the envelope mint stage. This is
/// the substrate's path-native answer to multi-plugin orchestration:
/// the dispatch lives in the path algebra, not in side-channel CLI
/// loops. Single-surface callers route here with a 1-element slice.
fn mint_input_multi(
    project_root: &Path,
    plugins: &[PluginEntry],
    out_dir: &Path,
    quiet: bool,
    library_bindings: bool,
) -> MintPathInput {
    let mut inputs = HashMapInputCatalog::default();
    let mut algebra: Vec<PathAlgebra> = Vec::with_capacity(plugins.len() + 1);
    let mut lift_step_names: Vec<String> = Vec::with_capacity(plugins.len());

    for (idx, plugin) in plugins.iter().enumerate() {
        let lift_input = Input::Spec(lift_plugin::build_lift_params(
            project_root,
            &plugin.surface,
            LiftPluginOptions {
                identify_only: false,
                library_bindings,
                workspace_override: plugin.workspace_override.clone(),
                emit: plugin.emit.clone(),
                layer: plugin.layer.clone(),
            },
        ));
        let lift_input_cid = address(&lift_input);
        inputs.put(lift_input_cid.clone(), lift_input);
        let lift_step_name = if plugins.len() == 1 {
            // Preserve the historic single-step name `lift` so any
            // path-document fixtures or external tooling keyed on it
            // keep working.
            "lift".to_string()
        } else {
            format!("lift_{idx}")
        };
        algebra.push(PathAlgebra {
            name: lift_step_name.clone(),
            kit: format!("lift-plugin:{}", plugin.surface),
            inputs: vec![lift_input_cid],
            depends_on: vec![],
            verb: Verb::Transform,
        });
        lift_step_names.push(lift_step_name);
    }

    let surface_for_mint = plugins
        .first()
        .map(|p| p.surface.clone())
        .unwrap_or_default();
    let mint_input = Input::Spec(json!({
        "projectRoot": project_root.display().to_string(),
        "surface": surface_for_mint,
        "outDir": out_dir.display().to_string(),
        "options": {
            "quiet": quiet
        }
    }));
    let mint_input_cid = address(&mint_input);
    inputs.put(mint_input_cid.clone(), mint_input);

    algebra.push(PathAlgebra {
        name: "mint".to_string(),
        kit: "provekit-mint".to_string(),
        inputs: vec![mint_input_cid],
        depends_on: lift_step_names,
        verb: Verb::Transform,
    });

    MintPathInput {
        input: Input::Path(Box::new(CorePath { algebra })),
        inputs,
    }
}

fn mint_lift_response(
    project_root: &Path,
    out_dir: &Path,
    quiet: bool,
    lift_resp: Value,
) -> Result<DispatchResult, String> {
    let kind = lift_resp
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or("lift response missing `kind` field")?;
    match kind {
        "proof-envelope" => {
            let filename_cid = lift_resp
                .get("filename_cid")
                .and_then(|v| v.as_str())
                .ok_or("missing filename_cid")?
                .to_string();
            let contract_set_cid = lift_resp
                .get("contract_set_cid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let bytes_b64 = lift_resp
                .get("bytes_base64")
                .and_then(|v| v.as_str())
                .ok_or("missing bytes_base64")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(bytes_b64)
                .map_err(|e| format!("decode bytes_base64: {e}"))?;

            std::fs::create_dir_all(out_dir)
                .map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
            let out_path = out_dir.join(format!("{filename_cid}.proof"));
            std::fs::write(&out_path, &bytes)
                .map_err(|e| format!("write {}: {e}", out_path.display()))?;

            if !quiet {
                for d in lift_resp
                    .get("diagnostics")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    let s = d.as_str().unwrap_or("");
                    if !s.is_empty() {
                        println!("{}: {s}", "note".dimmed());
                    }
                }
            }

            Ok(DispatchResult {
                filename_cid,
                contract_set_cid,
                bytes_written: bytes.len(),
                proof_file: Some(out_path),
                lift_result: redact_lift_result(lift_resp),
            })
        }
        "ir-document" => {
            let ir = lift_resp
                .get("ir")
                .and_then(|v| v.as_array())
                .ok_or("ir-document response missing `ir` array")?;

            let authorities = lift_resp.get("authorities").and_then(|v| v.as_array());
            let implications = lift_resp.get("implications").and_then(|v| v.as_array());
            let witnesses = lift_resp.get("witnesses").and_then(|v| v.as_array());
            let (bytes, filename_cid, contract_set_cid) = mint_from_ir_document(
                ir,
                authorities,
                implications,
                witnesses,
                &project_root,
                out_dir,
                quiet,
            )?;

            std::fs::create_dir_all(out_dir)
                .map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
            let out_path = out_dir.join(format!("{filename_cid}.proof"));
            std::fs::write(&out_path, &bytes)
                .map_err(|e| format!("write {}: {e}", out_path.display()))?;

            if !quiet {
                let diags = lift_resp.get("diagnostics").and_then(|v| v.as_array());

                if let Some(diags) = diags {
                    for d in diags {
                        let s = d.as_str().unwrap_or("");
                        if !s.is_empty() {
                            println!("{}: {s}", "note".dimmed());
                        }
                    }
                }
            }

            Ok(DispatchResult {
                filename_cid,
                contract_set_cid,
                bytes_written: bytes.len(),
                proof_file: Some(out_path),
                lift_result: lift_resp,
            })
        }
        other => Err(format!(
            "unsupported response shape `{other}`; expected `proof-envelope` or `ir-document`",
        )),
    }
}

fn redact_lift_result(mut lift_resp: Value) -> Value {
    if let Some(obj) = lift_resp.as_object_mut() {
        if obj.contains_key("bytes_base64") {
            obj.insert(
                "bytes_base64".to_string(),
                Value::String("<redacted>".to_string()),
            );
        }
    }
    lift_resp
}

fn mint_result_claim(
    input: &Input,
    lift_claim: Option<&DomainClaim>,
    result: &DispatchResult,
) -> Result<DomainClaim, KitError> {
    let value = dispatch_result_to_value(result);
    let term = Term::Const {
        value,
        sort: Sort::Primitive {
            name: "MintResult".to_string(),
        },
    };
    let contract = FunctionContractDomain
        .project(&term, &Boundary::default())
        .map_err(|error| KitError::Transformation(error.to_string()))?;
    let to = if result.filename_cid.is_empty() {
        address(&term)
    } else {
        Cid::parse(result.filename_cid.clone()).unwrap_or_else(|_| address(&term))
    };
    let result_cid = address(&term);
    let premises = lift_claim
        .map(|claim| vec![claim.cid()])
        .unwrap_or_default();

    Ok(DomainClaim {
        domain: DomainKind::Other("provekit-mint".to_string()),
        contract,
        artifacts: vec![result_cid],
        from: vec![address(input)],
        premises,
        to,
        witness: None,
        payload: Some(term),
        verdict: Verdict::Unresolved,
        attestation: None,
    })
}

fn dispatch_result_to_value(result: &DispatchResult) -> Value {
    json!({
        "kind": "mint-result",
        "filenameCid": result.filename_cid,
        "contractSetCid": result.contract_set_cid,
        "bytesWritten": result.bytes_written,
        "proofFile": result.proof_file.as_ref().map(|path| path.display().to_string()),
        "lift": result.lift_result,
    })
}

// ---------------------------------------------------------------------------
// ir-document → proof-envelope minting
// ---------------------------------------------------------------------------

/// #1358 / #1355: Fill `family` and `library_version` on each IR entry from
/// the project's platform_profile when the entry doesn't already pin those
/// axes via @sugar / @boundary annotation. ANNOTATION WINS: an entry whose
/// emission already includes a family or library_version (because walk_rpc
/// pulled it from the source annotation) keeps that value verbatim.
///
/// Applies to all per-concept memento kinds:
///   - library-sugar-binding-entry
///   - realization-memento
///
/// Refusal-memento is intentionally not stamped — refusals are about a
/// concept that DIDN'T close in this surface; the realization-tuple axes
/// don't apply (the realization didn't happen).
pub(crate) fn stamp_platform_profile(
    entries: &mut Vec<Value>,
    profile: &crate::project_config::PlatformProfile,
) {
    for entry in entries.iter_mut() {
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "library-sugar-binding-entry" && kind != "realization-memento" {
            continue;
        }
        let Some(obj) = entry.as_object_mut() else {
            continue;
        };
        if let Some(family) = &profile.family {
            if !obj.contains_key("family") {
                obj.insert("family".to_string(), Value::String(family.clone()));
            }
        }
        if let Some(version) = &profile.version {
            if !obj.contains_key("library_version") {
                obj.insert(
                    "library_version".to_string(),
                    Value::String(version.clone()),
                );
            }
        }
    }
}

fn mint_from_ir_document(
    ir: &[Value],
    authorities: Option<&Vec<Value>>,
    implications: Option<&Vec<Value>>,
    witnesses: Option<&Vec<Value>>,
    project_root: &Path,
    out_dir: &Path,
    quiet: bool,
) -> Result<(Vec<u8>, String, String), String> {
    use std::collections::BTreeMap;

    #[derive(Clone)]
    struct AuthorityRef {
        cid: String,
        seed: Ed25519Seed,
        principal: String,
    }

    struct MintedContractRef {
        attestation_cid: String,
        pre_hash: Option<String>,
        post_hash: Option<String>,
        inv_hash: Option<String>,
    }

    impl MintedContractRef {
        fn slot_hash(&self, slot: &str) -> Option<&str> {
            match slot {
                "pre" => self.pre_hash.as_deref(),
                "post" => self.post_hash.as_deref(),
                "inv" => self.inv_hash.as_deref(),
                _ => None,
            }
        }
    }

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut authorities_by_id: BTreeMap<String, AuthorityRef> = BTreeMap::new();
    let mut proof_authority: Option<AuthorityRef> = None;
    let mut contracts_by_name: BTreeMap<String, MintedContractRef> = BTreeMap::new();
    let mut content_cids: Vec<String> = Vec::new();
    let default_signer_seed: Ed25519Seed = FOUNDATION_V0_SEED;
    let produced_at = "2026-05-03T18:00:00Z".to_string();
    let witness_cids_by_contract =
        emit_witnesses_by_contract(witnesses, project_root, out_dir, quiet)?;

    if let Some(authorities) = authorities {
        for authority in authorities {
            let id = required_str(authority, "id", "authority")?;
            let principal = optional_str(authority, "principal").unwrap_or(id);
            let scope_kind = required_str(authority, "scopeKind", id)?;
            let scope = required_str(authority, "scope", id)?;
            let seed = deterministic_signer_seed(principal);
            let key = ed25519_pubkey_string(&seed);
            let parent_id = optional_str(authority, "parent")
                .or_else(|| optional_str(authority, "issuer"))
                .or_else(|| optional_str(authority, "parentAuthority"));
            let parent = match parent_id {
                Some(parent_id) => Some(authorities_by_id.get(parent_id).ok_or_else(|| {
                    format!("authority `{id}` references missing parent `{parent_id}`")
                })?),
                None => None,
            };
            let parent_authority_cid = parent.map(|parent| parent.cid.clone());
            let signer_seed = parent.map(|parent| parent.seed).unwrap_or(seed);
            let args = MintAuthorityArgs {
                principal: principal.to_string(),
                key: key.clone(),
                scope_kind: scope_kind.to_string(),
                scope: scope.to_string(),
                parent_authority_cid,
                produced_by: "provekit-cli".to_string(),
                produced_at: produced_at.clone(),
                signer_seed,
            };
            let minted =
                mint_authority(&args).map_err(|e| format!("mint authority `{id}`: {e}"))?;
            let authority_ref = AuthorityRef {
                cid: minted.cid.clone(),
                seed,
                principal: principal.to_string(),
            };
            if scope_kind == "proof" && proof_authority.is_none() {
                proof_authority = Some(authority_ref.clone());
            }
            if authorities_by_id
                .insert(id.to_string(), authority_ref)
                .is_some()
            {
                return Err(format!("duplicate authority `{id}`"));
            }
            members
                .entry(minted.cid.clone())
                .or_insert(minted.canonical_bytes);
        }
    }

    for decl in ir {
        let kind = decl.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "contract" && kind != "function-contract" {
            continue;
        }

        let name = decl
            .get("name")
            .or_else(|| decl.get("symbol"))
            .or_else(|| decl.get("fn_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();
        let out_binding = decl
            .get("outBinding")
            .or_else(|| decl.get("out_binding"))
            .and_then(|v| v.as_str())
            .unwrap_or("out")
            .to_string();
        let pre = decl
            .get("pre")
            .or_else(|| decl.get("precondition"))
            .map(json_to_cvalue);
        let post = decl
            .get("post")
            .or_else(|| decl.get("postcondition"))
            .map(json_to_cvalue);
        let inv = decl
            .get("inv")
            .or_else(|| decl.get("invariant"))
            .map(json_to_cvalue);

        if pre.is_none() && post.is_none() && inv.is_none() {
            continue;
        }

        // Body-derived op-contract slots (#1436/#1440): a `function-contract`
        // decl carries the function's `formals` (+ `formalSorts`), lifted by
        // walk / JavaSourceLifter from the method body. Carry them through so
        // the minted `kind:"contract"` memento's header bears them and
        // `body_discharge::CatalogResolver` can resolve the body-obligation.
        // Non-function `contract` decls have no formals; the vecs stay empty
        // and the minted bytes are unchanged.
        let formals_json = decl.get("formals").and_then(|v| v.as_array());
        let formals: Vec<String> = formals_json
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let formal_sorts: Vec<std::sync::Arc<provekit_canonicalizer::Value>> = decl
            .get("formalSorts")
            .or_else(|| decl.get("formal_sorts"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(json_to_cvalue).collect())
            .unwrap_or_default();
        // A bridge is written only when this contract is a body-bearing
        // function target: it carries a `post` AND an explicit `formals`
        // field. Presence is the marker, not non-emptiness: zero-arg
        // functions carry `formals: []` and are still body-bearing. The
        // bridge's `sourceSymbol` is the function's bare name as it appears
        // in harvested call ctors. For a v1 function contract the harvested
        // ctor uses the bare ident, so prefer the explicit
        // `bridgeSourceSymbol` if the lifter set one, else the function's
        // simple name.
        let bridge_source_symbol: Option<String> =
            if kind == "function-contract" && post.is_some() && formals_json.is_some() {
                Some(
                    decl.get("bridgeSourceSymbol")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| simple_function_symbol(&name)),
                )
            } else {
                None
            };
        let authority = optional_str(decl, "authority")
            .map(|authority_id| {
                authorities_by_id.get(authority_id).ok_or_else(|| {
                    format!("contract `{name}` references missing authority `{authority_id}`")
                })
            })
            .transpose()?;
        let mut input_cids = string_array(decl, "inputCids", &name)?;
        if let Some(witness_cids) = witness_cids_by_contract.get(&name) {
            input_cids.extend(witness_cids.iter().cloned());
        }
        if let Some(authority) = authority {
            input_cids.push(authority.cid.clone());
        }
        let emit_empty_formals =
            kind == "function-contract" && formals_json.is_some() && formals.is_empty();
        let signer_seed = authority
            .map(|authority| authority.seed)
            .unwrap_or(default_signer_seed);
        let produced_by = authority
            .map(|authority| authority.principal.clone())
            .unwrap_or_else(|| "provekit-cli".to_string());

        let args = MintContractArgs {
            contract_name: name,
            pre,
            post,
            inv,
            out_binding,
            produced_by,
            produced_at: produced_at.clone(),
            input_cids,
            authoring: Authoring::Lift {
                lifter: "ir-document".to_string(),
                evidence: "minted from ir-document RPC response".to_string(),
                source_cid: None,
            },
            signer_seed,
            formals,
            emit_empty_formals,
            formal_sorts,
        };

        let ccid = contract_cid(&args);
        let pre_hash = args.pre.as_ref().map(formula_hash);
        let post_hash = args.post.as_ref().map(formula_hash);
        let inv_hash = args.inv.as_ref().map(formula_hash);
        content_cids.push(ccid.clone());

        let m = mint_contract(&args).map_err(|e| format!("mint contract: {e}"))?;

        // Production bridge-writer (#1436/#1440, PR-23): for a body-derived
        // function contract, AUTOMATICALLY mint the bridge that points a
        // harvested call at this contract's body-obligation. This is the
        // pipeline that was missing -- `bind_function_bridge` existed but no
        // production verb called it, so verify could only reach the seam via
        // hand-built test bundles. The bridge's `targetContractCid` is this
        // contract's ATTESTATION CID (`m.cid`, the member key the verifier
        // indexes `pool.mementos` by), so `CatalogResolver` resolves the
        // chain. Language-neutral: it operates on the protocol's fields, not
        // on any source language.
        if let Some(source_symbol) = bridge_source_symbol {
            let bridge = mint_bridge(&MintBridgeArgs {
                produced_by: "provekit-cli".to_string(),
                produced_at: produced_at.clone(),
                source_symbol,
                source_layer: "source".to_string(),
                target_contract_cid: m.cid.clone(),
                target_layer: "kit".to_string(),
                ir_arg_sorts: vec![],
                ir_return_sort: String::new(),
                notes: "auto-minted body-discharge bridge (PR-23)".to_string(),
                signer_seed,
            });
            members
                .entry(bridge.cid.clone())
                .or_insert(bridge.canonical_bytes);
        }

        if contracts_by_name
            .insert(
                args.contract_name.clone(),
                MintedContractRef {
                    attestation_cid: m.cid.clone(),
                    pre_hash,
                    post_hash,
                    inv_hash,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate contract `{}`", args.contract_name));
        }

        members.entry(m.cid.clone()).or_insert(m.canonical_bytes);
    }

    // #1358 / #1355: stamp the project's platform_profile onto each
    // realization-bearing IR entry so absent annotation axes get filled in
    // from the shim's single declarative profile. Annotation pins always
    // win; this only fills floating axes.
    let cfg = read_project_config(project_root);
    if let Some(profile) = cfg.platform_profile.as_ref() {
        let mut stamped: Vec<Value> = ir.iter().cloned().collect();
        stamp_platform_profile(&mut stamped, profile);
        for decl in &stamped {
            match decl.get("kind").and_then(|v| v.as_str()) {
                Some("library-sugar-binding-entry") => {
                    let (cid, bytes) = mint_library_sugar_binding_entry(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                Some("refusal-memento") => {
                    let (cid, bytes) = mint_refusal_memento(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                Some("realization-memento") => {
                    let (cid, bytes) = mint_realization_memento(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                _ => {}
            }
        }
    } else {
        for decl in ir {
            match decl.get("kind").and_then(|v| v.as_str()) {
                Some("library-sugar-binding-entry") => {
                    let (cid, bytes) = mint_library_sugar_binding_entry(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                Some("refusal-memento") => {
                    let (cid, bytes) = mint_refusal_memento(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                Some("realization-memento") => {
                    let (cid, bytes) = mint_realization_memento(decl)?;
                    members.entry(cid).or_insert(bytes);
                }
                _ => {}
            }
        }
    }

    project_body_templates_for_sugar_bindings(ir)?;

    if members.is_empty() {
        return Err("no contracts to mint".to_string());
    }

    if let Some(implications) = implications {
        for implication in implications {
            let name = implication
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed-implication");
            let antecedent_name = required_str(implication, "antecedent", name)?;
            let consequent_name = required_str(implication, "consequent", name)?;
            let antecedent_slot = optional_str(implication, "antecedentSlot").unwrap_or("post");
            let consequent_slot = optional_str(implication, "consequentSlot").unwrap_or("post");

            let antecedent = contracts_by_name.get(antecedent_name).ok_or_else(|| {
                format!("implication `{name}` references missing contract `{antecedent_name}`")
            })?;
            let consequent = contracts_by_name.get(consequent_name).ok_or_else(|| {
                format!("implication `{name}` references missing contract `{consequent_name}`")
            })?;
            let antecedent_hash = antecedent.slot_hash(antecedent_slot).ok_or_else(|| {
                format!(
                    "implication `{name}` references missing slot `{antecedent_slot}` on contract `{antecedent_name}`"
                )
            })?;
            let consequent_hash = consequent.slot_hash(consequent_slot).ok_or_else(|| {
                format!(
                    "implication `{name}` references missing slot `{consequent_slot}` on contract `{consequent_name}`"
                )
            })?;
            let authority = optional_str(implication, "authority")
                .map(|authority_id| {
                    authorities_by_id.get(authority_id).ok_or_else(|| {
                        format!(
                            "implication `{name}` references missing authority `{authority_id}`"
                        )
                    })
                })
                .transpose()?;
            let additional_input_cids = authority
                .map(|authority| vec![authority.cid.clone()])
                .unwrap_or_default();
            let signer_seed = authority
                .map(|authority| authority.seed)
                .unwrap_or(default_signer_seed);
            let produced_by = authority
                .map(|authority| authority.principal.clone())
                .unwrap_or_else(|| "provekit-cli".to_string());

            let args = MintImplicationArgs {
                produced_by,
                produced_at: produced_at.clone(),
                antecedent_hash: antecedent_hash.to_string(),
                consequent_hash: consequent_hash.to_string(),
                antecedent_cid: antecedent.attestation_cid.clone(),
                consequent_cid: consequent.attestation_cid.clone(),
                additional_input_cids,
                antecedent_slot: antecedent_slot.to_string(),
                consequent_slot: consequent_slot.to_string(),
                prover: optional_str(implication, "prover")
                    .unwrap_or("bridgeworks-white-room")
                    .to_string(),
                prover_run_ms: implication
                    .get("proverRunMs")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                smt_lib_input: optional_str(implication, "smtLibInput")
                    .unwrap_or("")
                    .to_string(),
                proof_witness: optional_str(implication, "proofWitness")
                    .unwrap_or(name)
                    .to_string(),
                signer_seed,
            };

            let m = mint_implication(&args);
            members.entry(m.cid.clone()).or_insert(m.canonical_bytes);
        }
    }

    let contract_set_cid = compute_contract_set_cid(content_cids);

    let (proof_signer, proof_signer_seed) = if let Some(authority) = proof_authority {
        (authority.cid, authority.seed)
    } else {
        (
            ed25519_pubkey_string(&default_signer_seed),
            default_signer_seed,
        )
    };

    let proof_input = ProofEnvelopeInput {
        name: "ir-document".to_string(),
        version: "1.0.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: proof_signer,
        signer_seed: proof_signer_seed,
        declared_at: produced_at,
    };

    let built = build_proof_envelope(&proof_input);

    Ok((built.bytes, built.cid, contract_set_cid))
}

fn mint_library_sugar_binding_entry(decl: &Value) -> Result<(String, Vec<u8>), String> {
    let target_language = required_str(decl, "target_language", "library-sugar-binding-entry")?;
    let target_library_tag =
        required_str(decl, "target_library_tag", "library-sugar-binding-entry")?;
    let concept_name = required_str(decl, "concept_name", "library-sugar-binding-entry")?;
    let signature_shape_cid =
        required_str(decl, "signature_shape_cid", "library-sugar-binding-entry")?;
    let body_source = decl
        .get("body_source")
        .ok_or_else(|| "`library-sugar-binding-entry` missing `body_source`".to_string())?;
    let source_cid = required_str(body_source, "source_cid", "body_source")?;

    let envelope = json!({
        "body": decl,
        "header": {
            "bodySourceCid": source_cid,
            "conceptName": concept_name,
            "kind": "library-sugar-binding-entry",
            "signatureShapeCid": signature_shape_cid,
            "targetLanguage": target_language,
            "targetLibraryTag": target_library_tag,
        },
        "schemaVersion": "1",
    });
    let canonical = encode_jcs(&json_to_cvalue(&envelope));
    let cid = blake3_512_of(canonical.as_bytes());
    Ok((cid, canonical.into_bytes()))
}

/// Project the IR's library-sugar-binding-entry records into per-(lang,
/// library) body-templates JSON files. The body-templates file is the
/// realize side's loadable supply registry: the kit binary reads
/// `menagerie/<lang>-language-signature/specs/body-templates/<lang>-canonical-bodies-<library>.json`
/// at materialize time. Every entry's `emission_template` is generated
/// here as `kind = "verbatim"` with the shim's lifted body text and the
/// shim's parameter names rewritten as `${param0}`/`${param1}`/... so
/// downstream consumers' carrier comments substitute positionally at
/// realize time.
///
/// Per paper 24 §6, this file is a deterministic projection of the
/// .proof envelope's signed mementos. It is never hand-authored.
/// Anyone holding the envelope can recompute it; the only authority is
/// the kit's signature on the envelope itself.
fn project_body_templates_for_sugar_bindings(ir: &[Value]) -> Result<(), String> {
    use std::collections::BTreeMap;

    // Group library-sugar-binding-entry records by (target_language,
    // target_library_tag). Each group becomes one body-templates JSON.
    let mut grouped: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
    for decl in ir {
        if decl.get("kind").and_then(|v| v.as_str()) != Some("library-sugar-binding-entry") {
            continue;
        }
        let lang = decl
            .get("target_language")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let libtag = decl
            .get("target_library_tag")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if lang.is_empty() || libtag.is_empty() {
            continue;
        }
        // Kits that self-resolve shim proofs do not use the on-disk
        // canonical-bodies JSON cache: their realize kits resolve emission
        // bodies from package-manager-installed shim `.proof` files, and
        // `body_template_cid` content-addresses the kit-returned entries
        // directly. Projecting a JSON for those kits would re-introduce the
        // central registry the kit-resolution model removed.
        if lang == "typescript"
            || (lang == "python" && matches!(libtag.as_str(), "sqlite3" | "aiosqlite" | "requests"))
        {
            continue;
        }
        grouped.entry((lang, libtag)).or_default().push(decl);
    }

    if grouped.is_empty() {
        return Ok(());
    }

    // Locate the repo root by walking up from CWD to find the
    // `menagerie/` directory; the body-templates file lives under
    // `menagerie/<lang>-language-signature/specs/body-templates/`.
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let repo_root = locate_menagerie_root(&cwd)?;

    for ((lang, libtag), decls) in grouped {
        let mut entries: Vec<Value> = Vec::with_capacity(decls.len());
        for decl in &decls {
            // #1361 follow-up: the per-entry transform is factored into
            // `binding_entry_to_template_entry` so `provekit materialize`
            // can derive the SAME emission-template entries directly from a
            // shim's .proof (RPC-fed templates) without round-tripping
            // through the on-disk projection cache. The disk-write projector
            // here and the materialize path share one transform.
            if let Some(entry) = binding_entry_to_template_entry(decl, &libtag)? {
                entries.push(entry);
            }
        }
        if entries.is_empty() {
            continue;
        }

        let header_content = serde_json::json!({
            "target_language": lang,
            "template_name": format!("{lang}-canonical-bodies-{libtag}"),
            "entries": entries,
        });
        // Compute header.cid as blake3-512 of the JCS-canonicalized header
        // content. cmd_bind_migrate requires this when reading body-templates;
        // its absence was a pre-existing latent bug in the projection.
        let content_jcs = encode_jcs(&json_to_cvalue(&header_content));
        let header_cid = blake3_512_of(content_jcs.as_bytes());
        let doc = serde_json::json!({
            "header": { "cid": header_cid, "content": header_content },
        });

        let out_path = repo_root
            .join("menagerie")
            .join(format!("{lang}-language-signature"))
            .join("specs")
            .join("body-templates")
            .join(format!("{lang}-canonical-bodies-{libtag}.json"));
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let body =
            serde_json::to_string_pretty(&doc).map_err(|e| format!("serialize: {e}"))? + "\n";
        std::fs::write(&out_path, body)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    }
    Ok(())
}

/// Transform a single `library-sugar-binding-entry` IR record into the
/// emission-template entry shape that realize kits consume (the same shape
/// written into `<lang>-canonical-bodies-<tag>.json`). Returns `Ok(None)`
/// when the entry has no body text (nothing to emit).
///
/// `libtag` is the resolved target library tag; it stamps the entry so a
/// multi-library template cache can disambiguate. The shim's source param
/// names in `body_text` are rewritten to `${param0}`/`${param1}`/... so the
/// consumer's carrier substitutes positionally at realize time — this is
/// the load-bearing step that makes the template library-agnostic.
///
/// This transform is used by the on-disk projector retained for legacy
/// body-template JSON generation. Runtime kit realization resolves shim
/// proofs inside each language kit.
pub(crate) fn binding_entry_to_template_entry(
    decl: &Value,
    libtag: &str,
) -> Result<Option<Value>, String> {
    let concept_name = decl
        .get("concept_name")
        .and_then(|v| v.as_str())
        .ok_or("library-sugar-binding-entry missing concept_name")?;
    let param_names: Vec<String> = decl
        .get("param_names")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let body_text = decl
        .get("body_source")
        .and_then(|bs| bs.get("body_text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if body_text.is_empty() {
        return Ok(None);
    }
    let template = substitute_shim_params_with_placeholders(body_text, &param_names);
    let loss = decl
        .get("loss_record_contribution")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"form": "literal", "value": {"entries": []}}));
    let arity = param_names.len();
    let mut entry = serde_json::json!({
        "concept_name": concept_name,
        "emission_template": { "kind": "verbatim", "template": template },
        "loss_record_contribution": loss,
        "signature_guard": { "min_params": arity, "max_params": arity },
        "target_library_tag": libtag,
    });
    if let Some(observed) = decl.get("observed_dimension").and_then(|v| v.as_str()) {
        entry["observed_dimension"] = serde_json::Value::String(observed.to_string());
    }
    // #1390: propagate file_helpers (static field declarations) so the
    // realize plugin can emit them as `helpers` in the invoke response.
    // The assembler later hoists them into the compilation unit.
    if let Some(helpers) = decl.get("file_helpers").cloned() {
        entry["file_helpers"] = helpers;
    }
    Ok(Some(entry))
}

fn locate_menagerie_root(start: &Path) -> Result<PathBuf, String> {
    let mut current = start.to_path_buf();
    loop {
        if current.join("menagerie").is_dir() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("could not locate workspace root containing `menagerie/`".to_string());
        }
    }
}

fn substitute_shim_params_with_placeholders(body: &str, param_names: &[String]) -> String {
    // Replace each shim param name with `${paramN}`. Whole-word match
    // via simple state-machine over `[A-Za-z_][A-Za-z0-9_]*` token
    // boundaries. The shim's wrapper bodies are short, so this
    // straightforward pass suffices.
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let is_ident_start = c.is_ascii_alphabetic() || c == b'_';
        if is_ident_start {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let ident = std::str::from_utf8(&bytes[i..j]).unwrap_or("");
            if let Some(idx) = param_names.iter().position(|p| p == ident) {
                out.push_str(&format!("${{param{idx}}}"));
            } else {
                out.push_str(ident);
            }
            i = j;
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

fn mint_refusal_memento(decl: &Value) -> Result<(String, Vec<u8>), String> {
    let target_language = required_str(decl, "target_language", "refusal-memento")?;
    let surface = required_str(decl, "surface", "refusal-memento")?;
    let concept = required_str(decl, "concept", "refusal-memento")?;
    let reason = required_str(decl, "reason", "refusal-memento")?;
    let would_close_with_cluster =
        required_str(decl, "would_close_with_cluster", "refusal-memento")?;

    if reason.trim().is_empty() {
        return Err("`refusal-memento` missing non-empty `reason`".to_string());
    }
    if would_close_with_cluster.trim().is_empty() {
        return Err("`refusal-memento` missing non-empty `would_close_with_cluster`".to_string());
    }

    let envelope = json!({
        "body": decl,
        "header": {
            "concept": concept,
            "kind": "refusal-memento",
            "surface": surface,
            "targetLanguage": target_language,
            "wouldCloseWithCluster": would_close_with_cluster,
        },
        "schemaVersion": "1",
    });
    let canonical = encode_jcs(&json_to_cvalue(&envelope));
    let cid = blake3_512_of(canonical.as_bytes());
    Ok((cid, canonical.into_bytes()))
}

/// Mint a `realization-memento` (Boundary variant) into the envelope.
/// Emitted by `walk_rpc` for each `#[provekit::boundary]` annotation
/// it finds: a function tagged as the EDGE where a concept binds to
/// a per-language library. The materializer (downstream) reads these
/// when retargeting consumers to other languages and substitutes the
/// per-target sister library at each boundary callsite. The data type
/// already exists as `RealizationMemento::Boundary` in
/// `provekit-ir-types`; here we just envelope-mint it for the .proof.
fn mint_realization_memento(decl: &Value) -> Result<(String, Vec<u8>), String> {
    let realization_kind = required_str(decl, "realization_kind", "realization-memento")?;
    if realization_kind != "boundary" {
        return Err(format!(
            "realization-memento: only `realization_kind = \"boundary\"` is currently \
             minted; got `{realization_kind}`"
        ));
    }
    let target_language = required_str(decl, "target_language", "realization-memento")?;
    let concept_name = required_str(decl, "concept_name", "realization-memento")?;
    let library = required_str(decl, "library", "realization-memento")?;
    let source_function_name = required_str(decl, "source_function_name", "realization-memento")?;

    let envelope = json!({
        "body": decl,
        "header": {
            "conceptName": concept_name,
            "kind": "realization-memento",
            "realizationKind": "boundary",
            "library": library,
            "sourceFunctionName": source_function_name,
            "targetLanguage": target_language,
        },
        "schemaVersion": "1",
    });
    let canonical = encode_jcs(&json_to_cvalue(&envelope));
    let cid = blake3_512_of(canonical.as_bytes());
    Ok((cid, canonical.into_bytes()))
}

/// Reduce a function-contract `fnName` to the bare symbol a harvested call
/// ctor uses. Rust walk emits the bare ident already (`double`), so this is
/// the identity. Java's `JavaSourceLifter` emits a fully-qualified mangled
/// name (`com.example.Foo.doubleIt(int)`); the harvested junit assertion
/// ctor is the bare method name (`doubleIt`). Strip any parameter
/// signature, then take the last dot-segment. This is the bridge
/// `sourceSymbol`, which must equal the call ctor name for
/// `enumerate_callsites` to match.
fn simple_function_symbol(fn_name: &str) -> String {
    let without_params = fn_name.split('(').next().unwrap_or(fn_name);
    without_params
        .rsplit('.')
        .next()
        .unwrap_or(without_params)
        .to_string()
}

fn optional_str<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(|v| v.as_str())
}

fn required_str<'a>(value: &'a Value, field: &str, context: &str) -> Result<&'a str, String> {
    optional_str(value, field).ok_or_else(|| format!("`{context}` missing `{field}`"))
}

fn formula_hash(formula: &Arc<CValue>) -> String {
    blake3_512_of(encode_jcs(formula).as_bytes())
}

fn string_array(value: &Value, field: &str, context: &str) -> Result<Vec<String>, String> {
    let Some(values) = value.get(field) else {
        return Ok(Vec::new());
    };
    let array = values
        .as_array()
        .ok_or_else(|| format!("`{context}` field `{field}` must be an array"))?;
    array
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("`{context}` field `{field}` must contain only strings"))
        })
        .collect()
}

fn emit_witnesses_by_contract(
    witnesses: Option<&Vec<Value>>,
    project_root: &Path,
    out_dir: &Path,
    quiet: bool,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut by_contract: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Some(witnesses) = witnesses else {
        return Ok(by_contract);
    };
    for witness in witnesses {
        let attach_to = required_str(witness, "attachTo", "witness requirement")?;
        let emitted =
            crate::cmd_emit::emit_witness_requirement(project_root, witness, out_dir, quiet)
                .map_err(|e| format!("ORP witness failed: {attach_to}\n{e}"))?;
        by_contract
            .entry(attach_to.to_string())
            .or_default()
            .push(emitted.filename_cid);
    }
    Ok(by_contract)
}

fn deterministic_signer_seed(principal: &str) -> Ed25519Seed {
    let digest = blake3_512_of(format!("provekit-signer:{principal}").as_bytes());
    let hex = digest
        .strip_prefix("blake3-512:")
        .expect("blake3_512_of returns tagged digest");
    let mut seed = [0u8; 32];
    for (idx, slot) in seed.iter_mut().enumerate() {
        let hi = hex_nibble(hex.as_bytes()[idx * 2]);
        let lo = hex_nibble(hex.as_bytes()[idx * 2 + 1]);
        *slot = (hi << 4) | lo;
    }
    seed
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

/// Convert `serde_json::Value` to `provekit_canonicalizer::Value`.
fn json_to_cvalue(j: &Value) -> Arc<CValue> {
    match j {
        Value::Null => CValue::null(),
        Value::Bool(b) => CValue::boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                CValue::integer(u as i64)
            } else if let Some(f) = n.as_f64() {
                CValue::integer(f as i64)
            } else {
                CValue::integer(0)
            }
        }
        Value::String(s) => CValue::string(s.clone()),
        Value::Array(items) => {
            let v: Vec<_> = items.iter().map(|x| json_to_cvalue(x)).collect();
            CValue::array(v)
        }
        Value::Object(map) => {
            let entries: Vec<(String, Arc<CValue>)> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_cvalue(v)))
                .collect();
            CValue::object(entries)
        }
    }
}

// ---------------------------------------------------------------------------
// Self-contracts attestation signing
// ---------------------------------------------------------------------------

/// Build the signed self-contracts attestation JSON for a kit.
///
/// The signed body (per spec #94 §2) is the seven-field object without
/// `signature`. JCS encoding of that body is what the foundation key signs.
///
/// When `bundle_cid` is empty (lifter binary not found), the attestation
/// records `cid: ""`: callers can detect the empty-lifter case via this
/// field. The `contractSetCid` is still valid (it's the empty-set CID).
fn build_signed_attestation(lang: &str, bundle_cid: &str, contract_set_cid: &str) -> Value {
    let signer_pubkey = ed25519_pubkey_string(&FOUNDATION_V0_SEED);

    // Build the seven-field message body (no `signature`).
    // JCS sorts keys by code point; we build as a canonicalizer object in
    // the SAME field order as foundation-keygen does so the bytes are
    // byte-identical to what sign-self-contracts produces.
    let entries: Vec<(String, Arc<CValue>)> = vec![
        ("schemaVersion".to_string(), CValue::string("1".to_string())),
        (
            "kind".to_string(),
            CValue::string("self-contracts-attestation".to_string()),
        ),
        ("lang".to_string(), CValue::string(lang.to_string())),
        ("cid".to_string(), CValue::string(bundle_cid.to_string())),
        (
            "contractSetCid".to_string(),
            CValue::string(contract_set_cid.to_string()),
        ),
        (
            "declaredAt".to_string(),
            CValue::string(SELF_CONTRACTS_DECLARED_AT.to_string()),
        ),
        ("signer".to_string(), CValue::string(signer_pubkey.clone())),
    ];
    let msg_obj = CValue::object(entries);
    let jcs_bytes = encode_jcs(&msg_obj).into_bytes();
    let signature = ed25519_sign_string(&FOUNDATION_V0_SEED, &jcs_bytes);

    json!({
        "schemaVersion": "1",
        "kind": "self-contracts-attestation",
        "lang": lang,
        "cid": bundle_cid,
        "contractSetCid": contract_set_cid,
        "declaredAt": SELF_CONTRACTS_DECLARED_AT,
        "signer": signer_pubkey,
        "signature": signature,
    })
}

/// Write the signed attestation to `<repo_root>/.provekit/self-contracts-attestations/<lang>.json`.
///
/// The repo root is located by ascending from the project root looking for
/// a `.provekit/self-contracts-attestations/` directory. Falls back to
/// searching from CWD if the project root doesn't resolve it.
fn write_attestation(
    project_root: &Path,
    lang: &str,
    bundle_cid: &str,
    contract_set_cid: &str,
    quiet: bool,
) -> Result<PathBuf, String> {
    let attestation = build_signed_attestation(lang, bundle_cid, contract_set_cid);
    let json_str = serde_json::to_string_pretty(&attestation)
        .map_err(|e| format!("serialize attestation: {e}"))?;

    let attest_dir = find_attestation_dir(project_root)?;
    std::fs::create_dir_all(&attest_dir)
        .map_err(|e| format!("mkdir {}: {e}", attest_dir.display()))?;
    let out_path = attest_dir.join(format!("{lang}.json"));
    std::fs::write(&out_path, json_str.as_bytes())
        .map_err(|e| format!("write {}: {e}", out_path.display()))?;
    if !quiet {
        println!("{}: wrote {}", "attest".green().bold(), out_path.display());
    }
    Ok(out_path)
}

/// Locate the `.provekit/self-contracts-attestations/` directory by
/// searching upward from `start`.
fn find_attestation_dir(start: &Path) -> Result<PathBuf, String> {
    // Walk up through the directory tree looking for the attestation dir.
    let abs = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut cur = abs.as_path();
    loop {
        let candidate = cur.join(".provekit").join("self-contracts-attestations");
        if candidate.exists() {
            return Ok(candidate);
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => break,
        }
    }
    // Fall back: construct from current working directory.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    Ok(cwd.join(".provekit").join("self-contracts-attestations"))
}

// ---------------------------------------------------------------------------
// MintArgs + run
// ---------------------------------------------------------------------------

#[derive(Parser, Debug, Clone)]
pub struct MintArgs {
    #[command(subcommand)]
    pub command: Option<MintCommand>,
    /// Project root containing `.provekit/config.toml`. Defaults to current dir.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Kit shortcut: maps `<kit>` to `implementations/<kit>`.
    /// Equivalent to `--project implementations/<kit>`.
    /// Known kits: rust, go, cpp, ts, csharp, clr-bytecode, evm-bytecode, swift, java, python, ruby, zig, c, php.
    #[arg(long, conflicts_with = "project")]
    pub kit: Option<String>,
    /// Override the authoring surface (otherwise read from config or derived from --kit).
    #[arg(long)]
    pub surface: Option<String>,
    /// Ask the configured lifter for proof-producing host-language library-sugar bindings.
    #[arg(long)]
    pub library_bindings: bool,
    /// Output directory for the produced `.proof` file. Defaults to current dir.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Skip writing the signed attestation JSON.
    #[arg(long)]
    pub no_attest: bool,
    #[command(flatten)]
    pub flags: OutputFlags,
}

#[derive(Subcommand, Debug, Clone)]
pub enum MintCommand {
    Algorithm(AlgebraicMintArgs),
    Binding(AlgebraicMintArgs),
    Sort(AlgebraicMintArgs),
    Equation(AlgebraicMintArgs),
    EffectSignature(AlgebraicMintArgs),
    LanguageSignature(AlgebraicMintArgs),
    LanguageMorphism(AlgebraicMintArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct AlgebraicMintArgs {
    #[arg(long)]
    pub spec: PathBuf,
    #[arg(long)]
    pub signer: Option<PathBuf>,
    #[arg(long)]
    pub unsigned: bool,
    #[arg(long)]
    pub catalog: PathBuf,
}

pub fn run(args: MintArgs) -> u8 {
    if let Some(command) = args.command {
        return run_algebraic_mint(command);
    }

    // Resolve (project_root, surface, lang_key) from --kit or --project.
    let (project_root, derived_surface, lang_key) = if let Some(kit) = &args.kit {
        match resolve_kit(kit) {
            Some((path, surface, lang)) => (path, Some(surface), Some(lang)),
            None => {
                let known: Vec<&str> = KIT_TABLE.iter().map(|(a, _, _, _)| *a).collect();
                eprintln!(
                    "{}: unknown kit `{}`; known kits: {}",
                    "error".red().bold(),
                    kit,
                    known.join(", ")
                );
                return EXIT_USER_ERROR;
            }
        }
    } else {
        let path = args.project.clone().unwrap_or_else(|| PathBuf::from("."));
        (path, None, None)
    };

    if !project_root.exists() {
        eprintln!(
            "{}: project not found: {}",
            "error".red().bold(),
            project_root.display()
        );
        return EXIT_USER_ERROR;
    }

    let project_cfg = read_project_config(&project_root);
    let user_cfg = read_user_config();
    let configured_path = if args.kit.is_none() && args.surface.is_none() && args.out.is_none() {
        project_cfg
            .path_for("mint")
            .or_else(|| user_cfg.path_for("mint"))
    } else {
        None
    };

    let session = if let Some(path_file) = configured_path {
        dispatch_path(&project_root, Path::new(&path_file))
    } else if args.surface.is_none() && derived_surface.is_none() && !project_cfg.plugins.is_empty()
    {
        // Multi-plugin path: config.toml declared `[[plugins]]` and the
        // user didn't override with a single `--surface` or `--kit`.
        // Build a fan-in path with one lift step per declared plugin and
        // one terminal mint step depending on all of them. The path
        // executor walks each plugin's k(I)=t independently; mint merges
        // their ir-documents at the envelope-mint stage.
        if !args.flags.quiet {
            println!(
                "{}: {} plugin(s) declared: {}",
                "config".green().bold(),
                project_cfg.plugins.len(),
                project_cfg
                    .plugins
                    .iter()
                    .map(|p| p.display_name().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let out_dir = args.out.clone().unwrap_or_else(|| project_root.clone());
        dispatch_multi(
            &project_root,
            &project_cfg.plugins,
            &out_dir,
            args.flags.quiet,
            args.library_bindings,
        )
    } else {
        // Resolve surface: --surface > --kit derived > project config > user config.
        let surface = if let Some(s) = args.surface.clone() {
            s
        } else if let Some(s) = derived_surface {
            s
        } else {
            match project_cfg
                .surface_for("lift")
                .or_else(|| user_cfg.surface_for("lift"))
            {
                Some(s) => s,
                None => {
                    eprintln!(
                        "{}: no lift surface configured. Set [[plugins]] or [authoring] surface in .provekit/config.toml, or pass --surface/--kit.",
                        "error".red().bold()
                    );
                    return EXIT_USER_ERROR;
                }
            }
        };

        let out_dir = args.out.clone().unwrap_or_else(|| project_root.clone());
        dispatch(
            &project_root,
            &surface,
            &out_dir,
            args.flags.quiet,
            args.library_bindings,
        )
    };

    match session {
        Ok(session) => {
            let result = session.result;
            let contract_set_cid = if result.contract_set_cid.is_empty() {
                compute_contract_set_cid(vec![])
            } else {
                result.contract_set_cid.clone()
            };

            if args.flags.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": true,
                        "project": &project_root,
                        "surface": &session.surface,
                        "filenameCid": &result.filename_cid,
                        "contractSetCid": &contract_set_cid,
                        "bytesWritten": result.bytes_written,
                        "proofFile": &result.proof_file,
                        "lift": &result.lift_result,
                    }))
                    .expect("serialize mint JSON")
                );
            } else if !args.flags.quiet {
                println!();
                if !result.filename_cid.is_empty() {
                    println!("  catalog CID:        {}", result.filename_cid);
                }
                println!("  contractSetCid:     {contract_set_cid}");
                if result.bytes_written > 0 {
                    println!("  proof bytes:        {}", result.bytes_written);
                    if let Some(proof_file) = &result.proof_file {
                        println!("  .proof file:        {}", proof_file.display());
                    } else {
                        println!(
                            "  .proof file:        {}",
                            session
                                .out_dir
                                .join(format!("{}.proof", result.filename_cid))
                                .display()
                        );
                    }
                } else {
                    println!("  (no .proof written: lifter binary not found)");
                }
            } else {
                // Quiet mode: first line = bundle CID, second line = contractSetCid.
                // The Makefile captures contractSetCid via grep.
                if !result.filename_cid.is_empty() {
                    println!("{}", result.filename_cid);
                }
                println!("contractSetCid: {contract_set_cid}");
            }

            // Write attestation unless suppressed.
            if !args.no_attest {
                // Determine lang_key: use --kit derived value, else infer from surface.
                let lang = lang_key.as_deref().unwrap_or(&session.surface);
                if let Err(e) = write_attestation(
                    &project_root,
                    lang,
                    &result.filename_cid,
                    &contract_set_cid,
                    args.flags.quiet,
                ) {
                    eprintln!("{}: {e}", "warn".yellow().bold());
                    // Non-fatal: attestation write failure doesn't fail the mint.
                }
            }

            EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            EXIT_VERIFY_FAIL
        }
    }
}

fn run_algebraic_mint(command: MintCommand) -> u8 {
    let result = match command {
        MintCommand::Algorithm(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::AlgorithmSpec::from_path(&args.spec)?;
                algebraic_mint::mint_algorithm(spec, &signer, &catalog)
            })
        }
        MintCommand::Binding(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::BindingSpec::from_path(&args.spec)?;
                algebraic_mint::mint_binding(spec, &signer, &catalog)
            })
        }
        MintCommand::Sort(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::SortSpec::from_path(&args.spec)?;
                algebraic_mint::mint_sort(spec, &signer, &catalog)
            })
        }
        MintCommand::Equation(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::EquationSpec::from_path(&args.spec)?;
                algebraic_mint::mint_equation(spec, &signer, &catalog)
            })
        }
        MintCommand::EffectSignature(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::EffectSignatureSpec::from_path(&args.spec)?;
                algebraic_mint::mint_effect_signature(spec, &signer, &catalog)
            })
        }
        MintCommand::LanguageSignature(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::LanguageSignatureSpec::from_path(&args.spec)?;
                algebraic_mint::mint_language_signature(spec, &signer, &catalog)
            })
        }
        MintCommand::LanguageMorphism(args) => {
            let signer = signer_from_args(&args);
            signer.and_then(|signer| {
                let catalog = algebraic_mint::Catalog::new(args.catalog.clone())?;
                let spec = algebraic_mint::LanguageMorphismSpec::from_path(&args.spec)?;
                algebraic_mint::mint_language_morphism(spec, &signer, &catalog)
            })
        }
    };

    match result {
        Ok(minted) => {
            println!("{}\t{}", minted.cid, minted.path.display());
            EXIT_OK
        }
        Err(error) => {
            print_algebraic_error(&error);
            EXIT_USER_ERROR
        }
    }
}

fn signer_from_args(args: &AlgebraicMintArgs) -> algebraic_mint::Result<algebraic_mint::Signer> {
    if args.unsigned && args.signer.is_some() {
        return Err(algebraic_mint::MintError::Signer(
            "`--unsigned` cannot be combined with `--signer`".into(),
        ));
    }
    if args.unsigned {
        return Ok(algebraic_mint::Signer::from_unsigned_test_only());
    }
    let signer_path = args.signer.as_ref().ok_or_else(|| {
        algebraic_mint::MintError::Signer(
            "missing `--signer PATH` unless `--unsigned` is set".into(),
        )
    })?;
    algebraic_mint::Signer::from_pem_path(signer_path)
}

fn print_algebraic_error(error: &algebraic_mint::MintError) {
    let kind = match error {
        algebraic_mint::MintError::Validation(_) => "validation",
        algebraic_mint::MintError::Catalog(_) => "catalog",
        algebraic_mint::MintError::Signer(_) => "signer",
        algebraic_mint::MintError::Canonical(_) => "canonical",
        algebraic_mint::MintError::Io { .. } => "io",
        algebraic_mint::MintError::Json { .. } => "json",
    };
    eprintln!(
        "{}",
        serde_json::to_string(&json!({
            "error": {
                "kind": kind,
                "message": error.to_string()
            }
        }))
        .expect("serialize algebraic mint error")
    );
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_config::PlatformProfile;

    // -----------------------------------------------------------------
    // #1358 / #1355: stamp_platform_profile fills absent fields from
    // the project's platform_profile (per-shim default). Annotation-pinned
    // fields are NEVER overwritten — annotation wins.
    // -----------------------------------------------------------------

    fn sql_profile() -> PlatformProfile {
        PlatformProfile {
            language: Some("rust".to_string()),
            family: Some("concept:family:sql".to_string()),
            library: Some("rusqlite".to_string()),
            version: Some("0.39.0".to_string()),
        }
    }

    #[test]
    fn stamp_fills_absent_family_and_version_on_library_sugar_binding_entry() {
        let mut entries = vec![json!({
            "kind": "library-sugar-binding-entry",
            "concept_name": "concept:sql-query",
            "target_library_tag": "rusqlite",
        })];
        stamp_platform_profile(&mut entries, &sql_profile());
        let e = &entries[0];
        assert_eq!(e["family"], "concept:family:sql");
        assert_eq!(e["library_version"], "0.39.0");
    }

    #[test]
    fn stamp_preserves_annotation_pinned_family_and_version() {
        // Annotation wins — profile MUST NOT overwrite.
        let mut entries = vec![json!({
            "kind": "library-sugar-binding-entry",
            "concept_name": "concept:sql-query",
            "target_library_tag": "rusqlite",
            "family": "concept:family:sql-experimental",
            "library_version": "0.40.0-rc1",
        })];
        stamp_platform_profile(&mut entries, &sql_profile());
        let e = &entries[0];
        assert_eq!(
            e["family"], "concept:family:sql-experimental",
            "annotation family preserved"
        );
        assert_eq!(
            e["library_version"], "0.40.0-rc1",
            "annotation version preserved"
        );
    }

    #[test]
    fn stamp_applies_to_realization_memento_too() {
        let mut entries = vec![json!({
            "kind": "realization-memento",
            "realization_kind": "boundary",
            "concept_name": "concept:sql-query",
            "library": "rusqlite",
        })];
        stamp_platform_profile(&mut entries, &sql_profile());
        let e = &entries[0];
        assert_eq!(e["family"], "concept:family:sql");
        assert_eq!(e["library_version"], "0.39.0");
    }

    #[test]
    fn stamp_with_partial_profile_only_fills_pinned_axes() {
        // Profile floats `library`; only family + version get stamped.
        let profile = PlatformProfile {
            language: Some("rust".to_string()),
            family: Some("concept:family:hash".to_string()),
            library: None,
            version: Some("1".to_string()),
        };
        let mut entries = vec![json!({
            "kind": "library-sugar-binding-entry",
            "concept_name": "concept:blake3-512-of",
            "target_library_tag": "blake3",
        })];
        stamp_platform_profile(&mut entries, &profile);
        let e = &entries[0];
        assert_eq!(e["family"], "concept:family:hash");
        assert_eq!(e["library_version"], "1");
        // library not present in profile → not stamped → entry's
        // target_library_tag unchanged (annotation already had "blake3").
        assert_eq!(e["target_library_tag"], "blake3");
    }

    #[test]
    fn stamp_with_empty_profile_is_no_op() {
        let profile = PlatformProfile::default();
        let mut entries = vec![json!({
            "kind": "library-sugar-binding-entry",
            "concept_name": "concept:foo",
            "target_library_tag": "bar",
        })];
        stamp_platform_profile(&mut entries, &profile);
        let e = &entries[0];
        assert!(e.get("family").is_none(), "no family stamped");
        assert!(e.get("library_version").is_none(), "no version stamped");
    }

    #[test]
    fn resolve_kit_ts_maps_to_typescript_dir() {
        // Issue #204: ts kit routes to typescript-self-contracts surface (not the
        // generic workspace lifter) so --kit=ts mints real self-contracts.
        let (path, surface, lang) = resolve_kit("ts").expect("ts must resolve");
        assert_eq!(path, PathBuf::from("implementations/typescript"));
        assert_eq!(surface, "typescript-self-contracts");
        assert_eq!(lang, "ts");
    }

    #[test]
    fn resolve_kit_rust_maps_to_rust_dir() {
        // Issue #176 Tier 1: rust kit maps to rust-self-contracts surface so the
        // attestation reflects the canonical self-contracts slab, not the generic
        // workspace lifter.
        let (path, surface, lang) = resolve_kit("rust").expect("rust must resolve");
        assert_eq!(path, PathBuf::from("implementations/rust"));
        assert_eq!(surface, "rust-self-contracts");
        assert_eq!(lang, "rust");
    }

    #[test]
    fn resolve_kit_all_13_ci_kits() {
        let kits = [
            "rust",
            "go",
            "cpp",
            "ts",
            "csharp",
            "clr-bytecode",
            "evm-bytecode",
            "swift",
            "java",
            "python",
            "ruby",
            "zig",
            "c",
        ];
        for kit in kits {
            assert!(resolve_kit(kit).is_some(), "kit `{kit}` must resolve");
        }
    }

    #[test]
    fn resolve_kit_unknown_returns_none() {
        assert!(resolve_kit("haskell").is_none());
    }

    #[test]
    fn build_signed_attestation_has_required_fields() {
        let a = build_signed_attestation("rust", "blake3-512:deadbeef", "blake3-512:cafebabe");
        assert_eq!(a["schemaVersion"].as_str(), Some("1"));
        assert_eq!(a["kind"].as_str(), Some("self-contracts-attestation"));
        assert_eq!(a["lang"].as_str(), Some("rust"));
        assert_eq!(a["declaredAt"].as_str(), Some("2026-05-05T18:00:00Z"));
        assert!(a["signature"].as_str().unwrap().starts_with("ed25519:"));
        assert!(a["signer"].as_str().unwrap().starts_with("ed25519:"));
    }

    #[test]
    fn build_signed_attestation_is_deterministic() {
        let a = build_signed_attestation("go", "blake3-512:aa", "blake3-512:bb");
        let b = build_signed_attestation("go", "blake3-512:aa", "blake3-512:bb");
        assert_eq!(a, b);
    }

    #[test]
    fn dispatch_lift_params_source_paths_non_empty() {
        // C3 (verify_c3_lift_request_well_formed) requires source_paths to be
        // a non-empty array. Sending [] was the bug fixed in issue #166.
        let root = PathBuf::from(".");
        let params =
            crate::lift_plugin::build_lift_params(&root, "rust", LiftPluginOptions::default());
        let paths = params["source_paths"]
            .as_array()
            .expect("source_paths must be an array");
        assert!(
            !paths.is_empty(),
            "source_paths must not be empty: was C3 violation (issue #166)"
        );
        assert_eq!(paths[0].as_str(), Some("."), "first entry should be '.'");
    }

    #[test]
    fn dispatch_lift_params_has_surface_and_options() {
        let root = PathBuf::from(".");
        let params =
            crate::lift_plugin::build_lift_params(&root, "go", LiftPluginOptions::default());
        assert_eq!(params["surface"].as_str(), Some("go"));
        assert_eq!(
            params["config_path"].as_str(),
            Some(".provekit/config.toml")
        );
        assert!(
            params["workspace_root"].as_str().is_some(),
            "workspace_root should be present for lifters that resolve source through the project root"
        );
        assert_eq!(params["options"]["layer"].as_str(), Some("all"));
    }

    #[test]
    fn mint_input_is_a_composed_path() {
        let input = mint_input(
            std::path::Path::new("."),
            "rust-self-contracts",
            std::path::Path::new("out"),
            true,
            false,
        );
        let Input::Path(path) = input.input else {
            panic!("mint command input must be a composed path");
        };

        let lift = path.step("lift").expect("lift algebra step");
        let mint = path.step("mint").expect("mint algebra step");
        assert_eq!(lift.kit, "lift-plugin:rust-self-contracts");
        assert_eq!(mint.kit, "provekit-mint");
        assert_eq!(lift.inputs.len(), 1);
        assert_eq!(mint.inputs.len(), 1);
        assert_eq!(mint.depends_on, vec!["lift".to_string()]);
        assert!(path.cid().as_str().starts_with("blake3-512:"));
    }

    #[test]
    fn mint_input_can_request_library_binding_layer() {
        let input = mint_input(
            std::path::Path::new("."),
            "typescript-bind",
            std::path::Path::new("out"),
            true,
            true,
        );
        let Input::Path(path) = input.input else {
            panic!("mint command input must be a composed path");
        };
        let lift = path.step("lift").expect("lift algebra step");
        let lift_spec = input
            .inputs
            .get_input(&lift.inputs[0])
            .expect("lift input spec materialized");
        let Input::Spec(lift_spec) = lift_spec else {
            panic!("lift input must be an Input::Spec");
        };

        assert_eq!(
            lift_spec["options"]["layer"].as_str(),
            Some("library-bindings")
        );
    }

    #[test]
    fn mint_transform_rejects_invalid_path_algebra() {
        let input = Input::Path(Box::new(CorePath {
            algebra: vec![
                PathAlgebra {
                    name: "lift".to_string(),
                    kit: "lift-plugin:rust".to_string(),
                    inputs: vec![address(&Input::Spec(json!({
                        "surface": "rust",
                        "workspace_root": "."
                    })))],
                    depends_on: vec![],
                    verb: Verb::Transform,
                },
                PathAlgebra {
                    name: "mint".to_string(),
                    kit: "provekit-mint".to_string(),
                    inputs: vec![address(&Input::Spec(json!({
                        "outDir": "out"
                    })))],
                    depends_on: vec!["lift".to_string(), "missing".to_string()],
                    verb: Verb::Transform,
                },
            ],
        }));

        let error = MintKit::default()
            .transform(&input)
            .expect_err("invalid path algebra should be rejected before transport")
            .to_string();
        assert!(
            error.contains("missing step `missing`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn mint_from_ir_document_accepts_library_sugar_binding_without_contracts() {
        let ir = vec![json!({
            "body_source": {
                "file": "src/shims/requests.py",
                "source_cid": "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "span": {"start_line": 1, "start_col": 0, "end_line": 6, "end_col": 0}
            },
            "concept_name": "concept:http-request",
            "kind": "library-sugar-binding-entry",
            "loss_record_contribution": {"form": "literal", "value": {"entries": []}},
            "param_names": ["url"],
            "param_types": ["str"],
            "return_type": "int",
            "signature_shape_cid": "blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "source_function_name": "fetch_status",
            "target_language": "python",
            "target_library_tag": "requests",
            "term_shape": null,
            "term_shape_cid": null
        })];

        let (bytes, filename_cid, contract_set_cid) =
            mint_from_ir_document(&ir, None, None, None, Path::new("."), Path::new("."), true)
                .expect("library-sugar-only ir-document must mint without contracts");

        assert!(!bytes.is_empty());
        assert!(filename_cid.starts_with("blake3-512:"));
        assert_eq!(contract_set_cid, compute_contract_set_cid(vec![]));

        let catalog = provekit_verifier::cbor_decode::decode(&bytes).expect("decode proof");
        let members = catalog
            .as_map()
            .and_then(|m| m.get("members"))
            .and_then(|v| v.as_map())
            .expect("proof members");
        assert_eq!(members.len(), 1);
        let member = members.values().next().expect("library binding member");
        let envelope: Value =
            serde_json::from_slice(member.as_bstr().expect("member bytes")).expect("member JSON");
        assert_eq!(
            envelope.pointer("/header/kind").and_then(|v| v.as_str()),
            Some("library-sugar-binding-entry")
        );
        assert_eq!(
            envelope
                .pointer("/body/target_library_tag")
                .and_then(|v| v.as_str()),
            Some("requests")
        );
    }

    #[test]
    fn mint_from_ir_document_accepts_contract_decl_shape() {
        let ir = vec![json!({
            "kind": "contract",
            "symbol": "accept",
            "invariant": {
                "kind": "atomic",
                "name": "eq",
                "args": [
                    {"kind": "var", "name": "value"},
                    {"kind": "const", "value": 42, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }
        })];

        let (bytes, filename_cid, contract_set_cid) =
            mint_from_ir_document(&ir, None, None, None, Path::new("."), Path::new("."), true)
                .expect("mint bug-zoo style ir-document");
        assert!(!bytes.is_empty());
        assert!(filename_cid.starts_with("blake3-512:"));
        assert!(contract_set_cid.starts_with("blake3-512:"));
        let proof_path = PathBuf::from(format!("{filename_cid}.proof"));
        let report =
            provekit_verifier::proof_conformance::validate_proof_bytes(&proof_path, &bytes);
        assert!(
            report.errors.is_empty(),
            "minted ir-document proof should inspect cleanly: {:?}",
            report.errors
        );
    }

    #[test]
    fn mint_from_ir_document_mints_implication_mementos() {
        let ir = vec![
            json!({
                "kind": "contract",
                "name": "lower.claim",
                "outBinding": "out",
                "post": {"kind": "atomic", "name": "lower_holds", "args": []}
            }),
            json!({
                "kind": "contract",
                "name": "upper.claim",
                "outBinding": "out",
                "post": {"kind": "atomic", "name": "upper_holds", "args": []}
            }),
        ];
        let implications = vec![json!({
            "name": "lower-implies-upper",
            "antecedent": "lower.claim",
            "consequent": "upper.claim",
            "antecedentSlot": "post",
            "consequentSlot": "post"
        })];

        let (bytes, _, _) = mint_from_ir_document(
            &ir,
            None,
            Some(&implications),
            None,
            Path::new("."),
            Path::new("."),
            true,
        )
        .expect("mint contracts plus implication");
        let catalog = provekit_verifier::cbor_decode::decode(&bytes).expect("decode proof");
        let members = catalog
            .as_map()
            .and_then(|m| m.get("members"))
            .and_then(|v| v.as_map())
            .expect("proof members");

        assert_eq!(members.len(), 3);

        let mut contract_count = 0;
        let mut implication_count = 0;
        for member in members.values() {
            let bytes = member.as_bstr().expect("member bytes");
            let envelope: Value = serde_json::from_slice(bytes).expect("member JSON");
            match envelope.pointer("/header/kind").and_then(|v| v.as_str()) {
                Some("contract") => contract_count += 1,
                Some("implication") => {
                    implication_count += 1;
                    let inputs = envelope
                        .pointer("/header/inputCids")
                        .and_then(|v| v.as_array())
                        .expect("implication inputCids");
                    assert_eq!(inputs.len(), 2);
                }
                other => panic!("unexpected member kind {other:?}"),
            }
        }

        assert_eq!(contract_count, 2);
        assert_eq!(implication_count, 1);
    }

    #[test]
    fn mint_from_ir_document_links_contract_to_authority_memento() {
        let ir = vec![json!({
            "kind": "contract",
            "name": "checked_add_u8.postcondition",
            "outBinding": "out",
            "authority": "bridgeworks.software",
            "post": {"kind": "atomic", "name": "checked_add_u8.postcondition", "args": []}
        })];
        let authorities = vec![
            json!({
                "id": "bridgeworks.root",
                "principal": "bridgeworks.root",
                "scopeKind": "proof",
                "scope": "authority-backed-test"
            }),
            json!({
                "id": "bridgeworks.software",
                "principal": "bridgeworks.software",
                "scopeKind": "contract",
                "scope": "checked_add_u8.postcondition",
                "parent": "bridgeworks.root"
            }),
        ];

        let (bytes, filename_cid, _) = mint_from_ir_document(
            &ir,
            Some(&authorities),
            None,
            None,
            Path::new("."),
            Path::new("."),
            true,
        )
        .expect("mint authority plus contract");
        let proof_path = PathBuf::from(format!("{filename_cid}.proof"));
        let report =
            provekit_verifier::proof_conformance::validate_proof_bytes(&proof_path, &bytes);
        assert!(
            report.errors.is_empty(),
            "authority-backed proof should inspect cleanly: {:?}",
            report.errors
        );

        let catalog = provekit_verifier::cbor_decode::decode(&bytes).expect("decode proof");
        let root = catalog.as_map().expect("catalog map");
        let proof_signer = root
            .get("signer")
            .and_then(|v| v.as_tstr())
            .expect("proof signer");
        assert!(proof_signer.starts_with("blake3-512:"));

        let members = root
            .get("members")
            .and_then(|v| v.as_map())
            .expect("proof members");
        let mut authority = None;
        let mut authority_member_cid = None;
        let mut contract = None;
        for (cid, member) in members {
            let bytes = member.as_bstr().expect("member bytes");
            let envelope: Value = serde_json::from_slice(bytes).expect("member JSON");
            match envelope.pointer("/header/kind").and_then(|v| v.as_str()) {
                Some("authority")
                    if envelope
                        .pointer("/header/principal")
                        .and_then(|v| v.as_str())
                        == Some("bridgeworks.software") =>
                {
                    authority_member_cid = Some(cid.clone());
                    authority = Some(envelope);
                }
                Some("contract") => contract = Some(envelope),
                _ => {}
            }
        }
        let authority = authority.expect("authority memento");
        let authority_member_cid = authority_member_cid.expect("authority member cid");
        let contract = contract.expect("contract memento");
        let authority_key = authority
            .pointer("/header/key")
            .and_then(|v| v.as_str())
            .expect("authority key");

        assert_eq!(
            contract
                .pointer("/header/inputCids/0")
                .and_then(|v| v.as_str()),
            Some(authority_member_cid.as_str())
        );
        assert_eq!(
            contract
                .pointer("/envelope/signer")
                .and_then(|v| v.as_str()),
            Some(authority_key)
        );
    }

    #[test]
    fn mint_from_ir_document_rejects_implication_missing_contract() {
        let ir = vec![json!({
            "kind": "contract",
            "name": "upper.claim",
            "outBinding": "out",
            "post": {"kind": "atomic", "name": "upper_holds", "args": []}
        })];
        let implications = vec![json!({
            "name": "lower-implies-upper",
            "antecedent": "lower.claim",
            "consequent": "upper.claim",
            "antecedentSlot": "post",
            "consequentSlot": "post"
        })];

        let err = mint_from_ir_document(
            &ir,
            None,
            Some(&implications),
            None,
            Path::new("."),
            Path::new("."),
            true,
        )
        .expect_err("missing antecedent should fail");

        assert!(err.contains("lower.claim"), "error was: {err}");
    }

    #[test]
    fn empty_set_cid_is_stable() {
        // Verify compute_contract_set_cid([]) is stable across calls.
        let a = compute_contract_set_cid(vec![]);
        let b = compute_contract_set_cid(vec![]);
        assert_eq!(a, b);
        assert!(a.starts_with("blake3-512:"));
        // Print so the integration test can verify against the pinned value.
        eprintln!("empty-set CID = {a}");
    }
}
