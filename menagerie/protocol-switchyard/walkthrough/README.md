# Protocol Switchyard Walkthrough

A CLI-first tour of the Protocol Switchyard exhibit (paper 10). The
walkthrough mints a witnessed migration edge between two HTTP profile
catalog roots, then perturbs one rail at a time and shows the verifier
refuse.

Run the walkthrough in order:

```sh
./00-start-here.sh
./01-show-v1-profile.sh
./02-mint-v1-catalog.sh
./03-show-v2-profile.sh
./04-mint-v2-catalog.sh
./05-mint-migration-witness.sh
./06-verify-witness.sh
./10-break-spec-bytes.sh
./11-break-policy-mode.sh
./12-break-input-closure.sh
./13-break-evidence-cid.sh
./20-run-whole-exhibit.sh
```

Each script writes its inputs into a fresh temp directory under
`$TMPDIR`. The exhibit on disk under `profiles/http/` is read-only.
Break scripts mutate copies under `$TMPDIR`; the repository state never
needs restoration.

## Tour

| Script | What it shows | Receipts |
| --- | --- | --- |
| `00-start-here.sh` | Orientation; tool check; binary prep | local `provekit` and `provekit-protocol-switchyard` binaries are ready |
| `01-show-v1-profile.sh` | The two v1 obligation specs as bytes | raw spec text under `profiles/http/v1/specs/` |
| `02-mint-v1-catalog.sh` | Hash v1 specs into property CIDs and assemble the v1 catalog | `request-smuggling-refusal` and `content-length-transfer-encoding` CIDs, v1 catalog JSON |
| `03-show-v2-profile.sh` | The two v2 obligation specs and the diff against v1 | unified diff per obligation |
| `04-mint-v2-catalog.sh` | Hash v2 specs and assemble the v2 catalog | side-by-side property CIDs, v2 catalog JSON |
| `05-mint-migration-witness.sh` | Invoke `provekit protocol evolve` | `fromCatalogCid`, `toCatalogCid`, `bodyCid`, `witnessCid`, on-disk witness artifacts |
| `06-verify-witness.sh` | Invoke `provekit protocol check-evolution` against the minted witness | `ok: true`, body and catalog-diff CIDs match |

Maps to paper 10 sections 4 through 6: profiles carry obligations,
catalogs name those obligations by content CID, migration edges are
witnessed bodies that name two catalog roots plus a closure of evidence.

## Break Rails

Each break script perturbs exactly one input and runs the verifier.
The script exits non-zero only if the verifier refused with the rail
the perturbation targets. A break that "succeeds" silently is itself
a bug.

| Script | Rail perturbed | Failure mode |
| --- | --- | --- |
| `10-break-spec-bytes.sh` | catalog property layer to spec bytes | `provekit protocol evolve` refuses with `changed spec ` `request-smuggling-refusal` ` CID mismatch` because the file the `--changed-spec` flag points at hashes to a different CID than the catalog declares |
| `11-break-policy-mode.sh` | policy / body change-class agreement | `provekit protocol check-evolution` refuses with `policy accepts ` `extension-only` `, body declares ` `migration-required` after the body's `changeClass` field is rewritten |
| `12-break-input-closure.sh` | input closure of the body | verifier refuses with `inputCids missing required CID <catalogDiffCid>` after that CID is dropped from the body's `inputCids` array |
| `13-break-evidence-cid.sh` | evidence CID to evidence bytes | verifier refuses with `evidence catalogDiffCid is <X>, supplied catalog diff hashes to <Y>` after the catalog-diff bytes are tampered with under the witness dir |

Maps to paper 10 section 6's admission predicate: each rail is one
content-addressed bridge edge, and the verifier refuses the chain at
exactly the edge that broke.

## Integrator

| Script | What it shows |
| --- | --- |
| `20-run-whole-exhibit.sh` | The integrated runner (`cargo run -- --all`) emits the same four CIDs the tour produces piece by piece |

## Voice

Every script answers:

- What is ProvekIt doing here?
- What value does ProvekIt add?
- Which receipt proves the claim or refuses the chain?

The visitor's takeaway:

```text
Protocol versions are catalog roots.
Migrations are witnessed edges.
Compatibility is a checked route, not a release-note promise.
```

The break scripts prove the rails are not theatre. If the change-class
field, the input closure, the evidence-CID chain, or the spec-bytes
binding can be edited without consequence, then the green case at
script 06 is decorative. None of those edits go through.
