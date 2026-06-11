# sugar-check

Behavioral semver for Python packages. The pip wedge for [Sugar](../../docs/explanation/behavioral-versioning.md).

```
sugar-check check --rev <git-rev> [--require none|minor|major]
sugar-check diff <a> <b>            # passthrough to `sugar diff`
```

`check` lifts the working tree and the baseline revision into **pytest-derived
behavior contracts** (the test assertion *is* the contract), diffs them, and
fails if the version bump is dishonest. The fingerprint is reformat-stable:
refactor the implementation and the behavior CID holds; only a changed promise
moves it.

## pre-commit

```yaml
repos:
  - repo: https://github.com/TSavo/sugar
    rev: <sha>
    hooks:
      - id: sugar-check
```

Requires the `sugar` binary (`SUGAR_BIN` or PATH) and `sugar_lift_py_tests`
importable. Baseline is a git revision today; the PyPI-release baseline is the
next increment.
