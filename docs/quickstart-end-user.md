# Quickstart: mint, prove, and verify a `.proof`

This walks the path Sugar actually runs today: lift evidence into a signed,
content-addressed `.proof`, prove that the claims hold, and verify a `.proof`
by recomputation. The two numpy demos are the runnable artifacts; both run end
to end and produce the output shown below.

Correctness is `k(I) = t`. A `.proof` makes that promise checkable: the program,
its input, and its result are content-addressed and pinned, so the claim names
exactly what it is about. Verification is recomputation. You trust nothing, not
even the kit that produced the proof.

## Prerequisites

- Rust toolchain (rustup, stable channel) to build the CLI.
- `python3` and `z3` on PATH (the numpy demos provision their own venv; z3 is the
  solver).
- The Sugar repo cloned locally.

Confirm z3 is present:

```sh
z3 --version
```

## Step 1: build the CLI

From the repo root:

```sh
cargo install --path implementations/rust/sugar-cli
```

This installs `provekit` to `~/.cargo/bin/`. If that directory is not on your
PATH, add `export PATH="$HOME/.cargo/bin:$PATH"` to your shell config.

If you are working inside the repo and have already run a debug build, the binary
is at `implementations/rust/target/debug/provekit`; the demo scripts use that
path directly. Confirm whichever binary you intend to use:

```sh
provekit --version
# provekit 0.1.0
```

Confirm the install conforms to the protocol catalog it was built against:

```sh
sugar verify-protocol
```

This prints the expected and actual catalog CIDs and `status: match`. The binary
is the live authority for the catalog CID; do not trust a version number written
in prose.

## Step 2: prove a contract two ways (numpy-showcase)

`examples/numpy-showcase/` takes one real library operation, `numpy.add`, through
the full lifecycle: lift the sugar plus a `numpy.testing` contract plus a pytest
witness into one `.proof`, then prove the contract. The script provisions a venv
on first run.

```sh
./examples/numpy-showcase/run.sh
```

The `prove` step reports `discharged: 2`. The contract is discharged two
independent ways, and both agree:

```text
Sugar verifier report
  total callsites : 0
  discharged      : 2
  violations      : 0
  load errors     : 0

  [discharged]   ( -> )
      reason: test assertions mutually consistent about callsite `test_add_is_five` [solver 'z3' returned sat (counterexample found)]
  [discharged]   ( -> )
      reason: witnessed by recompute (kit): re-ran on pinned code; assertions held; witness CID reproduced
```

- CONSISTENCY: z3 proves the lifted contract is internally satisfiable. A
  self-contradictory spec is refused without running anything.
- WITNESS: the code is actually run, the run is content-addressed, and the
  verifier recomputes it.

The demo's degenerate case asserts `np.add(2,3)` both `== 5` and `== 6`. It is
refused both ways: z3 finds the conjunction UNSAT, and the actual run yields `5`,
so the `== 6` witness fails.

## Step 3: vendor a whole library, then verify it (numpy-vendor)

`examples/numpy-vendor/` is the end-to-end vendor-to-consumer flow. The universal
lifter sugar-lifts every module-level python function in the installed numpy into
one lean `.proof` (no shim, no edits to numpy), the vendor ships a separately
deployed witness package, and a consumer verifies it by recomputation.

```sh
./examples/numpy-vendor/run.sh
```

Real output (numpy 2.4.6; the function count tracks the installed numpy version):

```text
== sugar-lift ALL numpy -> numpy.proof (lean: CIDs, not inline bodies) ==
  numpy.proof:  13M, 2909 sugar members
== ship the witness PACKAGE (CID-named body, deployed separately) ==
  witness: passed blake3-512:049e169f... -> .provekit/witnesses/<cid>.witness
== VERIFY (consumer): rust recomputes; the kit oracle is untrusted ==
Sugar verification receipt
Witness dimension (rust recomputes; oracle untrusted)
  [pass] blake3-512:049e169f28547207...  (signature+content-address:package)
        oracle resolved via package; rust recomputed the CID and it matched
pass: 0 claims: 0 discharged ...
```

The `.proof` is 13M for ~2900 functions because it carries IDENTITY, not bodies:
CIDs and loci, not inline source or inline test logs. The body lives where the
ecosystem already put it (the installed numpy, the separately deployed
`<cid>.witness` package), and the verifier resolves it on demand and
recompute-verifies it. The kit that resolves the body is untrusted; the Rust CLI
BLAKE3's the body itself and compares to the pinned CID. A body that does not
recompute is refused, loudly. See
[docs/explanation/proofchain.md](explanation/proofchain.md) for the Source Oracle
and Witness Oracle.

