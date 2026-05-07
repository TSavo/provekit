# CICP Conformance Vectors

These are language-library golden vectors for the Content-Addressed CI
Protocol.

Each passing vector contains a JSON body claim. A conforming language
library must:

1. parse the body shape it supports;
2. JCS-canonicalize the exact JSON body bytes as a semantic JSON value;
3. derive the pinned BLAKE3-512 CID;
4. fail closed on the refusal vectors.

The Rust reference checker is `libprovekit::ci::check_ci_body`, surfaced
through:

```sh
provekit ci check --body protocol/conformance/cicp/<body>.json
```

These vectors are data-only conformance artifacts for catalog v1.6.2.
They do not by themselves define a new protocol catalog version.
