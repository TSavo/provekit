# BZ-OWNERSHIP-001: Borrowed Pages as Scratch

**Kingdom**: ownership  
**Missing edge**: `borrowed(buf) => not_aliased(dst, src)`

## Pattern

A function receives a buffer it does not own (borrowed from the caller) and uses
it as scratch space, clobbering the caller's data.  The contract the caller
expects -- that its buffer is read-only for the function's lifetime -- is
violated.

The bug manifests when the same pointer serves as both source and destination in
an operation that writes to its destination.  The canonical kernel-level shape:
`skcipher_request_set_crypt(req, sg, sg, ...)` where the same scatter-gather
list appears as both `src` and `dst`, causing an in-place crypt that overwrites
the caller-owned skb fragments.

## Exhibit (C)

`process_borrowed_buf(char *buf, ...)` receives a caller-owned buffer and
searches it for a target byte, but also zeroes each byte as it scans --
destroying the borrowed data.  The function's `BUG_ON` assertions capture:

```
pre = AND(buf != NULL, buf_len != 0, used <= buf_len)
```

No `dst != src` constraint exists.  A caller who passes the same buffer as both
input and (implicitly) output receives a function that silently zeros their data.

## Fixed (C)

`process_buf_to_dst(const char *src, char *dst, ...)` takes separate src and dst
parameters.  The explicit `BUG_ON(dst == src)` assertion adds:

```
pre = AND(src != NULL, dst != NULL, dst != src, buf_len != 0, used <= buf_len)
```

The `neq(dst, src)` precondition is now machine-checkable in the ProofIR.

## Composition gate

The missing edge fires (red) when the witness `neq(buf, NULL)` -- a non-null
borrowed buffer -- fails to imply `neq(dst, src)` (the non-aliasing requirement).
The fixed gate discharges (green) because the fixed function's own precondition
`neq(dst, src)` trivially implies `neq(dst, src)`.

## Wild sightings

- **CVE-2026-43500** (`rxkad_verify_packet_1`, `net/rxrpc/rxkad.c`): the
  `skcipher_request_set_crypt` call passes the same `sg` as both src and dst,
  clobbering caller-owned skb fragments during AFS decryption.
- **rxkad_verify_packet_2** (substrate-discovered sibling, same file): found
  structurally by the `borrowed-pages-as-scratch-v2.sql` predicate against the
  Linux kernel substrate; shares the same ownership-violation shape.

See `menagerie/pattern-predicate-protocol/experiments/2026-05-09-linux-kernel-net.md`
for the predicate run log.
