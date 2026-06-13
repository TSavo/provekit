# Awesome Source Audit Design

## Goal

Awesome is Sugar as the cross-language substrate: language kits lift native
source/test sugar into the same ProofIR, Rust mints and federates those claims as
`.proof`, and verification composes/refutes by canonical identity.

The first measurable surface is Java + Python source coverage, reported as a
countdown over source loci rather than a guess. For a given contract, a developer
must be able to ask:

> Where did this claim come from, what did the source walk see recursively, which
> lines are warranted, which are refused or refuted, and what remains to improve?

## Current Ground Truth

- `.proof` members carry contract bodies and source warrants, but not source
  text.
- A source warrant is a SourceMemento: file/span/params plus source/template
  CIDs.
- The source oracle resolves SourceMemento to SourceFragment by reading source
  files and recomputing CIDs.
- Java now has `JavaSourceOracle` and emits `sourceWarrants` on weak and strong
  universe contracts.
- Python reuses `sugar_lift_python_source.bind_lifter` and
  `source_oracle`; `source_memento_of` strips full body/template reconstruction
  down to the lean SourceMemento before proof-facing data sees it.
- `sugar diff` now accepts `unclassified_source` in residual ledgers and fails
  if AFTER contains unclassified source or drops the source-classification axis.

## Product Shape

The report is contract-local first and aggregate second.

For each contract:

1. Load the contract and its `sourceWarrants`.
2. Resolve each warrant through the language source oracle.
3. Recompute `source_cid` and `template_cid`.
4. Recursively walk the resolved AST/template.
5. Classify every source locus in the contract's span.
6. Render the lines with their recursive AST paths and statuses.
7. Roll the classifications into the ledger fields that `sugar diff` gates.

The aggregate Awesome countdown is the sum of those per-contract audits. It is
valid only when `unclassified_source == 0`.

## Status Semantics

Every source locus in the denominator has exactly one terminal status.

- `warranted`: this locus supports emitted ProofIR. It may point at a contract
  atom, source warrant, universe family, or table row.
- `support`: the locus is required for source resolution, name/arity mapping,
  declaration context, metadata accounting, or another non-constraint support
  role, but does not itself emit a solver constraint.
- `inactive`: the locus is a known branch or source shape that is out of scope
  for this concrete callsite relation.
- `refused`: the kit walked the locus and refused it by name because admitting
  it as a constraint would be semantically hazardous, such as side effects.
- `refuted`: the locus participates in a claim that verification proves cannot
  coexist with the vendor/source universe. This is a hard correctness result.
- `unclassified`: a bug. The source is in the denominator but the kit neither
  warranted, marked inactive/support, nor refused it. This is never accepted by
  `sugar diff`.

No report may use missing data, skipped files, parse holes, or unsupported AST
nodes as implicit success. They must become `refused` with a reason or
`unclassified` and fail the gate.

## Source Locus

A source locus is the smallest source unit the audit can classify without
lying. It is language-specific internally and language-neutral at the report
boundary.

Required fields:

```json
{
  "id": "blake3-512:<cid of canonical locus descriptor>",
  "file": "pkg/module.py",
  "span": {"start_line": 10, "start_col": 4, "end_line": 12, "end_col": 18},
  "line_range": [10, 12],
  "ast_path": "$.body.stmts[0].expr.args[1]",
  "node_kind": "Call",
  "status": "warranted",
  "reason": "python.translate-universe table row",
  "contract_name": "token_urlsafe#euf#...",
  "contract_cid": "blake3-512:<contract cid>",
  "warrant_cid": "blake3-512:<source warrant cid>"
}
```

`ast_path` is stable within the resolved template, not the raw parser object. It
is the bridge between "here are the source lines" and "here is the recursive AST
walk."

## Contract Audit Report

Each audited contract produces a JSON report member. It can be displayed by CLI,
stored as a sidecar, or later minted into `.proof` as non-normative telemetry.
The contract CID must not change when this report changes.

Shape:

