# rust-boundary-showcase — by-reference boundary fill (the rust mirror of numpy-showcase)

The surviving pattern after the demolition is **derive-from-source / by-reference**:
a `.proof` carries a LEAN `SourceMemento` (a locus + pinned `source_cid`/
`template_cid`, NO inline body). To get the body you ask a **Source Oracle** to
resolve it from disk; it returns the body IFF the on-disk source recomputes to
the pinned CIDs, else it REFUSES. Exact-or-refuse, no silent loss. (Supra omnia,
rectum — above all, correctness.)

Python already does this end-to-end (`numpy-showcase`, where numpy's plain
installed `rot90` is the producer — ZERO `@sugar` anywhere). This is the rust
mirror, end to end, in pure rust source:

```
vendor/   a path-dependency crate whose REAL source lives on disk. It is PLAIN
          rust — no `#[sugar::sugar]` tag, no sugar dependency. Its
          ordinary `pub fn reverse_chars` is the PRODUCER of the lean
          SourceMemento; in the `library-bindings` layer the binding is DERIVED
          from the crate name + fn name (the rust analog of numpy's plain,
          untouched `rot90`).
consumer/ a crate with a `#[sugar::boundary(library, call)]` stub
          (`unimplemented!()`) that materialize fills from the vendor's REAL
          source, CID-verified against the frozen vendor `.proof`. Boundary is
          NOT eliminated — only the sugar tag on the vendor is; the consumer
          still declares the edge it binds to.
```

The tag is gone: write a function, it's sugar. Every module-level `pub fn`
without a `#[sugar::sugar]` attribute is DERIVED into a
`library-sugar-binding-entry` (`binding_origin: "derived"`, `target_library_tag`
= the crate name, `symbol` = `<crate>.<fn>`). The lean SourceMemento it carries
is byte-identical to what the (still-supported) tagged path would emit for the
same function, so the oracle resolves a derived binding exactly as a tagged one.

## The chain (`run.sh`)

1. **derive-lift** the vendor with `SUGAR_LEAN_SOURCE=1` → a LEAN binding
   (locus + `source_cid`/`template_cid`, no inline body), DERIVED from the plain
   `pub fn` (no tag).
2. **mint** seals it into a content-addressed `.proof`, staged into the
   consumer's `.sugar/imports/`.
3. **materialize** (`sugar materialize`) finds the consumer's
   `#[sugar::boundary]` stub, asks the Source Oracle to resolve
   `reverse_chars`'s body from the live vendor crate (CID-verified against the
   frozen pin), and rewrites the stub body in place.
4. **DRIFT** — `run.sh` tampers the vendor body *after* the mint. The frozen pin
   no longer matches live disk, so materialize **REFUSES** (no write).

`run.sh` self-checks BOTH verdicts and exits non-zero if sugar does not
produce exactly them:

- the fill succeeded (verb exit 0, outcome `materialized`) and the stub body now
  equals `reverse_chars`'s REAL body (`s.chars().rev().collect()`), and the
  `unimplemented!` stub is gone;
- the drift was REFUSED (verb exit non-zero, outcome `refused`, the refusal cites
  a CID misalignment, and the stub was NOT rewritten).

## Why re-lifting the vendor would be wrong

`materialize` sources its pins from the FROZEN `.proof`, not a live
re-lift. A re-lift could never detect drift: the memento's `source_cid` would
come from the same disk read the oracle then recomputes against, so they would
match by construction. The temporal separation — pin frozen at mint, oracle
resolving against live disk — is what makes drift detectable. This is the
by-reference contract.

## Run it

```sh
cargo build -p sugar-cli --bin sugar -p sugar-walk --bin sugar-walk-rpc
./run.sh   # exits 0 on PASS
```

Everything is kit-side; the `.proof` is the transport; the rust substrate stays
proof-blind. The lift kit (`sugar-walk-rpc`) serves `lift` / `recognize` /
`sugar.plugin.materialize` as ONE source-oracle family — the same kit, the
same `syn` AST machinery, three directions over one lean `.proof`.
