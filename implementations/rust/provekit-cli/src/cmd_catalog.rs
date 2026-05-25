// SPDX-License-Identifier: Apache-2.0
//
// `provekit catalog`: a coherent, READ-ONLY browse surface over the
// neutral contract catalog at `menagerie/concept-shapes/catalog/`.
//
// Subcommands:
//   list                 — every concept hub + realization count; singletons flagged
//   show <concept>       — one concept: contract, contract-note, realizations, CID
//   realizations <c>     — a concept's realizations, filterable by --lang
//   filter               — query by name pattern / --lang / --singletons
//
// Counting rule (PR-0 / Q-1, #1417): when counting realizations for the
// singleton check we count ONLY realization mementos whose `role` is
// "abstraction-realization" (N-edges: hub → language). Entries with
// role "abstraction-lift" (M-edges: language → hub) are EXCLUDED — they
// answer a different question and do not count toward "how many languages
// realize this concept." A concept with < 2 such realizations is flagged
// as a `library:`-tag candidate per PR-0's validator (#1428).
//
// Data source: the catalog files on disk. The embedded `index.json` lags
// the filesystem (it omits several recently-minted hubs), so the on-disk
// abstraction/ + realization/ directories are authoritative here. The
// counting *logic* matches PR-0's validator; only the data source differs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use serde_json::{json, Value as Json};

use crate::OutputFlags;

const ROLE_REALIZATION: &str = "abstraction-realization";
const SINGLETON_THRESHOLD: usize = 2;

