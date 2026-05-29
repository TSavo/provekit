# Bridgeworks White-Room Contract Stack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a runnable Bridgeworks Menagerie exhibit for checked 8-bit addition, using real `.c` lifting where available and toy lifters for the non-software domains.

**Architecture:** Follow the Bug Zoo pattern. `menagerie/bridgeworks` owns the exhibit runner, manifests, native artifacts, expected receipts, and mutation cases. The runner invokes the Rust CLI, and the Rust CLI invokes a project-local lift-plugin surface. ProofIR remains the existing FOL grammar from `protocol/provekit-ir.cddl` and `2026-04-30-ir-formal-grammar.md`; `.proof` remains the existing deterministic-CBOR format from `provekit-proof-envelope`. Bridgeworks code does not invent a private ProofIR dialect or private proof format.

**Tech Stack:** Rust/Cargo Menagerie runner, Rust `provekit` CLI, lift-plugin protocol over NDJSON, existing C lifter path for `.c`, toy Bridgeworks adapters for `.trace`, `.asm`, `.isa`, `.v`, `.blif`, `.sp`, `.md`, and `.csv`, existing `provekit-claim-envelope` and `provekit-proof-envelope` crates for contract/implication mementos and `.proof` bundles.

---

## Non-Negotiables

- Bridgeworks works like Bug Zoo: manifest-driven, local runner, expected receipts, green positive case, red mutation cases.
- The runner shells through the Rust CLI. It does not mint mementos or construct `.proof` bytes itself.
- `.c` artifacts use the existing C lift surface. If the generic `.c` lifter needs a minimal marker path for this exhibit, extend `implementations/c/provekit-lift`, not Bridgeworks.
- The remaining domains use toy lifters, but they still emit valid ProofIR FOL declarations through the lift-plugin protocol.
- The final `.proof` contains the full implication chain as a DAG of CIDs: contracts plus implication mementos. The root `.proof` CID is the 64-byte inherited handle for the whole chain.

## File Structure

Create:

- `menagerie/bridgeworks/Cargo.toml`
- `menagerie/bridgeworks/src/main.rs`
- `menagerie/bridgeworks/src/lib.rs`
- `menagerie/bridgeworks/tests/smoke.rs`
- `menagerie/bridgeworks/checked-add-u8/specimen.yaml`
- `menagerie/bridgeworks/checked-add-u8/.provekit/lift/bridgeworks-checked-add/manifest.toml`
- `menagerie/bridgeworks/checked-add-u8/kit-rpc/run-bridgeworks-lifter.sh`
- `menagerie/bridgeworks/checked-add-u8/kit-rpc/bridgeworks-lifter.{rs or ts}`
- `menagerie/bridgeworks/checked-add-u8/contracts/*.yaml`
- `menagerie/bridgeworks/checked-add-u8/artifacts/software/checked_add_u8.c`
- `menagerie/bridgeworks/checked-add-u8/artifacts/compiler/lowering.trace`
- `menagerie/bridgeworks/checked-add-u8/artifacts/compiler/toy8.asm`
- `menagerie/bridgeworks/checked-add-u8/artifacts/isa/toy8.isa`
- `menagerie/bridgeworks/checked-add-u8/artifacts/rtl/alu.v`
- `menagerie/bridgeworks/checked-add-u8/artifacts/gates/full_adder.blif`
- `menagerie/bridgeworks/checked-add-u8/artifacts/cells/cells.sp`
- `menagerie/bridgeworks/checked-add-u8/artifacts/device-physics/mosfet-switch-paper.md`
- `menagerie/bridgeworks/checked-add-u8/artifacts/experiment/bandgap-measurements.csv`
- `menagerie/bridgeworks/checked-add-u8/artifacts/experiment/calibration-note.md`
- `menagerie/bridgeworks/checked-add-u8/mutations/**`
- `menagerie/bridgeworks/checked-add-u8/expected/*.json`
- `menagerie/bridgeworks/checked-add-u8/expected/*.proof-cid`

Modify:

- `implementations/rust/Cargo.toml`
- `implementations/rust/provekit-cli/src/cmd_mint.rs`
- `implementations/rust/provekit-claim-envelope/src/lib.rs`
- `menagerie/manifest.yaml`
- `menagerie/README.md`
- `menagerie/bridgeworks/README.md`
- `docs/superpowers/specs/2026-05-08-bridgeworks-white-room-contract-stack-design.md`

