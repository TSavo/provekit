# Lean 4 and mathlib Install Appendix

Consolidated build: see `tools/portfolio/Dockerfile` and `tools/portfolio/README.md`.

This appendix sets up the Lake project used by the verifier's Lean solver. It does not modify Dockerfiles.

Pinned versions:

- Lean toolchain: `leanprover/lean4:v4.29.1`
- mathlib release: `v4.29.1`
- mathlib release commit: `5e932f9`
- Project path used by `.provekit/config.toml`: `tools/portfolio/lean-mathlib`

## 1. Install elan

Linux and macOS:

```sh
curl https://elan.lean-lang.org/elan-init.sh -sSf | sh
source "$HOME/.elan/env"
```

Windows PowerShell:

```powershell
curl -O --location https://elan.lean-lang.org/elan-init.ps1
powershell -ExecutionPolicy Bypass -f elan-init.ps1
del elan-init.ps1
```

Install the pinned Lean toolchain:

```sh
elan toolchain install leanprover/lean4:v4.29.1
```

## 2. Create the Lake project

Run these commands from the repository root:

```sh
mkdir -p tools/portfolio/lean-mathlib
cd tools/portfolio/lean-mathlib
printf '%s\n' 'leanprover/lean4:v4.29.1' > lean-toolchain
cat > lakefile.lean <<'EOF'
import Lake
open Lake DSL

package provekit_mathlib

require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @ "5e932f9"

lean_lib ProvekitMathlib
EOF
mkdir -p ProvekitMathlib
printf '%s\n' 'import Mathlib' > ProvekitMathlib.lean
```

The verifier runs generated proof files with:

```sh
lake env lean /path/to/proof.lean
```

## 3. Resolve and cache mathlib

Run:

```sh
lake update
lake exe cache get
```

Do not build mathlib from source for the portfolio image. `lake exe cache get` downloads prebuilt `.olean` files and avoids a multi-hour local build.

## 4. Verify the project

Run:

```sh
lean --version
lake env lean --version
lake env lean - <<'EOF'
import Mathlib

theorem provekit_cache_smoke : True := by
  trivial

#print axioms provekit_cache_smoke
EOF
```

Expected result: both version commands report Lean `4.29.1`, and the smoke theorem checks without `sorryAx`.

## 5. Configure ProvekIt

The default workspace config already contains:

```toml
[solvers.lean]
binary = "lake"
ir_compiler = "lean"
timeout_seconds = 60
version = "4.x"
lake_project = "tools/portfolio/lean-mathlib"
```

If the Lake project lives elsewhere, set `lake_project` to that directory or to its `lakefile.lean`.

To force the elan proxy to a toolchain in solver config, add:

```toml
lean_toolchain = "leanprover/lean4:v4.29.1"
```