#[derive(Parser, Debug, Clone)]
pub struct CatalogArgs {
    #[command(subcommand)]
    pub cmd: CatalogCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CatalogCmd {
    /// List all concept hubs with realization counts; flag singletons.
    List(ListArgs),
    /// Show one concept: contract, realizations, CID.
    Show(ShowArgs),
    /// List a concept's realizations, optionally filtered by target language.
    Realizations(RealizationsArgs),
    /// Query concepts by name pattern, target language, or realization count.
    Filter(FilterArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ListArgs {
    /// Workspace root containing menagerie/concept-shapes/catalog. Defaults to
    /// the nearest ancestor of the current directory that contains it.
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ShowArgs {
    /// Concept name, e.g. `concept:assert` (the `concept:` prefix is optional).
    pub concept: String,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct RealizationsArgs {
    /// Concept name, e.g. `concept:closure` (the `concept:` prefix is optional).
    pub concept: String,
    /// Restrict to realizations whose target language matches (case-insensitive).
    #[arg(long)]
    pub lang: Option<String>,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct FilterArgs {
    /// Substring matched against concept names (case-insensitive).
    #[arg(long)]
    pub name: Option<String>,
    /// Keep concepts that have a realization in this target language.
    #[arg(long)]
    pub lang: Option<String>,
    /// Keep only singletons (< 2 abstraction-realization realizations):
    /// the `library:`-tag candidates.
    #[arg(long)]
    pub singletons: bool,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: CatalogArgs) -> u8 {
    match args.cmd {
        CatalogCmd::List(a) => dispatch(a.project.as_deref(), a.out.clone(), |cat| {
            run_list(cat, &a.out)
        }),
        CatalogCmd::Show(a) => dispatch(a.project.as_deref(), a.out.clone(), |cat| {
            run_show(cat, &a.concept, &a.out)
        }),
        CatalogCmd::Realizations(a) => dispatch(a.project.as_deref(), a.out.clone(), |cat| {
            run_realizations(cat, &a.concept, a.lang.as_deref(), &a.out)
        }),
        CatalogCmd::Filter(a) => dispatch(a.project.as_deref(), a.out.clone(), |cat| {
            run_filter(cat, &a, &a.out)
        }),
    }
}

/// Load the catalog once and hand it to `body`, mapping load errors to a
/// user-error exit. Keeps every subcommand's I/O bootstrap identical.
fn dispatch(project: Option<&Path>, out: OutputFlags, body: impl FnOnce(&Catalog) -> u8) -> u8 {
    let root = match resolve_catalog_root(project) {
        Some(r) => r,
        None => {
            if !out.quiet {
                eprintln!(
                    "{}: could not locate menagerie/concept-shapes/catalog \
                     (pass --project <workspace-root>)",
                    "error".red().bold()
                );
            }
            return crate::EXIT_USER_ERROR;
        }
    };
    match Catalog::load(&root) {
        Ok(cat) => body(&cat),
        Err(e) => {
            if !out.quiet {
                eprintln!("{}: {e}", "error".red().bold());
            }
            crate::EXIT_USER_ERROR
        }
    }
}

/// Find the catalog directory. If `--project` is given, use it directly.
/// Otherwise walk up from the current directory until a directory containing
/// `menagerie/concept-shapes/catalog` is found, so the command works from
/// any subdirectory of the repo.
fn resolve_catalog_root(project: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = project {
        let cat = p.join("menagerie/concept-shapes/catalog");
        return cat.is_dir().then(|| cat);
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cat = dir.join("menagerie/concept-shapes/catalog");
        if cat.is_dir() {
            return Some(cat);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// One realization memento, reduced to the fields the browse surface needs.
#[derive(Debug, Clone)]
struct Realization {
    cid: String,
    fn_name: String,
    target_lang: String,
    role: String,
}

impl Realization {
    fn is_n_edge(&self) -> bool {
        self.role == ROLE_REALIZATION
    }
}

/// One concept hub, with its realizations resolved from CID references.
#[derive(Debug, Clone)]
struct Concept {
    name: String,
    cid: String,
    /// Human note describing the contract (`contract_note` or `description`).
    contract_note: Option<String>,
    /// The contract formula, if present (the `concept:option-bind` hub uses a
    /// different `semantics`/`type` shape and has no `contract` field).
    contract: Option<Json>,
    /// Realizations referenced by this hub, resolved against the realization
    /// directory. Unresolved CIDs are dropped (the index can lag the files).
    realizations: Vec<Realization>,
}

impl Concept {
    /// Realizations that count toward the singleton check (N-edges only).
    fn n_edge_realizations(&self) -> Vec<&Realization> {
        self.realizations.iter().filter(|r| r.is_n_edge()).collect()
    }

    fn realization_count(&self) -> usize {
        self.n_edge_realizations().len()
    }

    fn is_singleton(&self) -> bool {
        self.realization_count() < SINGLETON_THRESHOLD
    }

    /// Distinct target languages among N-edge realizations, sorted.
    fn languages(&self) -> Vec<String> {
        let mut langs: Vec<String> = self
            .n_edge_realizations()
            .iter()
            .map(|r| r.target_lang.clone())
            .collect();
        langs.sort();
        langs.dedup();
        langs
    }

    fn has_lang(&self, lang: &str) -> bool {
        self.n_edge_realizations()
            .iter()
            .any(|r| r.target_lang.eq_ignore_ascii_case(lang))
    }
}

struct Catalog {
    /// Concepts keyed by name, sorted for deterministic output.
    concepts: BTreeMap<String, Concept>,
}

impl Catalog {
    /// Walk `abstractions/` and `realizations/` once each, resolve every
    /// hub's realization CIDs against a realization lookup, and build the
    /// in-memory catalog. Pure read; never mutates the catalog files.
    fn load(catalog_dir: &Path) -> Result<Self, String> {
        let realizations = load_realizations(&catalog_dir.join("realizations"))?;
        let abstractions_dir = catalog_dir.join("abstractions");
        if !abstractions_dir.is_dir() {
            return Err(format!(
                "abstractions directory not found: {}",
                abstractions_dir.display()
            ));
        }
        let mut concepts = BTreeMap::new();
        for entry in read_dir_sorted(&abstractions_dir)? {
            let raw = match std::fs::read_to_string(&entry) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let env: Json = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let memento = env.get("memento").unwrap_or(&env);
            // Both hub shapes name themselves via `operator` (the canonical
            // ConceptAbstractionMemento) or `name` (the older option-bind
            // hub shape). Defensive extraction avoids a panic on the 24th
            // file, which is not a ConceptAbstractionMemento.
            let Some(name) = memento
                .get("operator")
                .and_then(Json::as_str)
                .or_else(|| memento.get("name").and_then(Json::as_str))
            else {
                continue;
            };
            let cid = env
                .get("cid")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let contract_note = memento
                .get("contract_note")
                .and_then(Json::as_str)
                .or_else(|| memento.get("description").and_then(Json::as_str))
                .map(str::to_string);
            let contract = memento.get("contract").cloned();
            let realization_cids: Vec<String> = memento
                .get("realizations")
                .and_then(Json::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let resolved: Vec<Realization> = realization_cids
                .iter()
                .filter_map(|c| realizations.get(c).cloned())
                .collect();
            concepts.insert(
                name.to_string(),
                Concept {
                    name: name.to_string(),
                    cid,
                    contract_note,
                    contract,
                    realizations: resolved,
                },
            );
        }
        Ok(Self { concepts })
    }

    fn get(&self, name: &str) -> Option<&Concept> {
        if let Some(c) = self.concepts.get(name) {
            return Some(c);
        }
        // Accept the bare name without the `concept:` prefix.
        let with_prefix = format!("concept:{name}");
        self.concepts.get(&with_prefix)
    }

    fn iter(&self) -> impl Iterator<Item = &Concept> {
        self.concepts.values()
    }
}

/// Build a CID → Realization lookup by reading every realization memento.
fn load_realizations(dir: &Path) -> Result<BTreeMap<String, Realization>, String> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Err(format!(
            "realizations directory not found: {}",
            dir.display()
        ));
    }
    for entry in read_dir_sorted(dir)? {
        let raw = match std::fs::read_to_string(&entry) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let env: Json = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let memento = env.get("memento").unwrap_or(&env);
        let cid = match env.get("cid").and_then(Json::as_str) {
            Some(c) => c.to_string(),
            None => continue,
        };
        let role = memento
            .get("role")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let target_lang = memento
            .get("target_lang")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let fn_name = memento
            .get("fn_name")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        out.insert(
            cid.clone(),
            Realization {
                cid,
                fn_name,
                target_lang,
                role,
            },
        );
    }
    Ok(out)
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    paths.sort();
    Ok(paths)
}

// ---------------------------------------------------------------------------
// Subcommand bodies
// ---------------------------------------------------------------------------

fn run_list(cat: &Catalog, out: &OutputFlags) -> u8 {
    let concepts: Vec<&Concept> = cat.iter().collect();
    let singletons = concepts.iter().filter(|c| c.is_singleton()).count();

    if out.json {
        let rows: Vec<Json> = concepts
            .iter()
            .map(|c| {
                json!({
                    "concept": c.name,
                    "cid": c.cid,
                    "realizations": c.realization_count(),
                    "languages": c.languages(),
                    "singleton": c.is_singleton(),
                })
            })
            .collect();
        let payload = json!({
            "concepts": rows,
            "total": concepts.len(),
            "singletons": singletons,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        return crate::EXIT_OK;
    }
    if out.quiet {
        return crate::EXIT_OK;
    }

    println!("{}", "ProvekIt contract catalog".bold());
    println!(
        "  {} concepts, {} singleton(s) (< {} realizations — library:-tag candidates)\n",
        concepts.len(),
        singletons,
        SINGLETON_THRESHOLD
    );
    println!("  {:<32}  {:>5}  {}", "CONCEPT", "REAL", "LANGUAGES");
    println!("  {:<32}  {:>5}  {}", "-".repeat(32), "----", "---------");
    for c in &concepts {
        let langs = c.languages().join(", ");
        let count = c.realization_count();
        let flag = if c.is_singleton() {
            " *".yellow().to_string()
        } else {
            String::new()
        };
        println!("  {:<32}  {:>5}  {}{}", c.name, count, langs, flag);
    }
    if singletons > 0 {
        println!("\n  {} = singleton (library:-tag candidate)", "*".yellow());
    }
    crate::EXIT_OK
}

fn run_show(cat: &Catalog, concept: &str, out: &OutputFlags) -> u8 {
    let Some(c) = cat.get(concept) else {
        if !out.quiet {
            eprintln!("{}: no such concept: {concept}", "error".red().bold());
        }
        return crate::EXIT_USER_ERROR;
    };

    if out.json {
        let reals: Vec<Json> = c
            .realizations
            .iter()
            .map(|r| {
                json!({
                    "fn_name": r.fn_name,
                    "target_lang": r.target_lang,
                    "role": r.role,
                    "cid": r.cid,
                    "counts_as_realization": r.is_n_edge(),
                })
            })
            .collect();
        let payload = json!({
            "concept": c.name,
            "cid": c.cid,
            "contract_note": c.contract_note,
            "contract": c.contract,
            "realization_count": c.realization_count(),
            "languages": c.languages(),
            "singleton": c.is_singleton(),
            "realizations": reals,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        return crate::EXIT_OK;
    }
    if out.quiet {
        return crate::EXIT_OK;
    }

    println!("{} {}", "concept:".bold(), c.name.cyan().bold());
    println!("  cid : {}", c.cid);
    if let Some(note) = &c.contract_note {
        println!("\n  {}", "contract note".bold());
        println!("    {note}");
    }
    if let Some(contract) = &c.contract {
        println!("\n  {}", "contract".bold());
        let pretty = serde_json::to_string_pretty(contract).unwrap_or_default();
        for line in pretty.lines() {
            println!("    {line}");
        }
    }
    println!(
        "\n  {} ({}{})",
        "realizations".bold(),
        c.realization_count(),
        if c.is_singleton() {
            " — singleton, library:-tag candidate".yellow().to_string()
        } else {
            String::new()
        }
    );
    if c.realizations.is_empty() {
        println!("    {}", "(none)".yellow());
    }
    for r in &c.realizations {
        let edge = if r.is_n_edge() {
            r.target_lang.green().to_string()
        } else {
            format!("{} [{}]", r.target_lang, r.role)
                .dimmed()
                .to_string()
        };
        println!("    - {:<10}  {}", edge, r.fn_name);
    }
    crate::EXIT_OK
}

fn run_realizations(cat: &Catalog, concept: &str, lang: Option<&str>, out: &OutputFlags) -> u8 {
    let Some(c) = cat.get(concept) else {
        if !out.quiet {
            eprintln!("{}: no such concept: {concept}", "error".red().bold());
        }
        return crate::EXIT_USER_ERROR;
    };
    // Browse N-edge realizations only; that is what "realizations of a concept"
    // means here. Apply the optional language filter.
    let reals: Vec<&Realization> = c
        .n_edge_realizations()
        .into_iter()
        .filter(|r| lang.is_none_or(|l| r.target_lang.eq_ignore_ascii_case(l)))
        .collect();

    if out.json {
        let rows: Vec<Json> = reals
            .iter()
            .map(|r| {
                json!({
                    "fn_name": r.fn_name,
                    "target_lang": r.target_lang,
                    "role": r.role,
                    "cid": r.cid,
                })
            })
            .collect();
        let payload = json!({
            "concept": c.name,
            "lang": lang,
            "realizations": rows,
            "count": reals.len(),
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        return crate::EXIT_OK;
    }
    if out.quiet {
        return crate::EXIT_OK;
    }

    let scope = match lang {
        Some(l) => format!(" (lang = {l})"),
        None => String::new(),
    };
    println!(
        "{} {}{}",
        "realizations of".bold(),
        c.name.cyan().bold(),
        scope
    );
    if reals.is_empty() {
        println!("  {}", "(none)".yellow());
        return crate::EXIT_OK;
    }
    for r in &reals {
        println!("  - {:<10}  {}", r.target_lang.green(), r.fn_name);
    }
    crate::EXIT_OK
}

fn run_filter(cat: &Catalog, args: &FilterArgs, out: &OutputFlags) -> u8 {
    let name_pat = args.name.as_deref().map(str::to_ascii_lowercase);
    let matched: Vec<&Concept> = cat
        .iter()
        .filter(|c| {
            name_pat
                .as_deref()
                .is_none_or(|p| c.name.to_ascii_lowercase().contains(p))
        })
        .filter(|c| args.lang.as_deref().is_none_or(|l| c.has_lang(l)))
        .filter(|c| !args.singletons || c.is_singleton())
        .collect();

    if out.json {
        let rows: Vec<Json> = matched
            .iter()
            .map(|c| {
                json!({
                    "concept": c.name,
                    "cid": c.cid,
                    "realizations": c.realization_count(),
                    "languages": c.languages(),
                    "singleton": c.is_singleton(),
                })
            })
            .collect();
        let payload = json!({
            "filter": {
                "name": args.name,
                "lang": args.lang,
                "singletons": args.singletons,
            },
            "concepts": rows,
            "count": matched.len(),
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        return crate::EXIT_OK;
    }
    if out.quiet {
        return crate::EXIT_OK;
    }

    println!("{} {} match(es)", "catalog filter:".bold(), matched.len());
    for c in &matched {
        let langs = c.languages().join(", ");
        let flag = if c.is_singleton() {
            " *".yellow().to_string()
        } else {
            String::new()
        };
        println!(
            "  {:<32}  {:>3}  {}{}",
            c.name,
            c.realization_count(),
            langs,
            flag
        );
    }
    crate::EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locate the repo's real catalog by walking up from the crate dir.
    fn repo_catalog() -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let cat = dir.join("menagerie/concept-shapes/catalog");
            if cat.is_dir() {
                return cat;
            }
            assert!(dir.pop(), "could not find menagerie/concept-shapes/catalog");
        }
    }

    fn load() -> Catalog {
        Catalog::load(&repo_catalog()).expect("load real catalog")
    }

    #[test]
    fn loads_all_concept_hubs_without_panicking() {
        let cat = load();
        // 24 hubs on disk as of PR-10 (the embedded index lags at 18).
        assert!(
            cat.concepts.len() >= 22,
            "expected >= 22 concept hubs, got {}",
            cat.concepts.len()
        );
    }

    #[test]
    fn parses_the_non_standard_option_bind_hub() {
        // concept:option-bind uses a `name`/`semantics`/`type` shape rather
        // than the canonical operator/tier/contract shape. It MUST still load.
        let cat = load();
        let c = cat
            .get("concept:option-bind")
            .expect("option-bind hub present");
        assert_eq!(c.name, "concept:option-bind");
        assert!(
            c.contract_note.is_some(),
            "description maps to contract_note"
        );
    }

    #[test]
    fn counting_excludes_abstraction_lift_edges() {
        // Counting rule (#1417): only role == "abstraction-realization" counts.
        let cat = load();
        for c in cat.iter() {
            let counted = c.realization_count();
            let n_edges = c
                .realizations
                .iter()
                .filter(|r| r.role == "abstraction-realization")
                .count();
            assert_eq!(counted, n_edges, "{} count must equal N-edges", c.name);
            // Any abstraction-lift edge present must NOT be counted.
            let lift = c
                .realizations
                .iter()
                .filter(|r| r.role == "abstraction-lift")
                .count();
            assert_eq!(
                counted + lift,
                c.realizations.iter().filter(|r| !r.role.is_empty()).count(),
                "{}: counted + lift must equal all role-bearing realizations",
                c.name
            );
        }
    }

    #[test]
    fn assert_is_a_singleton() {
        // concept:assert has exactly one C realization → singleton candidate.
        let cat = load();
        let c = cat.get("concept:assert").expect("assert hub present");
        assert_eq!(c.realization_count(), 1);
        assert!(c.is_singleton());
        assert_eq!(c.languages(), vec!["c".to_string()]);
    }

    #[test]
    fn closure_is_multi_language_and_not_singleton() {
        let cat = load();
        let c = cat.get("concept:closure").expect("closure hub present");
        assert!(c.realization_count() >= 3);
        assert!(!c.is_singleton());
        assert!(c.has_lang("java"));
        assert!(c.has_lang("JAVA"), "lang match is case-insensitive");
    }

    #[test]
    fn bare_name_without_prefix_resolves() {
        let cat = load();
        assert!(cat.get("assert").is_some());
        assert!(cat.get("closure").is_some());
    }

    #[test]
    fn lang_filter_finds_java_realizers() {
        let cat = load();
        let java: Vec<&str> = cat
            .iter()
            .filter(|c| c.has_lang("java"))
            .map(|c| c.name.as_str())
            .collect();
        assert!(java.contains(&"concept:closure"));
        // A C-only singleton must NOT show up under a java filter.
        assert!(!java.contains(&"concept:assert"));
    }

    #[test]
    fn missing_concept_returns_none() {
        let cat = load();
        assert!(cat.get("concept:does-not-exist").is_none());
    }
}
