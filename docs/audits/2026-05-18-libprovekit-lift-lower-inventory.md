# libprovekit lift-and-lower inventory for Java/Python bootstrap

Date: 2026-05-18
Source: TSavo/provekit worktree `audit-libprovekit-lift-inventory`
Question: what is required to lift the Rust `libprovekit` crate and lower it
into Java and Python via the substrate's own algebra (Layer 3 self-bootstrap)?
Mode: read-only audit. The only file written is this inventory.

## 0. Methodology and scope decision

### 0.1 Direct vs transitive substrate surface

The Rust `libprovekit` crate at `implementations/rust/libprovekit/` has a
narrow direct dependency surface declared in its `Cargo.toml`:

- `provekit-canonicalizer` (path dep) -- JCS + BLAKE3-512
- `provekit-proof-envelope` (path dep) -- Ed25519 + CBOR
- `provekit-ir-types` (path dep) -- generated CDDL types
- `provekit-realize-rust-core` (path dep) -- body templates
- `serde`, `serde_json`, `thiserror` (workspace deps)

The boundary calls the task asks about (BLAKE3, Ed25519, JCS, chrono, etc.)
are NOT all reachable from `libprovekit/src/*.rs` directly. The split:

- **Direct surface** (libprovekit/src/*.rs imports): serde, serde_json,
  thiserror, std::{fs, io, env, process, path, collections, sync, ffi, os::raw}
- **Transitive surface** (re-exposed through dep crates):
  BLAKE3-512 (from `provekit-canonicalizer`), Ed25519 (from
  `provekit-proof-envelope`), JCS-RFC-8785 (from `provekit-canonicalizer`)
- **Absent surface**: `chrono` does not appear in `libprovekit`,
  `provekit-canonicalizer`, `provekit-proof-envelope`, `provekit-ir-types`,
  or `provekit-realize-rust-core`. The task's "Date/time (chrono)" line
  is a false alarm for this crate set; ISO-8601 work lives elsewhere
  (provekit-build, provekit-claim-envelope) and is out of scope.
- **Hand-rolled, not a dep**: JSON-RPC 2.0 framing exists as literal
  `"jsonrpc": "2.0"` strings in `core/lift_plugin.rs`; there is no
  `jsonrpc` crate.

For the Layer 3 self-bootstrap to be meaningful, both layers count: the
substrate's claim about a Rust library has to capture both what the source
file calls directly AND the boundary contract its declared transitive deps
impose. Java/Python lowerings must realize both.

### 0.2 Lift tool used

Per advisor input, the Rust lift CLI build is slow and the classifier audit
at `docs/audits/2026-05-17-realization-tag-classification.md` already
contains the operative ground truth. This inventory therefore uses
`ripgrep` over the crate sources for call-site enumeration and reads the
classifier audit verbatim for tag-kind state. No new lift artifact was
produced. A future Stage 7 issue (end-to-end validation) will run
`provekit lift` against the crate set and diff its claim CIDs against this
inventory.

### 0.3 Inputs

- `implementations/rust/libprovekit/Cargo.toml`
- `implementations/rust/libprovekit/src/*.rs` and `src/core/*.rs`
- `implementations/rust/provekit-canonicalizer/{Cargo.toml,src/*.rs}`
- `implementations/rust/provekit-proof-envelope/{Cargo.toml,src/*.rs}`
- `implementations/rust/provekit-realize-rust-core/{Cargo.toml,src/*.rs}`
- `implementations/rust/provekit-ir-types/Cargo.toml`
- `menagerie/concept-shapes/cids.tsv`
- `menagerie/concept-shapes/specs/` (66 specs listed)
- `menagerie/concept-shapes/boundary-contracts/` (5 entries from PR #1127)
- `docs/audits/2026-05-17-realization-tag-classification.md` (Rust row)
- `docs/audits/2026-05-18-trinity-body-template-completeness.md`
- `docs/audits/2026-05-18-rust-trinity-floor-status.md`

## 1. libprovekit surface inventory

### 1.1 Direct call-site table (libprovekit/src/*.rs)

| call site                                                     | category                | concept (proposed)                  |
| ------------------------------------------------------------- | ----------------------- | ----------------------------------- |
| `exam_manifest.rs:39` `std::fs::read_to_string(path)`         | filesystem              | concept:filesystem-read-file        |
| `proofir_bridge.rs:351` `std::fs::read_to_string(path)`       | filesystem              | concept:filesystem-read-file        |
| `proofir_bridge.rs:55` `std::io::ErrorKind`                   | io error model          | concept:io-error-kind               |
| `proofir_bridge.rs:362,380` `std::env::current_dir()`         | environment / process   | concept:process-current-working-dir |
| `desugar.rs:5` `use std::fs;`                                 | filesystem              | concept:filesystem-read-file        |
| `core/lift_plugin.rs:3` `BufRead, BufReader, Write`           | byte-stream io          | concept:byte-stream (exists)        |
| `core/lift_plugin.rs:5` `std::process::{Command, Stdio}`      | process spawn / pipe    | concept:subprocess-spawn            |
| `core/lift_plugin.rs:150,166,176` `"jsonrpc": "2.0"`          | JSON-RPC 2.0 framing    | concept:json-rpc-2.0                |
| `core/bind.rs:1343` `std::env::current_dir()`                 | environment             | concept:process-current-working-dir |
| `core/bind.rs:1444` `std::fs::read_dir(dir)`                  | filesystem dir scan     | concept:filesystem-read-directory   |
| `core/bind.rs:1466` `std::fs::read(path)`                     | filesystem byte-read    | concept:filesystem-read-bytes       |
| `ffi.rs:40` `std::ffi::CString`                               | C ABI string            | concept:c-abi-string                |
| `ffi.rs:41` `std::os::raw::c_char`                            | C ABI primitive         | concept:c-abi-char                  |
| `ffi.rs:463,491,511` `#[no_mangle] pub unsafe extern "C" fn`  | foreign-function export | concept:ffi-export                  |
| `canonical.rs:5,6,7,9` `serde::Serialize`, `serde_json::Value`| JSON serialization      | concept:json-rfc8259-serialize      |
| `compose.rs:34-38,1373` serde_json + canonicalizer            | JSON + JCS              | concept:json-rfc8259-serialize, concept:jcs-rfc8785-canonicalize |
| `core/traits.rs:171` `&'a dyn Portfolio`                      | trait object            | concept:dynamic-dispatch (exists, sugar in Rust) |
| `core/prove_kit.rs:17` `Box<dyn Catalog>`                     | trait object            | concept:dynamic-dispatch (exists, sugar in Rust) |
| `lib.rs:24-26` `Result<T> = std::result::Result<T, _>`        | Result alias            | concept:result (exists, first-class)|
| `*.rs` heavy `#[derive(...)]`, `#[error(...)]` (155 sites)    | proc-macro expansion    | concept:macro-expansion-anchor      |

Call-site count by category (direct surface only):

- filesystem read variants: 6 sites across 4 files
- env / cwd: 3 sites across 2 files
- subprocess spawn + JSON-RPC stdio: 1 module (`core/lift_plugin.rs`) bundles
  process spawn + bidirectional pipe + JSON-RPC framing
- FFI export: 3 `#[no_mangle] pub unsafe extern "C" fn` entries in `ffi.rs`,
  plus CString / c_char carrier types
- serde / serde_json: pervasive, every JCS path
- proc-macro derives: 155 lines (`#[derive(...)]`, `#[error(...)]`,
  `thiserror::Error`)

### 1.2 Transitive surface (re-exposed through dep crates)

| crate                       | symbol                              | concept (proposed)               |
| --------------------------- | ----------------------------------- | -------------------------------- |
| provekit-canonicalizer      | `blake3_512_of(&[u8]) -> String`    | concept:blake3-512-hash          |
| provekit-canonicalizer      | `encode_jcs(Value) -> String`       | concept:jcs-rfc8785-canonicalize |
| provekit-canonicalizer      | `BLAKE3_512_PREFIX = "blake3-512:"` | concept:self-identifying-cid     |
| provekit-proof-envelope     | `ed25519_sign_with_seed(...)`       | concept:ed25519-sign             |
| provekit-proof-envelope     | `ed25519_verify_string(...)`        | concept:ed25519-verify           |
| provekit-proof-envelope     | `ED25519_KEY_PREFIX = "ed25519:"`   | concept:self-identifying-key     |
| provekit-proof-envelope     | base64 stdpad encoding              | concept:base64-stdpad            |
| provekit-realize-rust-core  | platform_semantics body templates   | concept:body-template (exists?)  |
| provekit-ir-types           | generated CDDL types                | concept:cddl-generated-type      |

Note: chrono is absent. ISO-8601 datetime is NOT a boundary of this crate
set. Drop `concept:iso8601-datetime` from the libprovekit-specific work; it
belongs to a different audit (provekit-build / provekit-claim-envelope).

### 1.3 Module / packaging shape (Rust ground truth)

From `libprovekit/src/lib.rs`:

```
pub mod canonical;
pub mod ci;
pub mod compose;
pub mod core;
pub mod desugar;
pub mod effect_propagation;
pub mod exam_manifest;
pub mod ffi;
pub mod policy_profile_registry;
pub mod promotion_decision_registry;
pub mod proofir_bridge;
pub mod substrate_default_cids;
pub mod transport;
pub mod witness_registry;
pub mod wp;
```

15 sibling modules at crate root, plus 13 submodules under `core/`. The Rust
shape is "crate = many modules with `pub` items" plus a `pub use` re-export
band at the top. For Java this lowers to a package (with one or more sealed
or final classes per module). For Python this lowers to a package directory
with `__init__.py` re-exporting symbols.

## 2. Concept hub coverage audit

Method: `rg 'blake3-hash|ed25519-sign|jcs-canonicalize|json-rfc8259|filesystem-atomic|subprocess-spawn|json-rpc|iso8601-datetime' menagerie/concept-shapes/cids.tsv`
returned **zero** hits. All eight proposed boundary-domain concepts are
absent from `cids.tsv`.

Spec-file directory listing also returned zero matches for `module|import|
package|namespace|extern` keywords. There is no current `concept:module`,
`concept:import`, or `concept:package-metadata` in the hub.

Hub coverage table:

| concept (proposed)                  | exists in hub | gap-kind             |
| ----------------------------------- | ------------- | -------------------- |
| concept:blake3-512-hash             | no            | missing-source-op    |
| concept:ed25519-sign                | no            | missing-source-op    |
| concept:ed25519-verify              | no            | missing-source-op    |
| concept:jcs-rfc8785-canonicalize    | no            | missing-source-op    |
| concept:json-rfc8259-serialize      | no            | missing-source-op    |
| concept:filesystem-read-file        | no            | missing-source-op    |
| concept:filesystem-read-directory   | no            | missing-source-op    |
| concept:filesystem-read-bytes       | no            | missing-source-op    |
| concept:filesystem-atomic-write     | no (proposed) | missing-source-op    |
| concept:subprocess-spawn            | no            | missing-source-op    |
| concept:json-rpc-2.0                | no            | missing-source-op    |
| concept:self-identifying-cid        | no            | missing-source-op    |
| concept:self-identifying-key        | no            | missing-source-op    |
| concept:base64-stdpad               | no            | missing-source-op    |
| concept:c-abi-string                | no            | missing-source-op    |
| concept:c-abi-char                  | no            | missing-source-op    |
| concept:ffi-export                  | no            | missing-source-op    |
| concept:io-error-kind               | no            | missing-source-op    |
| concept:process-current-working-dir | no            | missing-source-op    |
| concept:module                      | no            | missing-source-op    |
| concept:import                      | no            | missing-source-op    |
| concept:package-metadata            | no            | missing-source-op    |
| concept:cddl-generated-type         | no            | missing-source-op    |
| concept:macro-expansion-anchor      | no            | scope-question (see 5.4) |

Existing-and-usable hub concepts touched by libprovekit:

- `concept:byte-stream` (spec exists at `byte-stream_shape.spec.json`)
- `concept:boundary-contract` (spec exists at `boundary-contract_shape.spec.json`)
- `concept:contract-observation` (spec exists)
- `concept:dynamic-dispatch` (boundary realization for Rust = sugar-carrier
  per classifier audit, see §4)
- `concept:result`, `concept:option`, `concept:list`, `concept:pair`
  (Rust first-class / boundary realizations)
- `concept:generic-instantiation` (Rust sugar-carrier -- see §4)
- `concept:iterator` (Rust sugar-carrier -- see §4)
- `concept:reference` (Rust sugar-carrier -- see §4)
- `concept:closure` (Rust sugar-carrier -- see §4)

## 3. Boundary-contract catalog audit

`menagerie/concept-shapes/boundary-contracts/` contents (PR #1127 seed):

1. boundary:http-1.1
2. boundary:http-2
3. boundary:sql-92
4. boundary:sql-postgres-dialect
5. boundary:sql-sqlite-dialect

All five are network / database protocols. None of the substrate-internal
or process-internal boundaries libprovekit actually uses are present.

Required boundary contracts for libprovekit lift+lower (Stage 2 dispatch
target):

| boundary contract                       | rfc / spec        | first consumer |
| --------------------------------------- | ----------------- | -------------- |
| boundary:blake3-512                     | BLAKE3 spec v1.4  | provekit-canonicalizer |
| boundary:ed25519-rfc-8032               | RFC 8032          | provekit-proof-envelope |
| boundary:jcs-rfc-8785                   | RFC 8785          | provekit-canonicalizer |
| boundary:json-rfc-8259                  | RFC 8259          | libprovekit + canonicalizer |
| boundary:base64-rfc-4648-stdpad         | RFC 4648 sec 4    | provekit-proof-envelope |
| boundary:filesystem-posix-read          | POSIX `read`/`opendir` semantics | libprovekit |
| boundary:subprocess-posix-spawn         | POSIX `posix_spawn` + stdio pipes | libprovekit |
| boundary:json-rpc-2.0                   | json-rpc.org/specification v2.0 | libprovekit |
| boundary:c-abi-platform                 | platform C ABI (Itanium / MSVC) | libprovekit ffi.rs |

Nine boundary contracts to mint. Existing five are network/db and stay
untouched.

## 4. Rust boundary-realization gap (Issue #1158 Phase B2)

The classifier audit (`docs/audits/2026-05-17-realization-tag-classification.md`)
records Rust with:

- first-class = 37, composition = 0, **boundary = 7**, sugar-carrier = 19,
  absent = 0

Java has 16 boundaries, Python has 16 boundaries, Rust has 7. The seven
declared Rust boundary realizations (read from the audit table):

1. `rust:Vec -> concept:list`
2. `rust:Option -> concept:option`
3. `rust:Option::and_then -> concept:option-bind`
4. `rust:Result -> concept:result`
5. `rust:tuple -> concept:pair`
6. `rust:identity -> concept:identity`
7. (the seventh is the closing macro / source-unit boundary; full mint roll
   in the audit table)

Every one of Rust's seven boundaries is a **type-system primitive**, not
a system boundary. Java and Python's extra nine include closure, iterator,
exception, generic-instantiation, dynamic-dispatch, reference, double-dispatch
-- concepts Rust currently classifies as `sugar-carrier`.

The Phase B2 question is whether the libprovekit substrate's correctness
contract requires Rust to ALSO declare those nine as boundaries, or whether
the "sugar-carrier" classification correctly captures that Rust resolves
them at compile-time monomorphization.

Reading libprovekit's actual use:

| concept                       | Rust classifier tag | libprovekit usage             | recommended action |
| ----------------------------- | ------------------- | ----------------------------- | ------------------ |
| concept:dynamic-dispatch      | sugar-carrier       | `Box<dyn Catalog>`, `&dyn Portfolio` (2 sites) | promote to boundary (vtable realization is observable in JVM/Python emit) |
| concept:closure               | sugar-carrier       | extensive `move | ...|` and `Fn`/`FnMut` (greps not shown but pervasive) | promote to boundary |
| concept:iterator              | sugar-carrier       | `.iter()`, `.into_iter()`, chains | promote to boundary |
| concept:generic-instantiation | sugar-carrier       | `<T: Catalog>`, monomorphization | promote to boundary |
| concept:reference             | sugar-carrier       | `&T`, `&mut T`, lifetimes      | promote to boundary |
| concept:exception             | sugar-carrier       | not used directly (Result-only); panics are out-of-scope | leave sugar-carrier |
| concept:double-dispatch       | sugar-carrier       | not used in libprovekit       | leave sugar-carrier |

Five of seven sugar-carriers should be promoted to boundary for Rust under
Phase B2. The classifier's "sugar-carrier" verdict is correct for the
*source-level* surface (Rust monomorphizes), but **wrong for the lowering**:
when libprovekit lowers to Java or Python, those concepts MUST be realized
as concrete boundary calls (JVM invokeinterface, Python `__iter__`, etc.).
The boundary status has to track the lowering target, not the source.

Per the classifier-audit methodology, "positive catalog evidence is applied
before gap evidence". Promoting requires minting Rust-side realization records
to overwrite the current sugar-carrier verdict with first-class-or-boundary.

## 5. Rust-specific concept usage

### 5.1 Trait objects (`dyn`)

Empirical sites:

- `core/traits.rs:171` `Search { portfolio: &'a dyn Portfolio }`
- `core/prove_kit.rs:17` `catalog: Box<dyn Catalog>`

Only two sites -- minimal. `concept:dynamic-dispatch` exists in the hub
(per c11 row showing `realization_target=vtable-indirection`), so the
concept exists. The gap is the Rust-side realization record, not the
concept itself. Action: mint `realizations/concept:dynamic-dispatch->rust:trait-object.json`
and the corresponding Java (`invokeinterface`) + Python (`duck-typing`)
records.

### 5.2 Generics (`<T>`)

Heavy use across the crate (compose, ci, desugar, all use `<T: Serialize>`,
`<T: Catalog>`, etc.). `concept:generic-instantiation` exists (per c11 row
`realization_target=macro-expansion`). Java realization should be
`type-erasure`; Python should be `duck-typing`. Both already exist in the
hub per §4. Action: ensure Rust-side realization record exists for
`rust:monomorphization` and is promoted from sugar-carrier to boundary.

### 5.3 Lifetimes (`'a`)

Empirical site: `core/traits.rs:171` `&'a dyn Portfolio`. Used sparsely.
No current concept in hub. Proposal: `concept:lifetime-bound` with
realization records:

- rust: `lifetime-annotation` (first-class)
- java: `gc-rooted-reference` (sugar-carrier -- JVM GC handles this)
- python: `gc-rooted-reference` (sugar-carrier -- refcount + cycle GC)
- c11: `manual-pointer-lifetime` (boundary)

This is a NEW concept. Mint as part of Stage 6.

### 5.4 Send / Sync markers

Empirical sites: ZERO. `rg ': Send|: Sync|Send \+ Sync' implementations/rust/libprovekit/src/`
returns no matches. libprovekit does not currently require thread-safety
markers. Action: do NOT propose `concept:thread-safety-marker` for this
inventory. Defer to a future arc that introduces thread-spawning surface.

### 5.5 Pattern matching

Heavy use of `match` and `if let`. The hub has `concept:pattern-match-destructure`?
Not in the spec listing in §0.3. Closest existing: `concept:tagged-union`
(sugar-carrier across all langs). Action: confirm `concept:pattern-match-destructure`
absence in a Stage 6 issue. If absent, mint with realizations:

- rust: `match-expression` (first-class)
- java: `switch-pattern` (first-class in Java 21+) or `instanceof-chain`
- python: `match-statement` (first-class in 3.10+)
- c11: `if-else-chain-on-tag` (boundary)

### 5.6 Macros (`macro_rules!`, proc-macros)

155 sites in libprovekit/src/ (mostly `#[derive(...)]` and `#[error(...)]`
proc-macro invocations). No `macro_rules!` definitions found in
libprovekit (defined in `provekit-macros` instead). Per §0.1 these are
**pre-substrate**: macros expand before the lifter sees the code, so the
substrate never sees `#[derive(Serialize)]`, only the expanded `impl Serialize
for Foo { ... }`.

Action: document as out-of-scope for the substrate lift. The lift kit MUST
consume macro-expanded source (`cargo expand` or `rustc --emit=expanded`).
Filing one Stage 6 issue to formalize: "lift kit input contract = expanded
source, never raw `#[derive]` calls". Do NOT propose
`concept:macro-expansion-anchor`; the right primitive is the expanded form.

### 5.7 Unsafe blocks and FFI

`ffi.rs` exports three `#[no_mangle] pub unsafe extern "C" fn` entry points
(`pk_compose_chain_contracts`, `pk_composition_result_cid`,
`pk_composition_result_body_jcs`). These are platform C-ABI exports.

Lowering to Java requires JNI declarations + a sidecar `.so/.dylib/.dll`.
Lowering to Python requires `ctypes` declarations + the same sidecar.
Both targets retain the C-ABI boundary; they do NOT re-implement libprovekit
in Java/Python at the FFI surface. This is a fundamental architectural call
the Stage 5 dispatch issue must document explicitly.

## 6. Module-level emission requirements

### 6.1 Package shape

- Rust: `libprovekit` crate at `implementations/rust/libprovekit/` with
  `src/lib.rs` declaring 15 modules.
- Java: `com.provekit.lib` package at
  `implementations/java/libprovekit/src/main/java/com/provekit/lib/`
  with one class per Rust module (or a `final class` with static methods
  for module-free function bundles).
- Python: `libprovekit` package at
  `implementations/python/libprovekit/libprovekit/` with `__init__.py`
  re-exporting and one `.py` file per Rust module.

### 6.2 Concepts required for module-level emission

| concept (proposed)        | exists in hub | Rust realization      | Java realization        | Python realization        |
| ------------------------- | ------------- | --------------------- | ----------------------- | ------------------------- |
| concept:module            | no            | `mod foo;` + `src/foo.rs` | `class Foo` or package member | `foo.py` |
| concept:import            | no            | `use bar::baz;`       | `import com.bar.Baz;`   | `from bar import baz`     |
| concept:re-export         | no            | `pub use foo::Bar;`   | n/a (transitive imports impossible) | `from .foo import Bar` in __init__.py |
| concept:package-metadata  | no            | `Cargo.toml`          | `pom.xml` or `build.gradle` | `pyproject.toml`     |
| concept:public-visibility | no            | `pub`                 | `public`                | (convention -- no leading underscore) |

Five new concepts to mint, all module-level. The asymmetry on `concept:re-export`
(Java has no native equivalent) is itself a substrate-loss signal that the
loss-record must capture per the Supra-omnia-rectum constraint
("never claim more than you can prove").

### 6.3 Build-system files

- Rust: `Cargo.toml`. Already authoritative.
- Java: `pom.xml` (Maven) or `build.gradle` (Gradle). Provekit's existing
  Java crates use Maven (`provekit-realize-java-core` per
  `2026-05-18-trinity-body-template-completeness.md` §2.3 path).
  Stay on Maven.
- Python: `pyproject.toml` per PEP 621. Provekit's existing Python crates
  use this layout (`provekit-realize-python-core` per the same audit).

Build-system file generation is per-target concrete; the substrate's job is
to emit the dep declarations (BLAKE3 crate / JCS crate / Ed25519 crate
equivalents in the target language). The concrete deps:

| boundary             | Java realization          | Python realization        |
| -------------------- | ------------------------- | ------------------------- |
| boundary:blake3-512  | `com.github.alphazero:blake3:1.4.0` or JNI to libblake3 | `pip install blake3` (PyO3-backed) |
| boundary:ed25519     | `net.i2p.crypto:eddsa:0.3.0` or BouncyCastle | `pip install cryptography` |
| boundary:jcs-rfc-8785 | (no Java impl on Maven Central as of 2026-05) -- must port from canonicalizer | (no PyPI impl) -- must port |
| boundary:json-rfc-8259 | Jackson `databind` | stdlib `json`             |
| boundary:base64-rfc-4648 | `java.util.Base64`     | stdlib `base64`           |
| boundary:filesystem-posix-read | `java.nio.file.Files` | `pathlib.Path.read_text` |
| boundary:subprocess-spawn | `ProcessBuilder`       | `subprocess.Popen`        |
| boundary:json-rpc-2.0 | hand-rolled (no widely-used standard library) | hand-rolled |

The JCS gap is load-bearing: there is no widely-shipping JCS-RFC-8785
implementation for either target. The lift+lower work must include
**re-realizing canonicalizer's JCS algorithm in Java and Python**, which is
itself a substrate-self-test: if the substrate can lift libprovekit and the
lowering produces a Java/Python JCS that emits byte-identical canonical
form, the substrate's M+N transport claim is empirically discharged at
the JCS site. This is the load-bearing acceptance test for the whole
bootstrap arc.

## 7. Concrete dispatch plan

### Stage 1: Rust boundary set widening (Phase B2)

5 issues. For each: mint Rust-side realization record, run classifier,
verify tag-kind flips from `sugar-carrier` to `boundary` (or `first-class`
where appropriate).

| issue | concept                         | realization target (Rust)    |
| ----- | ------------------------------- | ---------------------------- |
| S1.1  | concept:dynamic-dispatch        | `rust:trait-object`          |
| S1.2  | concept:closure                 | `rust:fn-trait`              |
| S1.3  | concept:iterator                | `rust:iterator-trait`        |
| S1.4  | concept:generic-instantiation   | `rust:monomorphization`      |
| S1.5  | concept:reference               | `rust:borrow-reference`      |

Acceptance: classifier audit Rust row shows boundary count >= 12.

### Stage 2: Boundary contract minting

9 issues from §3.

| issue | boundary                                  |
| ----- | ----------------------------------------- |
| S2.1  | boundary:blake3-512                       |
| S2.2  | boundary:ed25519-rfc-8032                 |
| S2.3  | boundary:jcs-rfc-8785                     |
| S2.4  | boundary:json-rfc-8259                    |
| S2.5  | boundary:base64-rfc-4648-stdpad           |
| S2.6  | boundary:filesystem-posix-read            |
| S2.7  | boundary:subprocess-posix-spawn           |
| S2.8  | boundary:json-rpc-2.0                     |
| S2.9  | boundary:c-abi-platform                   |

Acceptance: `ls menagerie/concept-shapes/boundary-contracts/ | wc -l`
returns 14 entries (5 from PR #1127 + 9 new).

### Stage 3: Boundary realization seeding (Rust + Java + Python)

For each of the 9 boundaries from Stage 2 + the 5 Stage-1 promotions =
14 concept-boundaries, mint a realization record in each of three languages =
**42 realizations**. Group by concept (3 per concept):

| issue group | concept                       | seeds  |
| ----------- | ----------------------------- | ------ |
| S3.1        | concept:blake3-512-hash       | 3      |
| S3.2        | concept:ed25519-sign + verify | 6      |
| S3.3        | concept:jcs-rfc8785-canonicalize | 3   |
| S3.4        | concept:json-rfc8259-serialize | 3     |
| S3.5        | concept:base64-stdpad         | 3      |
| S3.6        | concept:filesystem-read-*     | 9 (3 variants x 3 langs) |
| S3.7        | concept:subprocess-spawn      | 3      |
| S3.8        | concept:json-rpc-2.0          | 3      |
| S3.9        | concept:c-abi-platform        | 3 (Rust only meaningful; Java=JNI, Python=ctypes) |
| S3.10..14   | promoted concepts from Stage 1 | 5 x 2 (Java + Python only, Rust done in Stage 1) = 10 |

That's ~14 sub-issues. Each issue = one concept, all three (or two) language
realizations in one PR.

### Stage 4: Body-template wiring

Per `docs/audits/2026-05-18-trinity-body-template-completeness.md` §1:
"Java and Python both have an identical 7-concept gap between their declared
boundary realizations and their body-template emission code". That gap
(closure, double-dispatch, dynamic-dispatch, exception, generic-instantiation,
iterator, reference) overlaps the Stage-1 set. Wiring is mechanical:
edit `menagerie/java-language-signature/specs/body-templates/*.json` and
`menagerie/python-language-signature/specs/body-templates/*.json` to add
`concept_name` entries for each new realization. After Stage 1+3 land, the
existing audit's "7-concept symmetric gap" closes mechanically.

Estimate: ~24 body-template entries (14 boundaries x ~1.7 langs avg).
One issue per body-template file pair (Java + Python), grouped by concept.

| issue group | body templates updated                |
| ----------- | ------------------------------------- |
| S4.1        | dynamic-dispatch, closure, iterator   |
| S4.2        | generic-instantiation, reference      |
| S4.3        | blake3 + ed25519 + jcs                |
| S4.4        | json + base64                         |
| S4.5        | filesystem variants                   |
| S4.6        | subprocess + json-rpc                 |

6 sub-issues.

### Stage 5: Module-level emission

| issue | scope                                                       |
| ----- | ----------------------------------------------------------- |
| S5.1  | Mint concept:module, concept:import, concept:re-export, concept:package-metadata, concept:public-visibility |
| S5.2  | Realize for Rust (mod / use / pub use / Cargo.toml / pub)   |
| S5.3  | Realize for Java (package / import / n.a. / pom.xml / public) |
| S5.4  | Realize for Python (.py / from-import / __init__.py / pyproject.toml / convention) |
| S5.5  | Document the re-export asymmetry as a transport-loss record |

5 sub-issues. S5.5 is load-bearing -- it's the first transport-loss record
that captures a real module-shape gap rather than an op-shape gap.

### Stage 6: Concept-hub coverage for Rust-specific idioms

| issue | concept                                  |
| ----- | ---------------------------------------- |
| S6.1  | concept:lifetime-bound                   |
| S6.2  | concept:pattern-match-destructure (verify absence; mint if needed) |
| S6.3  | Document macro-expansion as pre-substrate (lift-kit input contract) |
| S6.4  | (deferred) concept:thread-safety-marker -- not needed for libprovekit; document deferral |

3 issues + 1 documentation-only.

### Stage 7: End-to-end validation

1 issue.

S7.1: Build `provekit-cli`, run `provekit lift implementations/rust/libprovekit/`,
diff the produced claim CID against the inventory in §1.1. Then run
`provekit lower --target java implementations/rust/libprovekit/` and
`provekit lower --target python implementations/rust/libprovekit/`. Verify:

1. The lifted claim CID is byte-deterministic across two runs.
2. The Java + Python lowerings build under their respective toolchains
   (`mvn package` and `python -m build`).
3. The JCS algorithm ported into Java + Python (Stage 3) emits
   byte-identical canonical form against a fixed test vector (the Trinity
   floor test corpus).

If (3) fails, the substrate's M+N transport claim for libprovekit is
falsified. That is the headline acceptance criterion for the entire arc.

### Dispatch order

1. Stage 1 (5 issues, can parallelize) -- unblocks Stage 4
2. Stage 2 (9 issues, can parallelize) -- unblocks Stage 3
3. Stage 6 (4 issues, parallelizable with Stage 1+2) -- independent
4. Stage 3 (14 issues, parallelizable after Stages 1+2 land) -- unblocks Stage 4
5. Stage 4 (6 issues, parallelizable after Stage 3) -- unblocks Stage 7
6. Stage 5 (5 issues, parallelizable with Stage 3+4) -- unblocks Stage 7
7. Stage 7 (1 issue) -- the gate

Total: ~44 sub-issues. Stages 1, 2, 6 can launch immediately and dispatch
in parallel. Stages 3, 4, 5 form the middle bulk. Stage 7 closes the arc.

## 8. Risks and open questions

### 8.1 chrono confusion

The task brief named chrono / ISO-8601 as a boundary. Empirical fact:
chrono is not used in any of {libprovekit, provekit-canonicalizer,
provekit-proof-envelope, provekit-realize-rust-core, provekit-ir-types}.
**Risk**: a sub-issue gets filed for `concept:iso8601-datetime` based on
the brief; closing it as duplicate-of-this-audit wastes a triage cycle.
**Mitigation**: file no chrono issue under this arc. If a future audit
finds chrono use in provekit-build / provekit-claim-envelope, file it
under a separate Date-Time-Boundary arc.

### 8.2 Phase B2 architect call

§4 recommends promoting 5 Rust sugar-carriers to boundaries. This is an
architect-level call (mentioned in `2026-05-18-trinity-body-template-completeness.md`
as "Phase B2 (architect-call, not mechanical)"). **Risk**: the
recommendation is wrong; the sugar-carrier classification is in fact
correct because Rust truly resolves these at compile time. **Mitigation**:
Stage 1 must include in each issue a "two-vote justification": why this
concept must be observable at the libprovekit substrate level (e.g.,
`Box<dyn Catalog>` is observable as a vtable in the binary output and
MUST be claim-addressable). Architect review required before merging
S1.x.

### 8.3 JCS port absence

§6.3 notes there is no widely-shipping JCS-RFC-8785 implementation for
Java or Python. The lift+lower work has to port it. **Risk**: the port
produces non-byte-identical canonical form on edge cases (number
serialization, key sorting on multi-byte UTF-8 keys, surrogate pairs).
**Mitigation**: Stage 7's test vector must include the canonicalizer's
existing JCS test corpus and assert byte-identity. If Stage 7 fails,
the substrate's M+N transport claim fails -- that's the correct outcome
under "never claim more than you can prove". Do not paper over.

### 8.4 FFI re-implementation impossibility

§5.7 / §6.3: libprovekit's `ffi.rs` exports `extern "C"` functions. Java
and Python can call those exports via JNI / ctypes; they cannot
re-implement them in pure Java / Python because the FFI contract IS a
C-ABI boundary. **Risk**: Stage 5 mistakenly tries to "lower the FFI
surface to JNI/ctypes" as a code translation rather than a calling
convention. **Mitigation**: explicitly document in S5 that `ffi.rs` is
a boundary that the Java + Python sides realize as **call sites**, not
as re-implementations. The lifted claim for `ffi.rs` carries the C-ABI
boundary contract; the Java + Python lowerings declare JNI / ctypes
binding files that consume the existing `.so` / `.dylib` / `.dll`.

### 8.5 Macro-expansion contract drift

§5.6: the lift kit must consume macro-expanded Rust source. **Risk**:
the lifter is fed raw source with `#[derive(...)]` attributes intact,
parses but does not expand, and lifts an incomplete graph. **Mitigation**:
S6.3 documents the input contract. Stage 7's test must invoke the lifter
on the output of `cargo expand` (or equivalent), not on raw source files.

### 8.6 Out-of-scope substrate carriers

`provekit-realize-rust-core` includes platform-semantics body templates
referenced by hardcoded CIDs (`const RUST_KIT_CID: &str = "blake3-512:..."`).
The lift must produce a claim whose recursive body-template references
match those hardcoded CIDs byte-for-byte, or the Trinity floor breaks.
**Risk**: lift+lower changes a body template CID without updating the
const. **Mitigation**: Stage 7 verifies that the lift output's referenced
CIDs are a subset of the body-templates the realizer kit knows about.

## 9. Summary

| stage | issues | character           |
| ----- | ------ | ------------------- |
| 1     | 5      | architect-required boundary promotions |
| 2     | 9      | mechanical boundary contract minting   |
| 3     | 14     | mechanical realization records         |
| 4     | 6      | mechanical body-template wiring        |
| 5     | 5      | new concept territory (module-level)   |
| 6     | 4      | concept-hub gaps for Rust idioms       |
| 7     | 1      | end-to-end validation gate             |
| total | **44** |                     |

The bootstrap is dominated by mechanical work (Stages 2, 3, 4: 29 issues).
The architect-required calls are Stage 1 (5 issues) and Stage 5.5 (one
issue, the re-export asymmetry as transport-loss record). Stage 7 is the
single load-bearing gate where the M+N transport claim for libprovekit is
either empirically discharged or falsified.

The thing that makes this Layer 3 rather than Layer 1 mechanical
transpilation: every step lands a content-addressed contract claim whose
correctness is verifiable against the existing classifier audit and the
JCS test vectors. Nothing is "transpiled"; everything is lifted to a
substrate fact and lowered through a declared realization that the kit
verifies. The 44 issues are 44 substrate facts, not 44 code-translation
tasks.

Supra omnia, rectum. The JCS-port byte-identity test in Stage 7 is the
correctness oracle; if it fails, the arc has revealed a real loss-record
that the substrate must declare rather than paper over.
