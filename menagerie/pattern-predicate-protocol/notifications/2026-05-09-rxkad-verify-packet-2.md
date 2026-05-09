# Coordinated Notification: rxkad_verify_packet_2 → Hyunwoo Kim (V4bel)

## Receipt-chain reference

This notification is the next step after the
[`2026-05-09-linux-kernel-net.md`](../experiments/2026-05-09-linux-kernel-net.md)
experimental record. The cluster predicate flagged
`rxkad_verify_packet_2` as a substrate-discovered same-class sibling
of CVE-2026-43500. Per the responsible-disclosure flow the original
researcher gets the heads-up before any public writeup.

## Notification artifact

| Field | Value |
|---|---|
| Date | 2026-05-09 |
| Channel | direct email |
| Recipient | imv4bel@gmail.com (Hyunwoo Kim, V4bel) |
| Subject | `rxkad_verify_packet_2 — same-class sibling of CVE-2026-43500` |
| Signing key | Kevlar `<evilgenius@nefariousplan.com>`, fp `5FD2 1B4F E7E4 A3CA 7971 CB09 DE66 3978 8E09 1026` |
| Signing algorithm | Ed25519 GPG clearsign over UTF-8 body (PGP/INLINE) |
| Body BLAKE2b-512 | `d76af715e78dfe03e93e4f0fe78f15abfb1d2bbba7a27efdf58b61046f9279f0071fa1e0a1762664040bd0e79db54ea0187b6dba48353d1a775e20c5dde19301` |
| Body size | 4,180 bytes including PGP envelope |
| Publication hold | 48 hours from notification, extendable on request |

The signed body itself is correspondence and is NOT committed to this
public repository. The hash above lets V4bel (or any future auditor)
verify against the bytes in his received email.

## Substantive claim communicated

`rxkad_verify_packet_2` (`net/rxrpc/rxkad.c` L494) has the same
`skb_to_sgvec` → in-place `skcipher_request_set_crypt(req, sg, sg, ...)`
→ `crypto_skcipher_decrypt(req)` pattern as `rxkad_verify_packet_1`,
named in V4bel's CVE-2026-43500 disclosure. Both lack local
`skb_cow_data` / `skb_unshare` / `skb_make_writable`. Dispatch is
through `rxkad_verify_packet`'s switch on
`call->conn->security_level`: AUTH → `_1`, ENCRYPT → `_2`. So `_2`
is reachable on level-2 rxrpc connections, a different access vector
from `_1`'s level-1. Per-fire write primitive in `_2` is `sp->len`
(rounded down to 8) rather than the fixed 8 bytes in `_1`.

V4bel's submitted upstream gate-widening patch in
`rxrpc_input_call_event` (`if (skb_cloned(skb))` →
`if (skb_cloned(skb) || skb->data_len)`) fires before the dispatcher
and covers both paths. Patch is sufficient; the disclosure record is
the only thing that doesn't yet enumerate `_2`.

## Decision points returned to V4bel

1. Was `_2` already on his radar, omitted from the public writeup
   intentionally?
2. If not, fold into CVE-2026-43500's description, or file
   separately for tracking?
3. Adjust the 48-hour publication hold if he'd like more time.

## Provenance of the discovery

The substrate that surfaced `_2` was lifted via ProvekIt's C lifter
(`implementations/c/provekit-lift-c-kernel-doc`) over Linux 7.1.0-rc2
`net/` subtree. The lifter-side fixes that made
`aead_request_set_crypt` and similar recovery-wrapped calls visible
were merged earlier the same day as PRs #507 and #510. The predicate
that flagged the candidates is
[`borrowed-pages-as-scratch-v2.sql`](../predicates/borrowed-pages-as-scratch-v2.sql)
in this destination. The lift-and-query workflow is captured in
[`tools/lift_kernel.py`](../tools/lift_kernel.py) and
[`tools/run_predicates.py`](../tools/run_predicates.py).

## Receipt-chain summary

```text
Pattern (editorial, nefariousplan)
  -> Predicate (this destination, predicates/borrowed-pages-as-scratch-v2.sql)
  -> Substrate (linux@7.1.0-rc2 net/ lift via ProvekIt)
  -> Result set (2 candidates, both true positives)
  -> Triage (one TP is the disclosed CVE-2026-43500; the other is
     substrate-discovered)
  -> Notification (this artifact)
  -> Public writeup (TBD, post-acknowledgment, post-hold)
  -> FRP receipt (when the spec's reference implementation lands)
```

This destination is the receipt; the notification is the
coordination step; the public writeup is the editorial close;
the FRP receipt is the formal close once tooling exists.
