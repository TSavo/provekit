# Authenticated Betrayal Walkthrough

Run the walkthrough in order:

```sh
../tools/install-native-receipt-tools.sh
./00-start-here.sh
./01-map-package-rails.sh
./02-admit-baseline.sh
./03-show-conventional-green.sh
./04-preserve-contracts-fail-witness.sh
./05-weaken-contracts-fail-version.sh
./06-substitute-bytes-fail-binary.sh
./07-reuse-stale-ci-fail-closure.sh
./08-run-whole-exhibit.sh
```

Each script follows the Bridgeworks cadence:

1. human-readable explanation of what is about to happen;
2. pause before execution when run interactively;
3. raw `provekit` output with line numbers when JSON is shown;
4. highlighted raw lines or CIDs from the full output;
5. human analysis tying the receipts back to the claim.

The exhibit proves this claim with receipts:

```text
SLSA does not catch it.
in-toto does not catch it.
ProvekIt does.
```

The npm package is `safe-json`, a small believable JSON boundary helper. The
baseline `1.4.1` release has four contracts:

- `parse.deterministic`
- `parse.no-network-effect`
- `package.no-install-side-effect`
- `runtime.no-env-secret-read`

The poisoned `1.4.2` release is signed by the same maintainer and keeps the
same contract set, but it reads `process.env.SAFE_JSON_TOKEN` on a rare
telemetry-shaped input path.

## Receipt Flow

The visible proof work is done by:

```text
provekit package inspect
provekit lift
provekit lower
provekit mint
provekit version check-extension
provekit verify
provekit-supply-chain-rails
```

`provekit package inspect` emits the package-shaped conventional receipt set:
package identity, maintainer signature, registry metadata, tarball hash,
SLSA VSA verification, in-toto pipeline verification, SBOM contents, binary
CID, and package input-closure CID. ProvekIt invokes `slsa-verifier verify-vsa`
and `in-toto-verify`; the walkthrough does not call those tools directly.

Those receipts are intentionally green. They are useful context, but they do
not decide admission. ProvekIt admission goes red on one of the rails:

| Script | Rail | Receipt |
| --- | --- | --- |
| `02-admit-baseline.sh` | baseline | `provekit mint` emits a main `.proof` after witnesses lower successfully |
| `03-show-conventional-green.sh` | conventional context | SLSA and in-toto verifier receipts are green but admission is `not-decided` |
| `04-preserve-contracts-fail-witness.sh` | witness | lower refuses `runtime.no-env-secret-read` with `env-secret-read` |
| `05-weaken-contracts-fail-version.sh` | contract set | `oldSet subset newSet` rejects the weakened update |
| `06-substitute-bytes-fail-binary.sh` | binary | observed `binaryCid` differs from the release receipt |
| `07-reuse-stale-ci-fail-closure.sh` | CI closure | candidate `inputClosureCid` differs from the accepted baseline closure |
| `08-run-whole-exhibit.sh` | matrix | runner shows conventional green plus all red rails |

## Script Voice

Every script answers:

- What is ProvekIt doing here?
- What value does ProvekIt add beyond package-manager or provenance checks?
- Which receipt proves or rejects the initial claim?

The important sentence for the visitor:

```text
The malicious maintainer is not blocked from claiming the contract; they are
forced to lower the claim into evidence.
```

That sentence is proven twice:

1. preserved contract set, red witness rail;
2. weakened contract set, red compatibility rail.
