# `sugar-verify`: GitHub Action

Run the Sugar standing-invariant gate as a GitHub Actions step. This is
Channel 1 of the Sugar distribution surface (see
[`docs/specs/2026-04-27-constraint-driven-development.md`](../../../docs/specs/2026-04-27-constraint-driven-development.md): "Distribution: two channels"): every developer adds it to their CI.

## Quick start

Drop this into `.github/workflows/sugar.yml`:

```yaml
name: Sugar
on: [push, pull_request]

jobs:
  prove:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - uses: sugar/sugar/.github/actions/sugar-verify@main
```

The action wraps `npx sugar verify --ci`. Exit code 1 (a
violation) fails the workflow. Exit code 2 (decay) fails by default and can be
demoted to a warning by setting `fail-on-decay: false`. Exit code 3 (internal
error) always fails.

## Inputs

| Name                | Default | Description |
| ------------------- | ------- | ----------- |
| `working-directory` | `.`     | Directory to `cd` into before running `sugar verify`. |
| `fail-on-decay`     | `true`  | When `false`, exit code 2 (decay) is treated as success: useful while the constraint corpus is still settling. |
| `verbose`           | `false` | Pass `--verbose` to `sugar`. |
| `sugar-version`  | `latest` | npm version spec passed to `npx -y "sugar@<version>"`. Pin a known-good version in production. |

## Outputs

| Name          | Description |
| ------------- | ----------- |
| `report-json` | JSON-encoded `VerifyReport` from the run. Useful as input to a PR-comment step. |
| `summary-md`  | Markdown summary suitable for posting as a PR comment. The action also writes this to `$GITHUB_STEP_SUMMARY` automatically. |

## Wiring outputs to a PR comment

```yaml
      - id: prove
        uses: sugar/sugar/.github/actions/sugar-verify@main
        with:
          fail-on-decay: false

      - if: always() && github.event_name == 'pull_request'
        uses: marocchino/sticky-pull-request-comment@v2
        with:
          header: sugar
          message: ${{ steps.prove.outputs.summary-md }}
```

## Note on this repository

This action lives inside the Sugar repo so consumers can reference it as
`sugar/sugar/.github/actions/sugar-verify@main`. Sugar itself
does not consume this action against its own codebase: see
[`.github/workflows/sugar-example.yml`](../../workflows/sugar-example.yml)
for a copy-paste-ready template that downstream repositories can use.