```json
{
  "kind": "source-audit",
  "version": 1,
  "language": "python",
  "contract": {
    "name": "pkg.fn#euf#...",
    "cid": "blake3-512:<contract cid>"
  },
  "source_warrants": [
    {
      "kind": "source-memento",
      "file": "pkg/module.py",
      "source_function_name": "fn",
      "span": {"start_line": 10, "start_col": 0, "end_line": 20, "end_col": 0},
      "source_cid": "blake3-512:<body cid>",
      "template_cid": "blake3-512:<template cid>",
      "param_names": ["x"]
    }
  ],
  "resolved": [
    {
      "file": "pkg/module.py",
      "source_cid": "blake3-512:<recomputed body cid>",
      "template_cid": "blake3-512:<recomputed template cid>",
      "source_ok": true,
      "template_ok": true
    }
  ],
  "lines": [
    {
      "line": 14,
      "text_hash": "blake3-512:<line bytes cid>",
      "nodes": ["blake3-512:<locus id>"]
    }
  ],
  "walk": [
    {
      "id": "blake3-512:<locus id>",
      "ast_path": "$.body.stmts[0]",
      "node_kind": "Return",
      "span": {"start_line": 14, "start_col": 4, "end_line": 14, "end_col": 28},
      "status": "warranted",
      "reason": "return shape matched translate universe",
      "emits": ["blake3-512:<contract cid>"]
    }
  ],
  "totals": {
    "source_loci": 1,
    "warranted": 1,
    "inactive": 0,
    "support": 0,
    "refused": 0,
    "refuted": 0,
    "unclassified_source": 0
  },
  "source_locus_multiset_cid": "blake3-512:<canonical sorted locus ids>"
}
```

`lines.text_hash` keeps the display honest without embedding source text into
proof-bearing data. Human CLI output may show the source text because it is read
from local source files at report time; the durable report should carry hashes
and spans.

## Ledger Contract

The Awesome ledger extends the existing residual ledger. It remains a simple
JSON object so `sugar diff --ledger-before --ledger-after` can gate it.

Required fields for source-aware ledgers:

```json
{
  "corpus": "awesome/python-fixtures",
  "assert_macros": 0,
  "discharged": 0,
  "refused": 0,
  "unaccounted": 0,
  "unclassified_source": 0,
  "source_loci": 0,
  "source_warranted": 0,
  "source_inactive": 0,
  "source_support": 0,
  "source_refused": 0,
  "source_refuted": 0,
  "source_locus_multiset_cid": "blake3-512:<canonical sorted source locus ids>",
  "per_contract": []
}
```

`unclassified_source` is the hard totality gate and the product countdown.
A valid accepted target has no unclassified source. Loci should move to
`warranted` by emitting ProofIR when possible; `inactive` and `support` are
acceptable non-constraint accounting; `refused`/`refuted` are loud semantic
hazards, not backlog labels.

The next `sugar diff` enhancement after `unclassified_source` should pin
`source_locus_multiset_cid` under `--frozen`, matching the existing
`assertion_multiset_cid` behavior for assertion surfaces. That prevents a
count-preserving source-locus swap from passing as unchanged.

## Java Producer

Java must produce source audits from the same machinery that emits warrants.

Inputs:

- `JavaSourceOracle.SourceMemento`
- `JavaSourceOracle.resolve`
- the Java method template generated by `templateJson`
- the universe/test assertion families in `JavaTestAssertionsRpc`

Responsibilities:

- Resolve every `sourceWarrants` entry through `JavaSourceOracle`.
- Walk the resolved Java method template recursively.
- For each template node, classify the corresponding source span as warranted,
  refused, refuted, or unclassified.
- Emit named refusal families for unsupported nodes instead of dropping them.
- Attach emitted contract CIDs to warranted loci.
- Roll contract audits into a source-aware ledger.

Initial Java coverage should include the existing weak and strong universe
families because they already carry source warrants. Numeric, regex, CRC, MT,
instance, and error-sentinel families must either gain source warrants or appear
as named refusals in the audit when the source shape is semantically hazardous.

## Python Producer

Python must reuse `sugar_lift_python_source` for source resolution and template
shape.

Inputs:

- `source_memento_of`
- `_body_source_locator`
- `resolve_source_memento`
- `function_body_template`
- current `TranslateUniverse.source_memento`
- current `ContractDecl.source_warrants`

Responsibilities:

- Resolve every `sourceWarrants` entry through the Python source oracle.
- Walk `ast_template` recursively using stable template paths.
- Classify every template node and source line inside the warranted span.
- Keep `body_text` and raw source text out of durable proof-facing data.
- Emit named refusals for unsupported template nodes only when admitting the
  shape would be semantically hazardous; otherwise leave it unclassified until
  the kit emits the matching ProofIR, or classify it as support/inactive when it
  is genuinely non-constraint source.
- Roll contract audits into a source-aware ledger.

Initial Python coverage should include `TranslateUniverse` because it already
threads source mementos into contract warrants. Other universe families in
`translate_universe.py` must either gain source warrants or appear as named
refusals only when the source shape is semantically hazardous.

## CLI Surface

The first CLI surface should be report-oriented, not another verifier.

Proposed commands:

- `sugar awesome audit <proof-or-project> --contract <name-or-cid> --json`
- `sugar awesome ledger <proof-or-project> --json`

The audit command renders the contract-local view. The ledger command emits the
aggregate JSON accepted by `sugar diff`.

