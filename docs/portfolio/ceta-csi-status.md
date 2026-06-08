# CeTA and CSI Upstream Status

Investigated 2026-05-11. Task #41.

## Summary

Both CeTA (v2.46) and CSI (v1.2.7) binary distributions are **permanently unavailable**. The UIBK Computational Logic group website (`cl-informatik.uibk.ac.at`) has been decommissioned. No archived binaries, no GitHub mirrors, and no alternative hosting was found for either tool. The existing best-effort install pattern in `tools/portfolio/Dockerfile` is correct and should remain unchanged.

## URLs Checked and Their Status

### Primary (original) download paths

| URL | Status | Notes |
|---|---|---|
| `https://cl-informatik.uibk.ac.at/software/ceta/downloads/ceta-2.46-linux-x86_64.tar.gz` | HTTP 404 | Domain decommissioned |
| `https://cl-informatik.uibk.ac.at/software/csi/downloads/csi-1.2.7-linux-x86_64.tar.gz` | HTTP 404 | Domain decommissioned |
| `https://cl-informatik.uibk.ac.at/software/ceta/` | HTTP 404 | No redirect |
| `https://cl-informatik.uibk.ac.at/software/csi/` | HTTP 404 | No redirect |
| `http://cl-informatik.uibk.ac.at/isafor/` | HTTP 301 → 404 | Redirects to same dead domain |

### New UIBK domain attempts

UIBK reorganised under `www.uibk.ac.at/en/informatikcl/`. The CMS path exists but serves 404 for all software subpages:

| URL | Status | Notes |
|---|---|---|
| `https://www.uibk.ac.at/en/informatikcl/software/ceta/` | HTTP 404 | CMS page does not exist |
| `https://www.uibk.ac.at/en/informatikcl/software/csi/` | HTTP 404 | CMS page does not exist |
| `https://www.uibk.ac.at/informatikcl/software/ceta/downloads/ceta-2.46-linux-x86_64.tar.gz` | HTTP 302 → 404 | CMS redirect then 404 |
| `https://www.uibk.ac.at/informatikcl/software/csi/downloads/csi-1.2.7-linux-x86_64.tar.gz` | HTTP 302 → 404 | CMS redirect then 404 |

### Alternative sources

| Source | Outcome |
|---|---|
| Wayback Machine / archive.org | Zero snapshots of either tarball URL |
| Software Heritage | Origin URL not found in archive |
| GitHub search: `ceta-2.46-linux-x86_64` | Only result: this repo (`TSavo/sugar`) |
| GitHub search: `csi-1.2.7-linux` | Only result: this repo (`TSavo/sugar`) |
| GitHub: `ceta-trs/ceta`, `hezzel/csi`, `IsaFoR-CeTA/ceta` | All 404 |
| NixOS packages | No `ceta` or `csi` package in nixpkgs |
| SourceForge | 404 |
| Termination Competition Docker Hub (`termcomp/`) | 2025 competition images for AProVE, Wanda, NATT, etc.; no CeTA or CSI image |
| hezzel (Caro Fuhs, CSI author) GitHub repos | Only `cora` and `wanda`; no CSI |
| AFP `IsaFoR` / `First_Order_Rewriting` entry | Isabelle source only; binary generation requires Isabelle build toolchain |

## Root Cause

The UIBK Computational Logic group (`cl-informatik.uibk.ac.at`) was the sole distributor of CeTA and CSI binaries. That subdomain is gone as of at least 2026-05-11. The domain `cl-informatik.uibk.ac.at` returns HTTP 404 on all paths. The university CMS migration to `www.uibk.ac.at/en/informatikcl/` did not port the software download pages.

IsaFoR (the Isabelle source from which CeTA is generated) is alive as AFP entry `First_Order_Rewriting`, but the code-generation step requires a full Isabelle 2025 install plus the AFP, produces an SML binary via `isabelle build`, and adds gigabytes to the image. This is not appropriate for the portfolio Dockerfile.

## Impact on the Portfolio Image

The Dockerfile already handles this correctly:

```dockerfile
# CeTA verifies CPF certificates emitted by AProVE and CSI.
# Best effort: the uibk download URL has been unstable. If the fetch fails the
# build still succeeds, and the Maude reduce-verdict gate runs in untrusted mode
# (the portfolio falls through to vampire/coq for those obligations, per the
# equational-portfolio-extension spec). To enable the gate, install `ceta` and
# `csi` onto PATH separately. See tools/portfolio/maude-ceta-install.md.
RUN { install -d "/opt/ceta-${CETA_VERSION}" \
   && curl -fsSL \
        "https://cl-informatik.uibk.ac.at/software/ceta/downloads/ceta-${CETA_VERSION}-linux-x86_64.tar.gz" \
        ...
   ; } \
 || echo "WARNING: CeTA install skipped (download URL unavailable); the Maude/CeTA gate runs in untrusted mode"
```

The `|| echo WARNING` fallback means the image builds cleanly. `verify-portfolio.sh` uses `skip ceta` / `skip csi` when the binaries are absent, not `fail`. The portfolio falls through to vampire/coq per the equational-portfolio-extension spec.

**No Dockerfile change is needed. No install logic change is needed. The existing behavior is correct.**

## What to Do if CeTA/CSI Become Available Again

If the UIBK group republishes binaries at a new URL, or a GitHub mirror appears:

1. Update `ARG CETA_VERSION` and `ARG CSI_VERSION` in `tools/portfolio/Dockerfile`.
2. Replace the `curl` URL in the CeTA and CSI `RUN` blocks.
3. Add SHA256 verification: `echo "<sha256>  /tmp/ceta-....tar.gz" | sha256sum -c`.
4. Add a SHA256 verify step for CSI similarly.
5. Run `bash tools/portfolio/verify-portfolio.sh` in a container and confirm `PASS aprove-ceta` and `PASS csi`.
6. Commit and open a PR referencing this doc.

## Build-from-Source Option (not recommended now)

CeTA can be generated from the AFP `First_Order_Rewriting` entry via:

```sh
isabelle build -D $AFP/thys/First_Order_Rewriting CeTA
```

Constraints that make this unsuitable for the Dockerfile today:

- Requires Isabelle 2025 (~1 GB) plus AFP (~2 GB download, ~4 GB unpacked).
- Build time: 20-40 minutes, single-threaded by Isabelle session dependency.
- Output binary is SML via `isabelle process`; the generated `ceta` binary is not standalone without the Isabelle ML runtime.
- Total image size increase: ~6 GB.

If the gate becomes critical path, revisit with a separate pre-built `sugar-ceta-builder` image that caches the generated binary as a layer artifact.

Sign-off: T Savo
