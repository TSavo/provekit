# Experimental Record: 2026-05-09 — Linux kernel net/ substrate, five-predicate barrage

## Context

This record captures the experimental run that motivated and grounds
the [Pattern Predicate Protocol spec](../../../protocol/specs/2026-05-09-pattern-predicate-protocol.md).

The hypothesis under test: an editorially-named bug class (here,
[`borrowed-pages-as-scratch`](https://nefariousplan.com/patterns/borrowed-pages-as-scratch))
can be compiled into a deterministic SQL query against a substrate
produced by the C lifter, and the query's result set on a real kernel
codebase will identify both the publicly-disclosed instances of the
class and substrate-discovered same-class siblings, with a usable
true-positive rate.

## Substrate

| Item | Value |
|---|---|
| Source tree | `linux@7.1.0-rc2` (`torvalds/linux` HEAD as of fetch on 2026-05-09 morning) |
| Subtrees lifted | `net/ipv4`, `net/ipv6`, `net/rxrpc`, `net/core`, `net/xfrm` |
| Files lifted | 347 `.c` files |
| Call edges produced | 48,701 |
| Lifter | `implementations/c/provekit-lift-c-kernel-doc` from main at `0219939d` plus the in-flight RecoveryExpr fix (PR #510, since merged at `f8be0e0d`) |
| Lift wall time | 88 seconds |

The substrate database (`substrate.db`) is a SQLite file with the
schema described in PPP Section 3.1: `call_edges` populated from each
file's lift output, `lifted_files` recording per-file edge counts and
elapsed time. `functions` and `contracts` are populated by the v1
schema but not used by the predicates run in this experiment.

## Predicates run

Five `.sql` predicates from `predicates/` were applied to the
substrate. Each predicate's content-hash is its (exploratory) CID
under blake2b-512. The proper PPP CIDs would be BLAKE3-512; these
hashes are stable identifiers within this exploratory run.

| Predicate | Match count | Notes |
|---|---:|---|
| `borrowed-pages-as-scratch.sql` (v1) | 5 | Function-local; 40% TP rate |
| `borrowed-pages-as-scratch-v2.sql` | 2 | Caller-aware mitigation walk; 100% TP rate |
| `copy_from_user-no-bounds.sql` | 9 | Heuristic; mostly netfilter ioctls |
| `spin-lock-no-unlock.sql` | 2 | Likely intentional acquire-and-return-locked patterns; 0% TP rate |
| `rcu-read-lock-no-unlock.sql` | 0 | Substrate clean for this class |
| `kmalloc-no-free-locally.sql` | 43 | Most are functions handing the alloc to a caller; predicate needs ownership-flow refinement |

## Borrowed-pages-as-scratch v1 results and triage

| Function | Path | Classification | Reason |
|---|---|---|---|
| `esp_output_tail` | `net/ipv4/esp4.c` | False positive | Parent `esp_output_head` calls `skb_cow_data` (line 480) before this function runs; trust boundary already crossed upstream |
| `esp6_output_tail` | `net/ipv6/esp6.c` | False positive | Same upstream pattern in v6 |
| `rxkad_encrypt_response` | `net/rxrpc/rxkad.c` | False positive | The `response` skb is kernel-constructed (outgoing handshake) via `alloc_skb_with_frags`; trust boundary does not apply |
| `rxkad_verify_packet_1` | `net/rxrpc/rxkad.c` | True positive | CVE-2026-43500, named in V4bel's public disclosure |
| `rxkad_verify_packet_2` | `net/rxrpc/rxkad.c` | True positive | Substrate-discovered same-class sibling. Identical in-place pattern (`skcipher_request_set_crypt(req, sg, sg, sp->len, iv.x)`), explicit `/* Decrypt the skbuff in-place. TODO: ... */` comment from author, dispatched from the same parent (`rxkad_verify_packet`) as `_1`. Covered by the same V4bel patch at the upstream gate. Not named in the public disclosure. |

## Borrowed-pages-as-scratch v2 results

The v2 predicate adds a recursive descendant walk from each candidate's
direct caller, looking for any of the known unshare primitives or
kernel-skb allocators. Bounded depth = 5.

| Function | Path | Classification |
|---|---|---|
| `rxkad_verify_packet_1` | `net/rxrpc/rxkad.c` | True positive (CVE-2026-43500) |
| `rxkad_verify_packet_2` | `net/rxrpc/rxkad.c` | True positive (substrate-discovered sibling) |

Both `esp_output_tail` and `esp6_output_tail` are killed because the
parent `esp_output`'s transitive descendants include `esp_output_head
-> skb_cow_data`. `rxkad_encrypt_response` is killed because the
parent's transitive descendants include `alloc_skb_with_frags`.

The two true positives are preserved because their direct caller
(`rxkad_verify_packet`) does not call any unshare primitive, and the
parent walk dead-ends there: the upstream `rxrpc_input_call_event`
gate (where V4bel's patch lives) is reached only via a function-pointer
dispatch through the rxkad security-ops table, which is not a
call_edge in the lifted substrate.

**v1 to v2 receipt:** 5 candidates → 2 candidates, false-positive
count 3 → 0, true-positive rate 40% → 100%.

## Empirical observation that grounds PPP Section 8

V4bel's submitted patch widens `if (skb_cloned(skb))` to
`if (skb_cloned(skb) || skb->data_len)` in `net/rxrpc/call_event.c`
(line 337) and `net/rxrpc/conn_event.c` (line 248).

Applied locally to the kernel checkout:

```diff
-                            skb_cloned(skb)) {
+                            (skb_cloned(skb) || skb->data_len)) {
```

Re-lifting `net/rxrpc/call_event.c` produces an **identical 58-edge
call set** for `rxrpc_input_call_event`, pre-patch and post-patch.
V4bel's patch changes a gate condition, not a call edge. The v1 and v2
predicates over `call_edges` therefore observe `closure.shape =
unchanged` for V4bel's patch.

This is the load-bearing observation in PPP Section 8: **the
predicate's substrate binding determines what patch shapes can witness
closure under it**. To witness V4bel's specific patch via a PPP
receipt, the substrate must extend to expose gate conditions, and a
new predicate must bind to that schema. The current substrate's
inability to witness the patch is the precise feature request for the
substrate's next iteration.

## Cross-language federation aside

Spec Section 7 names cross-language federation as a property of v1
predicates that bind only to schema-v1 relations. The borrowed-pages-
as-scratch class is structurally identical in:

- C: `aead_request_set_crypt(req, sg, sg, ...)` over `skb_to_sgvec`-
  derived SGL.
- Java: `cipher.doFinal(buf, 0, len, buf, 0)` (BouncyCastle pattern)
  over a `ByteBuffer.wrap(externalArray)`-derived buffer.
- Go: `cipher.Stream.XORKeyStream(dst, src)` where `dst` and `src`
  alias the same backing slice from `bytes.NewReader(externalSlice)`.

Federation requires a per-language callee-mapping memento (PPP Section
7); none is published yet. This experiment does not exercise
federation.

## Receipts that this run does NOT yet produce

The PPP pipeline names six artifacts; this run produces the first
three exploratorily. Items 4-6 require the proper reference impl named
in PPP Appendix B.

| PPP artifact | Status in this run |
|---|---|
| Pattern (editorial) | Exists at https://nefariousplan.com/patterns/borrowed-pages-as-scratch |
| Predicate (mechanical) | `predicates/*.sql` |
| Query application | Printed to stdout; not yet a signed memento |
| Closure witness | Required two query applications over the same predicate; pre/post-patch comparison done by hand for the V4bel-RxRPC case |
| FRP receipt | Not produced (would cite the closure witness) |
| Proofchain head update | Not produced |

## Files

- Substrate: `/tmp/substrate-barrage/substrate.db` (transient, not
  committed; rebuildable from the kernel checkout via the steps in
  `tools/lift_kernel.py`).
- Predicates: `predicates/*.sql` (committed).
- Run script: `tools/run_predicates.py` (committed).
- Lift script: `tools/lift_kernel.py` (committed).
