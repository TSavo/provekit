# Experimental Record: 2026-05-09: Linux kernel wide substrate

## Context

Second-pass receipt over a substantially wider kernel surface. Tests
predicate stability and discovers whether new candidates surface
outside the original `net/` scope.

## Substrate

| Item | Value |
|---|---|
| Source tree | `linux@7.1.0-rc2` |
| Subtrees lifted | `net/ipv4`, `net/ipv6`, `net/rxrpc`, `net/core`, `net/xfrm`, `net/mac80211`, `net/wireless`, `net/bluetooth`, `crypto`, `security`, `drivers/net` |
| Files lifted | 4,332 `.c` files |
| Call edges produced | 621,100 |
| Lift wall time | 1340 seconds (~22 minutes) |
| Substrate size | 405 MB |

Substrate is 12.5× larger than the net/-only run.

## Predicate results

| Predicate | net/ count | wide count | New candidates |
|---|---:|---:|---|
| `borrowed-pages-as-scratch.sql` (v1) | 5 | 5 | 0 |
| `borrowed-pages-as-scratch-v2.sql` | 2 | 2 | 0 |
| `copy_from_user-no-bounds.sql` | 9 | 28 | +19 (drivers/net PPP, TUN, debugfs; security/apparmor; security/keys) |
| `spin-lock-no-unlock.sql` | 2 | 5 | +3 (drivers/net/ethernet: benet, ks8851, cassini) |
| `rcu-read-lock-no-unlock.sql` | 0 | 1 | +1 (security/smack: smk_seq_start, likely an FP: seq_file lock-start / unlock-stop pattern) |
| `kmalloc-no-free-locally.sql` | 43 | 459 | +416 (crypto/, security/, drivers/net/) |

## Observation: borrowed-pages-as-scratch is concentrated

Both v1 and v2 of the borrowed-pages predicate produce identical match
sets on the wide substrate as on the net/-only substrate. Crypto/,
security/, drivers/net/wireless, drivers/net/ethernet do not contain
in-place-AEAD-on-skb-frag receivers that hit the predicate.

This is a **structural finding about the kernel**, surfaced by the
substrate query:

> The in-place-AEAD-on-skb-frag pattern is concentrated in `net/rxrpc`.
> Outside that subsystem, the kernel does not have receivers shaped
> like `rxkad_verify_packet_*` in the lifted surface.

Translated to the editorial framing: the borrowed-pages-as-scratch
class, as currently named in nefariousplan's catalog and compiled by
the v2 predicate over schema-v1, has only the four publicly disclosed
receiver instances in the contemporary Linux kernel net stack (the two
ESP variants which are mitigated by upstream `skb_cow_data`, plus the
two RxRPC variants which are the V4bel disclosure target plus its
substrate-discovered sibling).

The kernel does not have a long tail of borrowed-pages instances
hiding in unaudited subsystems. That is itself a useful claim,
mechanically established.

## copy_from_user-no-bounds widening

The userspace-bounds heuristic predicate widens substantially (9 to
28). Categories of new candidates:

- **PPP I/O paths** (`drivers/net/ppp/ppp_*.c`): `ppp_async_ioctl`,
  `ppp_ioctl`, `ppp_set_compress`, `ppp_write`, `ppp_sync_ioctl`. PPP's
  driver layer takes ioctls and direct writes; the predicate flags
  them because they call `copy_from_user` without one of the explicit
  bounds-checking helpers in the predicate's named set. These warrant
  manual triage; the heuristic is coarse.
- **Wireless debugfs writers**: `b43_debugfs_write`,
  `b43legacy_debugfs_write`, `carl9170_debugfs_write`. Similar shape;
  debugfs takes user input by length.
- **WWAN core**: `wwan_port_fops_at_ioctl`, `wwan_port_fops_write`.
- **Security module userspace interfaces**: AppArmor's
  `aa_simple_write_to_buffer` and `multi_transaction_new`; keys
  subsystem's `key_get_type_from_user`. These are user-input-from-
  /sys interfaces.

This predicate is a heuristic, not a rigorous bug finder; the wider
match set is candidates for editorial follow-up, not signal that any
of these are vulnerabilities.

## spin-lock-no-unlock widening

Three new ethernet driver hits (be_cmds, ks8851_par, cassini) all
follow the pattern `*_lock_*` paired with `*_unlock_*` in a sibling
function (e.g. `cas_lock_tx` paired with `cas_unlock_tx`). The
function-level predicate cannot see across the pairing; these are
likely intentional acquire-and-return-locked patterns, not bugs.

This is a known limitation of the heuristic. A v2 of the lock-pairing
predicate would walk callers and check for matching unlock invocations;
that work has not been done.

## rcu-read-lock-no-unlock new finding

`smk_seq_start` in `security/smack/smackfs.c` calls `rcu_read_lock`
without a matching `rcu_read_unlock`. Inspection: this is the standard
seq_file iterator pattern where `start()` acquires a lock that `stop()`
releases. The predicate's function-level scope cannot see across
seq_operations callbacks; this is a false positive.

The pattern is identical to the spin-lock-no-unlock case: pairs cross
function boundaries via a vtable. A v2 predicate that follows
seq_operations function-pointer assignments would resolve it; that
work has not been done.

## Summary of receipts produced

The wide-substrate run produces:

1. **Confirmation that v2 borrowed-pages-as-scratch is stable**: same
   match set on a 12.5×-larger substrate. The predicate is not
   hallucinating across the wider surface.
2. **A kernel-scope bound on the class**: the class is concentrated in
   `net/rxrpc`; outside that subsystem, the lifted surface does not
   contain receivers of this shape.
3. **A widened candidate set for follow-up triage** under the heuristic
   predicates (copy_from_user, spin-lock, rcu, kmalloc). None of these
   are claims of vulnerability; all are candidates surfaced by
   structural pattern-matching.

The same caveats from the net/-only run apply: signed query-application
mementos and closure witnesses are not produced. The proper PPP-
canonical receipt machinery is the reference-impl work named in PPP
Appendix B.