Human output for one contract should read like:

```text
contract pkg.fn#euf#... (blake3-512:abc...)
source pkg/module.py:10-20 source ok template ok

line 14  warranted  $.body.stmts[0] Return
         emits blake3-512:abc...  reason return shape matched translate universe
line 15  refused    $.body.stmts[1] If
         reason branch has side effects / unsound ordering dependency

totals source_loci=2 warranted=1 inactive=0 support=0 refused=1 refuted=0 unclassified_source=0
```

## Gates

Required gates:

- Source oracle refusal fails the contract audit unless it is surfaced as a
  named source-resolution refusal.
- Any AST/template node in the selected denominator with no classification
  increments `unclassified_source`.
- Any ledger with `unclassified_source > 0` fails `sugar diff`.
- If BEFORE had source classification and AFTER drops it, `sugar diff` fails.
- In frozen mode, source locus identity must eventually be pinned by
  `source_locus_multiset_cid`.

## Measured Countdown

Awesome coverage is not "percent of Java/Python." It is a sequence of named,
recomputable ledgers over selected corpora.

Start with two concrete source-audit targets, not an abstract fixture count.
The first milestone corpora are:

- Java Apache Commons Codec, covering both:
  - Base64, centered on `encodeBase64` / `encodeBase64String`, using the
  existing vendored Commons Codec sources under
  `implementations/java/sugar-lift-java-tests/tests/fixtures/strong-universe/`
  and the product examples under `examples/java-b64-strong/` and
  `examples/java-urlsafe-seam/`;
  - CRC32, from Commons Codec source. If no in-repo Commons Codec CRC fixture is
  present, the first implementation task must vendor or fixture the Commons
  Codec CRC source before claiming coverage.
- Python itsdangerous token-padding, centered on
  `itsdangerous.encoding.base64_encode -> urlsafe_b64encode(...).rstrip(b"=")`,
  using `examples/itsdangerous-token-padding/`.

OpenJDK CRC32C is a useful later example of the same report shape, but it is not
a first-milestone gate. The first Java denominator is Commons Codec, including
its Base64 and CRC32 surfaces.

Current measured example ledgers:

| Target | Command | Source loci | Warranted | Refused | Inactive | Unclassified |
|---|---|---:|---:|---:|---:|---:|
| Commons Codec `Base64.encodeBase64String` | `bash examples/java-codec-universe/run.sh` | 51 | 11 | 21 | 19 | 0 |
| Commons Codec `PureJavaCrc32.update(byte[], int, int)` | `bash examples/java-commons-codec-crc32/run.sh` | 29 | 15 | 1 | 13 | 0 |
| itsdangerous `encoding.base64_encode` | `bash examples/itsdangerous-token-padding/run.sh` | 15 | 8 | 7 | 0 | 0 |

The Commons CRC32 report is intentionally centered on the vendor-tested
byte-array path, not the tiny per-byte helper: the source audit warrants the
slicing-by-8 input fold and table relation in `PureJavaCrc32.java` lines
605-612, marks impossible Duff's-device switch cases inactive for the canonical
9-byte input, and leaves no unclassified source.

Each corpus reports:

- total contracts audited
- total source loci
- warranted loci
- refused/refuted loci
- refuted loci
- unclassified source loci
- source locus multiset CID

The countdown number is `unclassified_source`, partitioned by language and AST
shape. A target is not accepted until every source line in its selected source
span is warranted, inactive, support, refused/refuted, with no line left
unclassified.

## Acceptance Criteria

The first Awesome source-audit milestone is complete when:

1. Java can emit a source audit JSON for a warranted universe contract.
2. Python can emit a source audit JSON for a warranted universe contract.
3. Both reports show recursive AST/template paths and line-to-node mappings.
4. Both reports roll up into source-aware ledgers with
   `unclassified_source == 0`.
5. `sugar diff --ledger-before --ledger-after` fails when AFTER introduces
   unclassified source or drops the source-classification axis.
6. The reports do not embed source bodies in proof-facing data; source text is
   resolved from files through source oracles.
7. Commons Codec `encodeBase64` / `encodeBase64String` has a contract-local
   source audit with zero unclassified lines.
8. Commons Codec CRC32 has a contract-local source audit with zero
   unclassified lines.
9. itsdangerous `base64_encode` has a contract-local source audit with zero
   unclassified lines.

## Non-Goals

- Do not prove all Java or Python semantics in this milestone.
- Do not embed source code in `.proof`.
- Do not make source-audit telemetry affect contract CIDs.
- Do not treat unsupported AST nodes as success.
- Do not collapse witness evidence into symbolic ProofIR conjunctions.
