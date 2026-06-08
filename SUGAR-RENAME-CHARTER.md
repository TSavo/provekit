# Sugar Rename Charter
**Sugar → Sugar — The Correctness Suite**
Head of committee: Kit. Owner/decider: T. Drafted 2026-06-07. Status: DRAFT, awaiting red-pen.

---

## 1. The decision (locked)

| | |
|---|---|
| Project / brand | **Sugar** |
| Descriptor | **The Correctness Suite** (Suite ≈ Sweet; pun is a passenger, not load-bearing) |
| Pitch | **"Sugar in, `.proof` out"** (GIGO inverted; the pitch *is* the pipeline) |
| Ontology | **Sugar is the substrate. `.proof` + `.witness` are the output.** Sugar operates *on* them; it doesn't become them. |
| CLI | `sugar <verb>` — `sugar lift`, `sugar materialize`, `sugar prove`, `sugar recognize`. The name IS the command. |
| Crates / repo | `sugar-*`, `libsugar`, `TSavo/sugar` |
| `sugar`/`sugar` collision | **Embraced.** "Sugar lifts sugar." The CLI proves it reads both ways and both are true. |
| **Keep, do not rebrand** | `.proof`, `.witness` (content, not brand) · the concept word "sugar" (lifter/binding) · `k(I)=t` as the *formal model* (stays as math; it's no longer the name) |

---

## 2. Magnitude (the truth, grounded on `origin/main`)

- **17,439** occurrences of `sugar` across **1,894 files**.
- **40+** rust crates (`sugar-*`, `libsugar`, `cargo-sugar-lift`) + the python kits (`sugar-lift-py-tests`, `sugar-realize-python-core`, …).
- **416** tracked paths under `.sugar/` (the config dir → `.sugar/`).
- **~all committed `.proof` files embed a `sugar` string** (19/20 sampled): kit-ids / minting authorities live *inside* the content-addressed artifact.

This is not a find-replace. It is three layers, and the third is the deep one.

---

## 3. The three layers

### Layer A — Narrative / brand (cheap, non-conflicting, can start now)
README headline, the paper series, the tagline, external brand, `k(I)=t` framing prose. Touches **zero** crates codex builds on. Risk-free; do whenever.

### Layer B — Code (stop-the-world cutover, clean window only)
40+ crate `[package] name`s + every dependent reference + every `use sugar_*` / `sugar::` import path + the `sugar` CLI binary & subcommand help + `.sugar/` → `.sugar/` (416 paths) + every lift/realize **manifest** (they name binaries/surfaces) + Makefile + CI workflows + package names. **Collides with the live parity campaign** (codex building `sugar-lift-rust-tests`; polars/menagerie queued). Waits for a stable window.

### Layer C — Proof (re-mint, the deep one)
Because proofs are content-addressed and the content **includes the kit identity** (`sugar-lift` as authority), renaming the kits changes **every proof CID**. The corpus must be **re-minted under `sugar-*` identities** — new CIDs, new `.proof` filenames, the snake-eats-tail self-application re-runs. *The system literally re-proves itself under its new name.* This is on-thesis (identity is content), and it's why Layer B can't be hand-patched — you re-run the mints, you don't edit CIDs.

---

## 4. Naming map

| old | new |
|---|---|
| `sugar` (CLI bin) | `sugar` |
| `sugar-cli` | `sugar-cli` |
| `libsugar` / `libsugar-rpc` / `libsugar-py` | `libsugar` / `libsugar-rpc` / `libsugar-py` |
| `sugar-walk` | `sugar-walk` |
| `sugar-lift*` (contracts, rpc-client, rust-tests, py-tests, python-source, …) | `sugar-lift*` |
| `sugar-ir-*` (types, symbolic, compiler-{coq,lean,maude,smt-lib}) | `sugar-ir-*` |
| `sugar-verifier`/`linker`/`linkerd`/`macros`/`lsp`/`claim-envelope`/`proof-envelope`/`sugar`/`canonicalizer` | `sugar-*` |
| `sugar-realize-*` / `sugar-emit-*` | `sugar-realize-*` / `sugar-emit-*` |
| `.sugar/` (config dir) | `.sugar/` |
| repo `TSavo/sugar` | `TSavo/sugar` |
| package `sugar` (npm) / pip / cargo | **needs a strategy — see Risk R1** |

---

## 5. Sequence

0. **Charter approved** (this doc, red-penned).
1. **Layer A** anytime (optional early win: see "Sugar" in the README).
2. **Gate the window:** parity campaign stable — lifter merged, polars + menagerie landed, **no open codex worktree on a `sugar-*` crate.**
3. **Layer B as one scripted swing:** a single deterministic rename **script** (not manual) over crate names, import paths, `.sugar/`→`.sugar/`, manifests, Makefile, CI. Run on a fresh worktree. **Green-gate before merge:** `cargo build --workspace` + `cargo test --workspace` + acid test + numpy/pandas/rust-boundary showcases. One PR (or a tight stack), one merge.
4. **Layer C re-mint:** re-run the mints so every `.proof` carries `sugar-*` identity; commit the new CIDs; the snake-eats-tail re-runs green under the new name.
5. **Tail:** repo rename + redirects, package registries, brand rollout.

The rename script is itself an artifact worth keeping — deterministic, reviewable, reproducible. Very Sugar.

---

## 6. Risks / committee flags

- **R1 — Package namespace collision.** `sugar` is near-certainly taken on crates.io / npm / pypi. Need a strategy before Layer B: scoped (`@sugar-suite/…`), qualified (`sugar-suite`), or a registry org. **Decision needed.**
- **R2 — Proof CID re-mint is mandatory, not optional** (Layer C). Don't hand-edit; re-run mints. Budget for the full corpus + snake-eats-tail.
- **R3 — `.sugar/` back-compat.** Every example/showcase/user project reads `.sugar/`. Hard cut, or a deprecation read-window (`sugar` reads `.sugar/` then falls back to `.sugar/` for N releases)? **Decision needed.**
- **R4 — Timing.** Layer B mid-campaign shatters every in-flight worktree/goal. Hold for the stable window.

---

## 7. Open decisions for T
1. **R1** package-namespace strategy.
2. **R3** `.sugar/` hard cut vs deprecation window.
3. Do Layer A now, or fold it into the one swing so nothing is half-renamed?
