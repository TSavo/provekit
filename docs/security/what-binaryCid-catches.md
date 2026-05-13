# What `binaryCid` catches

`binaryCid` pins the BLAKE3-512 hash of the compiled artifact. Under v1.4, it is **one axis of three** in the consumer's rank-3 pin `(contractCid, witnessCid, binaryCid)`. Each axis catches a distinct attack class; this doc covers `binaryCid` specifically.

When `binaryCid` is set in a `.proof` bundle, the verifier:

1. Computes BLAKE3-512 of the running compiled binary.
2. Compares against `binaryCid`.
3. If they don't match, rejects the proof.

This is the binary-axis supply-chain anchor. It catches the attack class where an adversary substitutes the binary while leaving the contract claim intact. Under v1.4, this composes with the contract-axis (`contractCid`) and witness-axis (`witnessCid`) checks per [`multi-dimensional-pinning.md`](multi-dimensional-pinning.md); together the rank-3 pin closes attack classes single-axis pinning leaves open.

## What "the binary" means per kit

The compiled artifact varies by language:

- **Rust**: the `.rlib` or executable produced by `cargo build`.
- **TypeScript/JavaScript**: the bundled `.js` (after webpack / esbuild / rollup) or the `.js` files served at runtime.
- **Python**: the bytecode `.pyc` files, or the source `.py` files (Python's "binary" is ambiguous; convention is to pin source bytes).
- **Java**: the `.class` files or the packaged `.jar`.
- **C / C++**: the linked executable or shared library.
- **Go**: the linked executable.
- **C#**: the compiled `.dll` / `.exe`.
- **EVM bytecode**: the deployed contract bytecode at a specific address.
- **Solana BPF**: the compiled BPF bytecode.
- **WASM**: the `.wasm` module.

The kit's documentation specifies which artifact `binaryCid` pins for that language. The hash is computed over the artifact's bytes exactly as they're loaded for execution.

## Catches: compiler backdoors that produce different output

Scenario: an attacker compromises your compiler. The compiler now produces malicious binaries from honest source.

The `.proof` was minted by an honest developer with an honest compiler at some earlier time, with `binaryCid` pinning the honest artifact. The attacker's compiler produces a different artifact whose hash does not match `binaryCid`. The verifier rejects.

This catches Thompson's "Trusting Trust" attack *as long as* the `.proof` was minted before the compiler was compromised. After the compromise, future `.proof` files would be minted with the malicious binary's CID, and the verifier would accept the malicious binary as matching its own CID. So `binaryCid` catches "compiler subverted between proof mint and consumer build" but not "compiler subverted before proof mint."

Defense in depth: reproducible builds. If multiple independent builds of the same source produce the same `binaryCid`, the consumer can compare across vendors and detect divergence even within the "subverted before proof mint" window.

## Catches: runtime injection of patched binaries

Scenario: an attacker compromises your CI or build environment and replaces a dependency's compiled binary with a malicious one before the consumer's `prove` step.

The consumer's verifier loads the (legitimate, signed) `.proof` bundle from the dependency's distribution. The bundle's `binaryCid` is the hash of the legitimate compiled binary. The attacker has substituted a different binary in the consumer's build environment. The verifier computes the running binary's hash, compares against `binaryCid`, sees mismatch, rejects.

This catches dependency injection attacks where the malicious code is delivered as a tampered binary (vs. a tampered source).

## Catches: dependency confusion via different binaries

Scenario: an attacker registers a package with a legitimate-sounding name on a public registry, hoping the consumer will accidentally pull theirs instead of the intended one (the "dependency confusion" attack class).

If the consumer pulls the wrong package, that package's `.proof` (if it has one) has `binaryCid` matching the wrong package's binary. If the consumer's expected `.proof` (or the consumer's expectations) require a specific `binaryCid`, the wrong package's `binaryCid` won't match.

This is a layered defense. The first line is naming and namespace discipline. The second line is `binaryCid` pinning specifically.

## Catches: monkey-patching at module load

Scenario: an attacker injects code that overrides a function at module load (Python `import`-time monkey patches, JavaScript `require`-time prototype pollution, Java `static` initializers).

If the patch changes the loaded module's bytes (because the attacker added a wrapping function), the module's binary CID changes. The `binaryCid` mismatch catches it.

Note: monkey patches that modify *instance state* without modifying *loaded bytes* are not caught by `binaryCid`. They require the kit's runtime guard (which checks the resolved function's CID at the call site, not just the module's hash). See [threat-model.md](threat-model.md) for the runtime-guard discussion.

## Catches: cross-platform binary substitution

Scenario: a `.proof` was minted on Linux (x86_64). The consumer is running on Windows or macOS or ARM. The compiled binary differs by platform.

`binaryCid` pinning catches this: the Linux `.proof` doesn't match the Windows binary's hash. The verifier rejects.

This is a feature, not a bug. Cross-platform binary differences are real; the `.proof` should be platform-specific. Kits that ship multi-platform should mint multiple `.proof` bundles (one per platform), each pinning the right binary.

## Catches: stale bundles pointing at old binaries

Scenario: a `.proof` was minted six months ago. The dependency has shipped an update. The consumer pulled the new dependency but is still using the old `.proof`.

The new binary's hash doesn't match the old `.proof`'s `binaryCid`. The verifier rejects, prompting the consumer to fetch a fresh `.proof` matching the current binary.

This is an essential property: `.proof` files cannot drift out of sync with their binaries undetected. Either the proof matches the binary, or it doesn't. There is no middle ground.

## Doesn't catch: malicious source whose compiled output matches

If an attacker writes malicious source code, signs the contract claiming the function does X (when it actually does Y), and ships the malicious binary alongside the lying `.proof`, then `binaryCid` matches. The attacker wrote the binary that the proof pins; mismatch is impossible.

This is the non-catch from [threat-model.md](threat-model.md): the signature attests to the signer, not to truth. `binaryCid` ensures consumers run the binary the signer intended; it does not ensure the signer's intent was honest.

Mitigation: trust decisions about whose signatures to accept.

## Doesn't catch: source-level changes that don't affect compiled bytes

Scenario: an attacker rewrites the source to remove security checks, then the compiler optimizes the removed checks away, producing the same binary as the honest source.

`binaryCid` matches. The check is silent. The verifier accepts.

This is rare in practice because source changes that meaningfully affect security usually affect compiled bytes. But it's possible in edge cases (e.g., dead-code elimination of a check that the compiler considered redundant).

Mitigation: source-level audits in addition to binary-level.

## Operational use

Kits that produce a compiled artifact should always set `binaryCid`. The cost is small (one hash computation per artifact); the value is large (closes the supply-chain anchor).

Kits that don't produce a compiled artifact (lift adapters, IR-only outputs) should not set `binaryCid`. Setting it to a wrong or arbitrary value undermines verification.

Kits that produce multiple deployable artifacts (e.g., a Rust crate with both a `lib.rs` and a `bin/main.rs`) should produce one `.proof` per artifact. Each pins its own `binaryCid`.

## Performance

`binaryCid` verification is one BLAKE3-512 hash computation per build. On modern hardware, this is sub-millisecond for typical artifact sizes (a few MB). The overhead is below the noise floor.

## Read next

- [what-binaryCid-does-not-catch.md](what-binaryCid-does-not-catch.md): the limits.
- [supply-chain.md](supply-chain.md): supply-chain attack scenarios in depth.
- [threat-model.md](threat-model.md): full threat coverage matrix.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md): what the signature buys.