---

### Task 1: Align The Spec With The Actual Architecture

**Files:**
- Modify: `docs/superpowers/specs/2026-05-08-bridgeworks-white-room-contract-stack-design.md`
- Modify: `menagerie/bridgeworks/README.md`

- [ ] **Step 1: Change source artifact from Rust to C**

Replace `checked_add_u8.rs` with `checked_add_u8.c` in the spec and README. The software layer is now native C so Bridgeworks can reuse the C lifter path.

- [ ] **Step 2: State CLI ownership**

Add this rule to the spec:

```markdown
The Bridgeworks runner invokes the Rust `provekit` CLI to run the lift-plugin
surface and mint `.proof` output. Bridgeworks owns exhibit orchestration and
fixtures; the CLI owns ProofIR validation, memento minting, implication mementos,
and deterministic `.proof` bytes.
```

- [ ] **Step 3: Commit**

```sh
git add docs/superpowers/specs/2026-05-08-bridgeworks-white-room-contract-stack-design.md menagerie/bridgeworks/README.md
git commit -m "Align Bridgeworks design with CLI-owned proof pipeline"
```

### Task 2: Extend CLI Mint To Carry The Chain DAG

**Files:**
- Modify: `implementations/rust/provekit-claim-envelope/src/lib.rs`
- Modify: `implementations/rust/provekit-cli/src/cmd_mint.rs`

- [ ] **Step 1: Expose contract property hashes**

Add a public helper beside `contract_cid`:

```rust
pub fn contract_property_hash(args: &MintContractArgs) -> String {
    let mut ph_kvs: Vec<(String, Arc<Value>)> = Vec::new();
    if let Some(pre) = &args.pre {
        ph_kvs.push(("pre".into(), pre.clone()));
    }
    if let Some(post) = &args.post {
        ph_kvs.push(("post".into(), post.clone()));
    }
    if let Some(inv) = &args.inv {
        ph_kvs.push(("inv".into(), inv.clone()));
    }
    ph_kvs.push(("outBinding".into(), Value::string(args.out_binding.clone())));
    hash_value(&Arc::new(Value::Object(ph_kvs)))
}
```

Then change `mint_contract` to call `contract_property_hash(args)` instead of duplicating that calculation.

- [ ] **Step 2: Add a test for hash consistency**

In `implementations/rust/provekit-claim-envelope/src/lib.rs` tests, mint a simple contract and assert the public helper equals the `header.propertyHash` present in the minted envelope bytes.

- [ ] **Step 3: Extend `ir-document` lift responses with optional implications**

In `cmd_mint.rs`, keep the existing `ir` field as the grammar-defined ProofIR `Declaration[]`. Add support for an optional sibling field:

```json
"implications": [
  {
    "name": "experiment-supports-device-physics",
    "antecedent": "experiment.material_parameters.within_tolerance",
    "consequent": "device_physics.mosfet_switch.valid",
    "antecedentSlot": "post",
    "consequentSlot": "post"
  }
]
```

This field is not ProofIR. It is a minting instruction for implication mementos between contract names already emitted in `ir`.

- [ ] **Step 4: Mint implication mementos into the same `.proof`**

Change `mint_from_ir_document` so it:

1. mints all `kind:"contract"` declarations as today;
2. records each contract by name with `contractCid`, `propertyHash`, and canonical memento bytes;
3. reads optional `implications`;
4. for each implication, calls `provekit_claim_envelope::mint_implication`;
5. inserts the implication memento bytes into the same `members` map;
6. builds one `.proof` envelope from contracts plus implications.

The contract set CID remains computed from contract CIDs only. The `.proof` filename CID changes whenever any contract or implication member changes, so it is the inherited root of the whole chain.

- [ ] **Step 5: Add CLI tests**

Add tests in `cmd_mint.rs` that feed `mint_from_ir_document`:

- two contracts and one implication;
- a broken implication that names a missing contract.

Expected:

- the valid case writes a `.proof` whose members include 3 mementos;
- the invalid case returns a user error naming the missing contract.

- [ ] **Step 6: Run focused tests**

