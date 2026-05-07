# ProvekIt CI Accepted Witness Store

This directory is a checked-in CICP acceptance root. Treat it as a
supply-chain artifact, not as a generic cache directory.

CI computes each kit's current `CIBlastRadius` and looks for an accepted
`CIJobResultBodyClaim` at:

```text
.provekit/ci/accepted/<kit>/<blast-radius-cid>.job-result.json
```

If the checked-in result witness validates and names the exact current
blast-radius CID, `provekit ci reuse` emits a `CIReuseBodyClaim`. A job
may skip only after that admission succeeds.

If the witness is missing or invalid, CI runs the normal job and uploads
a candidate result witness for review. Do not accept a candidate only
because it saves time. Accept it because the source, protocol catalog,
kit/toolchain, config, and witness input closure are the closure you
intend future CI runs to trust.
