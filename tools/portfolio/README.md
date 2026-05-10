# ProvekIt Portfolio Image

This directory contains the consolidated prove portfolio image. It installs every backend used by the ProvekIt verifier portfolio: z3, cvc5, Vampire, Coq `coqc`, Maude, AProVE, CeTA, CSI, Lean 4, and mathlib.

The image follows the solver seats described in:

- `protocol/specs/2026-05-02-multi-solver-protocol-v2.md`
- `protocol/specs/2026-05-10-equational-portfolio-extension.md`

## Build

Build from the repository root:

```sh
docker build -t provekit-portfolio:dev -f tools/portfolio/Dockerfile tools/portfolio/
```

The build is intentionally heavy. Lean plus the mathlib cache is large, and Vampire is built from source.

## Verify

Run the in-image verification script:

```sh
docker run --rm provekit-portfolio:dev /opt/portfolio/verify-portfolio.sh
```

The script prints `PASS` or `FAIL` for each backend and exits nonzero if any backend check fails.

## Backend Roles

| Backend | Role |
| --- | --- |
| z3 | SMT-LIB v2.6 baseline solver for first-order and arithmetic obligations. |
| cvc5 | SMT-LIB v2.6 portfolio peer, especially useful for strings and alternate SMT coverage. |
| Vampire | SMT-LIB v2.6 first-order prover seat for obligations that benefit from saturation proving. |
| `coqc` | Coq checker seat for Gallina proof artifacts and Coq-backed portfolio discharge. |
| Maude | Equational theory backend for obligations declared as `equational_theory`. |
| AProVE | Termination prover used to produce CPF certificates for the Maude CeTA gate. |
| CeTA | Certified checker for CPF termination and confluence certificates. |
| CSI | Confluence checker for the Maude CeTA gate. |
| Lean plus mathlib | Lean 4 dependent type and category theory seat, with prebuilt mathlib oleans fetched by Lake. |

## Pinned Versions

| Component | Pin |
| --- | --- |
| Base image | `ubuntu:24.04` |
| z3 | Ubuntu 24.04 apt package `4.8.12-3.1build1` |
| Coq | Ubuntu 24.04 apt package `8.18.0+dfsg-1build2` |
| Java runtime | Ubuntu 24.04 apt package `default-jre-headless=2:1.21-75+exp1` |
| cvc5 | `cvc5-1.2.1` static Linux x86_64 release |
| Vampire | `v5.0.1`, upstream tag commit `1b13eaf` |
| Maude | `3.5.1` Linux x86_64 release |
| AProVE | `master_2026_02_15` release jar |
| CeTA | `2.46` Linux x86_64 release |
| CSI | `1.2.7` Linux x86_64 release |
| Lean | `leanprover/lean4:v4.29.1` |
| mathlib | release `v4.29.1`, commit `5e932f9` |
| Rust | `stable`, minimal rustup profile |

## Lean Project

The image creates the mathlib Lake project at:

```text
/opt/lean-mathlib
```

`PROVEKIT_LEAN_PROJECT` is set to that path. The helper `provekit-lean-solve` runs `lake env lean` from the project directory.

## Combining With The Gold Image

This image is solver-first. If a single full-pipeline image is needed, extend this image and add the lifter or ingest layers:

```Dockerfile
FROM provekit-portfolio:dev
# Add the lifter and ingest layers here.
```

The inverse also works when the ingest image is the base:

```Dockerfile
FROM provekit-gold:dev
# Add the portfolio solver layers from tools/portfolio/Dockerfile.
```

Use the first form when verification is the main workload. Use the second form when ingest and lifting dominate and solver execution is an added stage.

Sign-off: T Savo
