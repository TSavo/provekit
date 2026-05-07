# ProvekIt CI Accepted Witness Store

This directory is a checked-in CICP acceptance root.

CI computes each kit's current `CIBlastRadius` and looks for an accepted
`CIJobResultBodyClaim` at:

```text
.provekit/ci/accepted/<kit>/<blast-radius-cid>.job-result.json
```

If the checked-in result witness validates and names the exact current
blast-radius CID, `provekit ci reuse` emits a `CIReuseBodyClaim` and the
job may skip. If the witness is missing or invalid, CI runs the normal
job and uploads a candidate result witness for review.