## Step 4: inherit a contract, get caught contradicting it

The composition failure mode Sugar exists to catch: two packages each pass
their own tests, but the assembled system holds contradictory claims. This is
demonstrated end to end and locked by a committed test.

A numpy vendor mints a `.proof` carrying `np.add(2,3) == 5`. A consumer stages
that `.proof` in `.provekit/imports/`, asserts something about the same call, and
runs `prove`:

- A consumer asserting `np.add(2,3) == 6` is REFUSED. It inherits numpy's `== 5`;
  the verifier conjoins the two same-callsite contracts and z3 finds
  `and(== 5, == 6)` UNSAT.
- A consumer asserting `np.add(2,3) == 5` is PROVEN.

Run the committed end-to-end test. It needs the built CLI, numpy, and z3, plus
the python kit packages on `PYTHONPATH`; it skips cleanly when any are missing.
Using the venv the numpy demos already built:

```sh
PYTHONPATH="implementations/python/provekit-lift-py-tests/src:implementations/python/provekit-lift-python-source/src" \
  python3 -m pytest \
  implementations/python/provekit-lift-py-tests/tests/test_inheritance_e2e.py
```

Both parametrizations pass: `consumer-agrees-PROVEN` and
`consumer-contradicts-REFUSED`. See
[docs/explanation/product.md](explanation/product.md) and
[docs/explanation/architecture.md](explanation/architecture.md) for the
cross-proof contract conjoin that makes inherited correctness work.

## Step 5: try it on your own project

```sh
sugar init
```

`sugar init` scaffolds `provekit.toml`, a `.provekit/` directory, a sample
invariant, and a GitHub Action. From there the flow is:

1. Declare your lift plugins in `.provekit/config.toml` and a manifest under
   `.provekit/lift/<surface>/manifest.toml` (the numpy demo scripts show concrete
   manifests, including the witness `resolve_witness_command` /
   `resolve_witness_method` fields).
2. `provekit mint --project .` dispatches the configured lift plugins and writes a
   signed `.proof`. (`lift` dispatches the lift-plugin protocol and prints raw
   ProofIR; `mint` is the composition step that envelopes lifted terms into the
   `.proof`.)
3. `provekit prove .` loads the `.proof` artifacts, resolves dependency proofs,
   conjoins same-callsite contracts, solves the obligations, recomputes
   witnesses, and reports discharge status.
4. `provekit verify --project .` verifies a kit end to end and emits a signed
   per-claim receipt.

The exact manifest wiring depends on your kit. `sugar doctor` validates a
kit's config and manifest before a run.

## Editor integration

For Python, the red squiggle is shipped. `provekit-editor-lsp-python` is a
persistent editor language server (LSP wire protocol) that renders `provekit
prove` directly: open a file in an LSP-capable editor and a contradicted
contract surfaces as an inline error diagnostic on the offending call. On open
and save it re-evaluates your project as it is on disk (it lifts the current
source into an isolated workspace and proves it, so the squiggle tracks the
buffer and never writes to your tree) and maps each unsatisfied obligation back
to its callsite. The squiggle on `np.add(2, 3) == 6` in the Step 4 consumer is
the same verdict prove prints, and it clears the moment you fix it to `== 5`. The
server is a thin client: verification still lives in the rust CLI; the editor
just renders it.

The cross-language, daemon-routed editor server (the `provekit-lsp` /
`provekit-linkerd` binaries) is still a roadmap item. The Python server above is
the language-native path; see [docs/quickstart-extender.md](quickstart-extender.md)
for how it is built and how to add one for another language.

## When something goes wrong

`provekit: command not found`: `~/.cargo/bin` is not on your PATH (or you meant
to use the in-repo `implementations/rust/target/debug/provekit`).

The numpy demos fail to provision: they need `python3` to build a venv and `z3`
on PATH. The scripts use PEP 668 venvs and never `--break-system-packages`.

`verify-protocol` mismatch: your installed binary and the catalog it expects have
drifted. Rebuild the CLI from the current tree.

## What is next

- [docs/explanation/product.md](explanation/product.md): the product surface.
- [docs/explanation/architecture.md](explanation/architecture.md): kits own
  language, the CLI owns proof.
- [docs/explanation/proofchain.md](explanation/proofchain.md): the `.proof` as
  identity, not bodies, plus the oracle trio.
- [docs/quickstart-extender.md](quickstart-extender.md): build or extend a kit.
- [docs/reference/per-language-status.md](reference/per-language-status.md): kit
  and language coverage.
