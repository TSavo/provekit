# Tutorial: Rust

A five-minute walkthrough for Rust developers. By the end you have a `.proof` catalog of signed contract mementos for a small Rust crate, you have verified the install conforms to the protocol catalog at CID `blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d` (v1.6.3), and you have run `sugar prove` against the catalog.

> **Other languages:** see [tutorials/](./) for TypeScript, Python, Java, C#, Ruby, Zig, and the [polyglot stack walkthrough](polyglot-stack.md). The Rust CLI is the canonical implementation; non-Rust kits use it for verification today.

For the current end-user quickstart (get a red squiggle in 10 minutes):

> [docs/quickstart-end-user.md](../quickstart-end-user.md)

## Step 1: install the source-built CLI

From the repository root:

```bash
cargo install --path implementations/rust/sugar-cli
```

The installed binary is `sugar`. It is the canonical Rust implementation for protocol v1.6.3; alternative implementations in other languages conform to the same catalog CID.

## Step 2: confirm protocol conformance

```bash
sugar verify-protocol
```

This reads the local CLI's declared catalog CID, recomputes every spec CID listed in the catalog from the spec bytes shipped with the install, and confirms the catalog hashes to the expected value:

```
blake3-512:dd0cc79889ee67d2594f5cfa20a191bafed15196fb2c5036f85deced7cd976055ae93825edebc10812b6fcf3c6ccf274fbc1137f32705aa0dc5938dc5825e31d
```

A mismatch means either the install is corrupted or the binary was built against a different protocol version. The exit code is 0 on conformance, 1 on drift.

## Step 3: lift a workspace

```bash
cd path/to/your-rust-crate
cargo sugar-lift
```

`cargo sugar-lift` walks the workspace, runs every registered lift adapter (today: `sugar-lift-proptest` and `sugar-lift-contracts`), canonicalizes the discovered annotations into IR, hashes each IR formula to a CID, wraps the formula in a contract memento envelope, signs the envelope with the local signing key, and writes the catalog to `target/.proof`.

The output looks something like:

```
sugar-lift: workspace root /path/to/your-rust-crate
  scanning crate my_crate ...
    proptest adapter: 4 properties lifted
    contracts adapter: 12 pre/post pairs lifted
  total mementos: 16
  catalog CID: blake3-512:b6d7c2772c2929...
  written to: target/.proof
```

The `.proof` is portable. Ship it alongside the crate's bytes; consumers find it during their own verifier walk.

## Step 4: verify

```bash
sugar prove
```

`sugar prove` walks `<projectRoot>` plus the dependency tree's `.proof` files, indexes the memento pool, runs the three-tier handshake at every call site, and reports the discharge breakdown:

```
sugar prove: project /path/to/your-rust-crate

memento pool:
  contracts:     16
  implications:   0
  bridges:       16

handshake breakdown:
  total call sites:        47
  discharged by hash:      31    (66%)
  discharged by cache:      0    ( 0%)
  discharged by solver:     9    (19%)
  flagged per call site:    7    (15%)
  violations:               0    ( 0%)

hash-discharge fraction: 0.66
```

The hash-discharge fraction is the headline metric: the share of call sites discharged at Tier 1 alone. A high fraction means contracts compose well across the workspace. A low fraction means real work to do; the work is the residue, not the average case.

When Tier 3 fires, the verifier mints a fresh implication memento for each `(post, pre)` pair Z3 discharges. The mementos are written per the publish policy in `sugar.config.yaml`:

```yaml
publish:
  implications:
    target: project    # one of: local, project, registry
```

`local` keeps mementos in `~/.sugar/cache/`. `project` writes them into the project's `.proof`. `registry` pushes to a configured implication server (a passive indexer; mementos remain signed by the original prover). Defaults to `local`.

## Step 5: inspect

```bash
sugar dump target/.proof
```

`sugar dump` pretty-prints a `.proof` envelope: members, bodies, signatures, and recomputed CIDs. Use it to confirm what the lift adapter actually produced and what is shipping in the catalog.

```bash
sugar hash <some-file>
```

`sugar hash` computes the BLAKE3-512 self-identifying CID of any file. Use it to verify your local install's spec bytes against the published CIDs in `protocol/specs/2026-04-30-protocol-catalog.json`.

```bash
sugar search --consequent some-formula.json
```

`sugar search` searches the catalog by content. "Find every contract whose post-condition canonicalizes to this CID" or "find every implication memento with this consequent" is grep over the memento pool.

## What's next

- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md) for the adoption guide per source library.
- [docs/reference/per-language-status.md](../reference/per-language-status.md) for the matrix of kits, libs, and adapters across all host languages.
- [docs/explanation/architecture.md](../explanation/architecture.md) for the four-layer model, the handshake, and the lattice tractability theorem.
- [docs/explanation/thesis.md](../explanation/thesis.md) for the deeper architectural claim.
- [protocol/specs/](../../protocol/specs/) for the canonical specs, addressed by CID.

## Troubleshooting

**`sugar verify-protocol` exits with code 1.** The local install's spec bytes do not hash to the expected catalog CID. Either the install is corrupted (re-run `cargo install --path implementations/rust/sugar-cli`) or the binary was built against a different protocol version (check `sugar version`).

**`cargo sugar-lift` reports zero mementos.** No lift adapter recognized any annotations in the workspace. Today's shipping adapters cover `proptest!` blocks and `#[contracts::requires]` / `#[contracts::ensures]` macros; if your crate uses a different annotation library, the adapter is on the v1.2 roadmap (see [per-adapter-coverage.md](../reference/per-adapter-coverage.md)).

**`sugar prove` reports a large `flagged per call site` count.** Tier 3 fell back to per-call-site Z3 because no `(post, pre)`-level discharge was possible. This is expected for the first run on a new codebase; subsequent runs benefit from the cached implication mementos minted on the first run, and the residue shrinks.

**Z3 not found.** Install Z3 (`brew install z3`, `apt install z3`, etc.). Tier 1 and Tier 2 of the handshake do not require Z3, but Tier 3 does, and the first run on any non-trivial codebase will hit Tier 3 at least once.