```sh
cargo test --manifest-path implementations/rust/provekit-cli/Cargo.toml mint_from_ir_document
cargo test --manifest-path implementations/rust/provekit-claim-envelope/Cargo.toml contract_property_hash
```

Expected: PASS.

- [ ] **Step 7: Commit**

```sh
git add implementations/rust/provekit-cli/src/cmd_mint.rs implementations/rust/provekit-claim-envelope/src/lib.rs
git commit -m "Mint implication DAGs from lift output"
```

### Task 3: Add Bridgeworks Runner Like Bug Zoo

**Files:**
- Create: `menagerie/bridgeworks/Cargo.toml`
- Create: `menagerie/bridgeworks/src/main.rs`
- Create: `menagerie/bridgeworks/src/lib.rs`
- Create: `menagerie/bridgeworks/tests/smoke.rs`
- Modify: `implementations/rust/Cargo.toml`

- [ ] **Step 1: Create crate**

Use the Bug Zoo crate as the template. Dependencies should be `clap`, `serde`, `serde_json`, `serde_yaml`, and `provekit-canonicalizer`.

- [ ] **Step 2: Implement runner behavior**

The runner should:

1. load `checked-add-u8/specimen.yaml`;
2. run host checks declared in the manifest;
3. invoke the Rust CLI:

```sh
cargo run --manifest-path implementations/rust/provekit-cli/Cargo.toml -- \
  mint \
  --project menagerie/bridgeworks/checked-add-u8 \
  --surface bridgeworks-checked-add \
  --out menagerie/bridgeworks/checked-add-u8/out \
  --no-attest \
  --json
```

4. parse the JSON report;
5. verify the `.proof` file exists;
6. invoke `provekit dump <file>.proof --json`;
7. compare the observed proof CID, contractSetCid, and named implication edges to expected fixtures;
8. run mutation cases and require refusal.

- [ ] **Step 3: Add tests**

`tests/smoke.rs` should assert:

- `provekit-bridgeworks --help` is self-contained and does not mention `provekit zoo`;
- `cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all --json` reports one exhibit;
- the positive exhibit emits a `.proof` CID beginning with `blake3-512:`;
- mutation mode reports every red case as refused.

- [ ] **Step 4: Run focused tests and confirm red**

```sh
cargo test --manifest-path menagerie/bridgeworks/Cargo.toml -- --nocapture
```

Expected: FAIL because the exhibit and lifter are not present yet.

- [ ] **Step 5: Commit**

```sh
git add implementations/rust/Cargo.toml menagerie/bridgeworks/Cargo.toml menagerie/bridgeworks/src menagerie/bridgeworks/tests
git commit -m "Add Bridgeworks runner skeleton"
```

### Task 4: Add Checked-Add Exhibit And Native Artifacts

**Files:**
- Create: `menagerie/bridgeworks/checked-add-u8/specimen.yaml`
- Create: `menagerie/bridgeworks/checked-add-u8/contracts/*.yaml`
- Create: `menagerie/bridgeworks/checked-add-u8/artifacts/**`

- [ ] **Step 1: Write `specimen.yaml`**

The manifest should mirror Bug Zoo style and name:

- host check command;
- lift surface `bridgeworks-checked-add`;
- positive proof fixture paths;
- eight mutation cases;
- expected implication chain:

```text
experiment.material_parameters.within_tolerance -> device_physics.mosfet_switch.valid
device_physics.mosfet_switch.valid -> cells.boolean_gates.valid_in_envelope
cells.boolean_gates.valid_in_envelope -> gates.full_adder.equations
gates.full_adder.equations -> rtl.alu.refines_add8
rtl.alu.refines_add8 -> isa.add8.carry_semantics
isa.add8.carry_semantics -> compiler.lowering.preserves_checked_add
compiler.lowering.preserves_checked_add -> checked_add_u8.postcondition
```

- [ ] **Step 2: Add C software artifact**

Create `artifacts/software/checked_add_u8.c`:

```c
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    bool overflow;
    uint8_t value;
} checked_add_u8_result;

/* provekit:contract checked_add_u8.postcondition */
checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    if (wide >= 256) {
        return (checked_add_u8_result){ .overflow = true, .value = 0 };
    }
    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
```

- [ ] **Step 3: Add native toy artifacts**

Add:

