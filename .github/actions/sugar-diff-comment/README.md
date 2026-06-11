# Sugar Diff Comment

Post the **behavior + residual delta** of a dependency bump (or any two revisions)
as a **recomputable** PR comment.

This is the distribution wedge for `sugar diff`. Every other supply-chain tool
in your pipeline reports what *passed* and asks you to trust it. This one rides
the dependency-bump PR you already review (Renovate, Dependabot, or a human) and
augments it with a verdict you can **reproduce from the proofs** — the comment
says so, because a verdict you must trust is just another vendor.

## What it reports

For the two revisions, lifting each side's minted proofs to behavior-CIDs:

- **Behavior** — `new · lost · held · renamed`, and the honest-semver `bump`.
  Names are sugar; the verdict is driven by the behavior-CID set, so a pure
  rename or reformat is *not* a change.
- **Residual** (when ledgers are supplied) — the **unproven set**:
  `undischarged` movement, `silent` drops, and whether the assertion
  **multiset** held or `MOVED`. A count-preserving member swap moves the
  multiset-CID even when every count holds.

The job's **exit code is the gate** — identical to running `sugar diff` in a
terminal. `--frozen` fails on any movement (the supply-chain pin);
`require: minor|major` is the honest-semver gate; the default fails on a lost
behavior or a grown residual.

## Usage

```yaml
# .github/workflows/dep-bump-verdict.yml
on:
  pull_request:
permissions:
  contents: read
  pull-requests: write   # to post the comment
jobs:
  sugar-diff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0           # base must be in history for --git
      - uses: TSavo/sugar/.github/actions/sugar-diff-comment@main
        with:
          require: minor           # reject a MAJOR delta dressed as a bump
          # frozen: 'true'         # or: pin byte-identical behavior
          # ledger-before / ledger-after: add the residual axis
```

## Inputs

| input | default | meaning |
| --- | --- | --- |
| `before` | PR base SHA | BEFORE revision |
| `after` | PR head SHA | AFTER revision |
| `path` | `.` | project subdir holding proofs in each revision |
| `require` | _(off)_ | honest-semver gate: `none\|minor\|major` |
| `frozen` | `false` | fail on ANY behavior/residual movement |
| `ledger-before` / `ledger-after` | _(off)_ | sweep ledgers → residual axis |
| `post-comment` | `true` | upsert the verdict as a PR comment |
| `fail-on-gate` | `true` | propagate the gate exit code to the job |
| `sugar-version` | `latest` | npm version spec for `sugar` |

The comment is **upserted** via a hidden `<!-- sugar-diff-comment -->` marker,
so re-runs edit one comment instead of spamming the thread.

## Outputs

- `comment-md` — the rendered Markdown verdict
- `verdict` — `pass` | `fail`
