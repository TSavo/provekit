# python guard shapes — the 2×4 matrix

Four runtime-guard bug classes, each proved on **both** numpy and pandas — a
2×4 matrix of proven cases, zero code changes.

```sh
./run.sh
```

| # | shape | numpy violation | pandas violation |
|---|---|---|---|
| 2 | **index bounds** | `a[5]` on len-3 → `IndexError` | `s.iloc[5]` → `IndexError` |
| 3 | **empty container** | `np.array([]).max()` → `ValueError` | empty `s.iloc[0]` → `IndexError` |
| 4 | **divide by zero** | `1.0 / 0.0` → silent `inf` | `Series / 0` → silent `inf` |
| 5 | **key access** | absent structured field → `ValueError` | absent column → `KeyError` |

## Why the witness axis

These are **runtime faults**, not logical contradictions. The consistency
(z3) axis is a spec-lint: `assert a[5] == 0` is a perfectly consistent
*proposition*, so consistency cannot see the bug. Only **running the code**
catches it. So this example registers only the **pytest-witness** seat — the
axis that proves correctness by execution. It re-runs each case under real
numpy/pandas: a guarded access is witnessed (**discharged**); a violation makes
the run raise (or, for divide-by-zero, makes the finiteness assertion fail), so
the witness is **refused**.

Divide-by-zero is the sharp one: numpy and pandas do **not** raise on float
division by zero, they return `inf`. provekit catches the silent `inf` the
interpreter let through, because the `_ok` case asserts `np.isfinite(...)` and
the `_bad` case fails that assertion at run time.

## The discrimination discipline

Each cell is a pair: a guarded `_ok` case provekit must **discharge**, and a
`_bad` case that breaches the guard which provekit must **refuse**. The witness
is per-file (it runs `pytest <file>`), so `_ok` and `_bad` live in separate
files — 8 cells × 2 = 16. `run.sh` checks the verdict **per file** (every `_ok`
discharged, every `_bad` refused), so a swapped or missing verdict fails the
gate. Verified green: 8 discharged, 8 refused.

## Environment

The witness runs real numpy + pandas, so it needs them in a venv (PEP 668:
never `--break-system-packages`). `run.sh` provisions `/tmp/pandas-witness-venv`.