- `lowering.trace` plus `toy8.asm` with `ADD8` and `BR_CARRY`;
- `toy8.isa` with unsigned carry semantics;
- `alu.v` with `assign carry = wide[8];`;
- `full_adder.blif` with `XOR3` and `MAJ3`;
- `cells.sp` with voltage/temperature/noise envelope;
- `mosfet-switch-paper.md` with explicit claim block;
- `bandgap-measurements.csv` and `calibration-note.md`.

- [ ] **Step 4: Commit**

```sh
git add menagerie/bridgeworks/checked-add-u8/specimen.yaml menagerie/bridgeworks/checked-add-u8/contracts menagerie/bridgeworks/checked-add-u8/artifacts
git commit -m "Add Bridgeworks checked-add artifacts"
```

### Task 5: Add Composite Bridgeworks Lift Surface

**Files:**
- Create: `menagerie/bridgeworks/checked-add-u8/.provekit/lift/bridgeworks-checked-add/manifest.toml`
- Create: `menagerie/bridgeworks/checked-add-u8/kit-rpc/run-bridgeworks-lifter.sh`
- Create: `menagerie/bridgeworks/checked-add-u8/kit-rpc/bridgeworks-lifter.{rs or ts}`

- [ ] **Step 1: Add project-local lift manifest**

```toml
name = "bridgeworks-checked-add"
version = "0.1.0"
protocol_version = "provekit-lift/1"
command = ["./kit-rpc/run-bridgeworks-lifter.sh"]
working_dir = "."

[capabilities]
authoring_surfaces = ["bridgeworks-checked-add"]
ir_version = "v1.1.0"
emits_signed_mementos = false
```

- [ ] **Step 2: Implement RPC protocol**

The lifter must implement `initialize`, `lift`, and `shutdown` over NDJSON. Its `lift` response must be:

```json
{
  "kind": "ir-document",
  "ir": [ "... ProofIR ContractDeclaration objects ..." ],
  "implications": [ "... chain edges by contract name ..." ],
  "diagnostics": []
}
```

The `ir` array must validate against the existing ProofIR grammar. It must contain no custom Bridgeworks-only declaration kinds.

- [ ] **Step 3: Delegate `.c` to the C lifter path**

For `checked_add_u8.c`, use the existing C lift surface. If the generic `implementations/c/provekit-lift` path cannot yet emit the contract marker, extend that existing C lifter minimally so this source lifts to `checked_add_u8.postcondition`. Do not implement a separate C parser inside Bridgeworks.

- [ ] **Step 4: Implement toy domain adapters**

The composite lifter owns these adapters:

- lowering trace and asm -> `compiler.lowering.preserves_checked_add`;
- ISA -> `isa.add8.carry_semantics`;
- RTL -> `rtl.alu.refines_add8`;
- gates -> `gates.full_adder.equations`;
- cells -> `cells.boolean_gates.valid_in_envelope`;
- paper -> `device_physics.mosfet_switch.valid`;
- measurements -> `experiment.material_parameters.within_tolerance`.

Each adapter parses native markers and emits a valid ProofIR contract declaration using existing FOL shapes: `forall`, `implies`, `and`, `atomic`, `ctor`, `var`, `const`, and `bitvec` sorts where needed.

- [ ] **Step 5: Add lifter self-test command**

The lifter script should support:

```sh
./kit-rpc/run-bridgeworks-lifter.sh --self-test
```

Expected: prints valid `ir-document` JSON and exits 0.

- [ ] **Step 6: Commit**

```sh
git add menagerie/bridgeworks/checked-add-u8/.provekit menagerie/bridgeworks/checked-add-u8/kit-rpc
git commit -m "Add Bridgeworks lift-plugin surface"
```

### Task 6: Add Mutation Cases

**Files:**
- Create: `menagerie/bridgeworks/checked-add-u8/mutations/**`

- [ ] **Step 1: Add red cases**

Add one mutation per layer:

- software drops overflow branch;
- lowering ignores carry;
- ISA uses signed overflow;
- RTL carries the wrong bit;
- gate netlist replaces XOR with OR;
- cell envelope omits noise margin;
- physics paper moves parameters outside envelope;
- measurement changes without calibration signature.

- [ ] **Step 2: Make the lifter reject weakened native artifacts**

Each adapter should return an RPC error or diagnostic that makes `provekit mint` fail closed for the mutation. The Bridgeworks runner records the missing or invalid edge named by `specimen.yaml`.

- [ ] **Step 3: Commit**

```sh
git add menagerie/bridgeworks/checked-add-u8/mutations menagerie/bridgeworks/checked-add-u8/kit-rpc
git commit -m "Add Bridgeworks mutation refusals"
```

### Task 7: Pin Expected Receipts

**Files:**
- Create: `menagerie/bridgeworks/checked-add-u8/expected/positive-mint.json`
- Create: `menagerie/bridgeworks/checked-add-u8/expected/positive-proof-inspect.json`
- Create: `menagerie/bridgeworks/checked-add-u8/expected/positive.proof-cid`
- Modify: `menagerie/bridgeworks/tests/smoke.rs`

- [ ] **Step 1: Generate fixtures through the CLI**

Run:

```sh
cargo run --manifest-path implementations/rust/provekit-cli/Cargo.toml -- \
  mint \
  --project menagerie/bridgeworks/checked-add-u8 \
  --surface bridgeworks-checked-add \
  --out menagerie/bridgeworks/checked-add-u8/out \
  --no-attest \
  --json
```

Then run `provekit dump` through the Rust CLI on the emitted proof file and save the JSON fixture.

- [ ] **Step 2: Pin runner expectations**

The smoke test should compare:

- `contractSetCid`;
- proof filename CID;
- 8 contract mementos;
- 7 implication mementos;
- all implication members have exactly two input CIDs;
- project verification passes existing `.proof` conformance checks.

- [ ] **Step 3: Commit**

```sh
git add menagerie/bridgeworks/checked-add-u8/expected menagerie/bridgeworks/tests/smoke.rs
git commit -m "Pin Bridgeworks proof receipts"
```

### Task 8: Publish Menagerie Status

**Files:**
- Modify: `menagerie/manifest.yaml`
- Modify: `menagerie/README.md`
- Modify: `menagerie/bridgeworks/README.md`

- [ ] **Step 1: Mark Bridgeworks runnable**

Update `menagerie/manifest.yaml`:

```yaml
  - id: bridgeworks
    path: bridgeworks
    runnable: true
    claim: ProofIR carries vertical contracts; .proof CIDs compress the implication DAG
    command: cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all
```

- [ ] **Step 2: Document the commands**

Add:

```sh
cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all
cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all --json
```

- [ ] **Step 3: Commit**

```sh
git add menagerie/manifest.yaml menagerie/README.md menagerie/bridgeworks/README.md
git commit -m "Mark Bridgeworks runnable"
```

### Task 9: Final Verification

**Files:**
- No edits expected.

- [ ] **Step 1: Run Bridgeworks**

```sh
cargo run --manifest-path menagerie/bridgeworks/Cargo.toml -- --all --json
```

Expected: PASS, one positive proof, eight red refusals.

- [ ] **Step 2: Run Bridgeworks tests**

```sh
cargo test --manifest-path menagerie/bridgeworks/Cargo.toml
```

Expected: PASS.

- [ ] **Step 3: Run existing Menagerie tests**

```sh
cargo test --manifest-path menagerie/bug-zoo/Cargo.toml
```

Expected: PASS.

- [ ] **Step 4: Run workspace checks**

```sh
pnpm test
pnpm build
```

Expected: PASS.

## Parallel Agent Ownership

- Coordinator: CLI mint DAG extension, Bridgeworks runner, fixtures, docs.
- C agent: `.c` checked-add artifact and existing C lifter path.
- Compiler/ISA agent: lowering trace, asm, ISA artifacts and toy adapter.
- Hardware agent: RTL, gate, and cell artifacts and toy adapters.
- Science agent: device-physics paper, measurement artifacts, and toy adapters.

Agents must coordinate only through contract sheets and named ProofIR claims. No agent should hard-code sibling implementation details.

## Self-Review Checklist

- Uses existing ProofIR grammar, not a private Bridgeworks grammar.
- Uses existing Rust CLI mint/proof path, not runner-owned `.proof` bytes.
- Uses C lifter path for `.c`.
- Makes only non-C domains toy lifters.
- Produces one `.proof` whose CID covers contracts plus all `p -> q` implications.
- Preserves the Bug Zoo Menagerie runner pattern.
