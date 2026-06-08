# Sparse-annotation-as-predicate: Linux kernel angles via the C lifter

Status: research note, 2026-05-09. Author: triage agent. Surface: `c-sparse`.

## What the lifter emits today

`implementations/c/sugar-lift-c-sparse/src/sparse.c` ingests `pk_c_source_facts` and
walks `facts->sparse_annotations[]`. Each annotation has `(name, argument_text, locus)`.
`emit_sparse_contracts()` translates five names into v1 atomic contracts of shape
`{kind:"contract", name, outBinding:"out", post:{kind:"atomic", name, args:[{var}]}}`:

- `__user`        -> `c-sparse.user-pointer` (var "ptr")
- `__rcu`         -> `c-sparse.rcu-pointer` (var "ptr")
- `__must_hold`   -> `c-sparse.must-hold` (var = `argument_text` or "lock")
- `__acquires`    -> `c-sparse.acquires` (var = `argument_text` or "lock")
- `__releases`    -> `c-sparse.releases` (var = `argument_text` or "lock")

Not yet emitted: `__iomem`, `__percpu`, `__force`, `__bitwise`, `__safe`. No callEdges, no
member-access nodes, no dereference sites. Integration test asserts `"callEdges":[]`. The
contract is a *type-level marker on a parameter named "ptr"*: it does not yet bind to a
specific function parameter, struct field, or expression in the IR. That gap is the work.

Kernel header sample confirms scale: in `include/linux/fs.h` and `sched.h` alone, dozens
of `__user`-typed parameters (`vfs_read`, `vfs_write`, `do_sys_open`, every `f_op->read`),
many `__rcu` fields (`task_struct.real_parent`, `inode.i_fsnotify_marks`, `cred`s on
`task_struct`), a sprinkle of `__percpu` (`task_struct.trc_reader_scp`), and `__force`
across the entire `fmode_t` and `RWF_*` table.

## Predicate ideas

### 1. Direct `__user` deref ("the syzkaller starter")

Source kind: `c-sparse.user-pointer` annotation on a function parameter or struct field,
contract scoped to a function CID. Predicate shape: `SELECT f.cid FROM functions f JOIN
sparse_annotations a ON a.function_cid=f.cid AND a.name='__user' JOIN deref_sites d ON
d.function_cid=f.cid AND d.target_var=a.parameter_name WHERE d.via NOT IN
('copy_from_user','copy_to_user','get_user','put_user','copy_from_user_nofault',
'strncpy_from_user','memdup_user','iter_iov','copy_struct_from_user') AND
NOT EXISTS (SELECT 1 FROM force_casts c WHERE c.expr_cid=d.expr_cid)`. Required
substrate extensions: a `parameter_name` field on the annotation fact (today it's
hardcoded "ptr"), a `deref_sites` table (member access, unary `*`, array-subscript,
`memcpy`/`memmove` first/second-arg call), and a callsite table indexing the safe-API
allowlist. Bug class: arbitrary kernel read/write triggered from userspace (KASAN
out-of-bounds, info leak, the entire `__copy_from_user`-was-missing genre: CVE-2017-5123
`waitid`, CVE-2022-32250 `nf_tables`). Effort: roughly two engineering days for the
substrate extensions (parameter binding + deref + callsite), one day for the predicate
and its allowlist, half a day for the false-positive triage on a single subsystem
(start with `fs/read_write.c`). Substrate today: insufficient: needs deref-site facts.

### 2. `__rcu` field read without `rcu_dereference` (UAF starter)

Source kind: `c-sparse.rcu-pointer` annotation on a *struct field* (the lifter already
sees these on `task_struct.real_parent`, `inode.i_fsnotify_marks`, etc.). Predicate
shape: `SELECT m.expr_cid FROM member_access m JOIN sparse_annotations a ON
a.struct_name=m.struct_name AND a.field_name=m.field_name AND a.name='__rcu' WHERE
m.read_kind='direct' AND NOT EXISTS (SELECT 1 FROM rcu_safe_calls r WHERE
r.wraps=m.expr_cid AND r.callee IN ('rcu_dereference','rcu_dereference_check',
'rcu_dereference_protected','rcu_dereference_raw','rcu_access_pointer','READ_ONCE'
WHEN inside __must_hold)) AND NOT EXISTS (SELECT 1 FROM force_casts WHERE
expr_cid=m.expr_cid)`. Substrate extensions: annotation must record `(struct_name,
field_name)` not just "ptr"; need a `member_access` fact emitted by the C parser; need
a callsite-wraps-expression relation; honor `__must_hold(rcu)` / `rcu_read_lock()` scope
as exemption. Bug class: use-after-free where the writer races a reader who skipped the
`smp_read_barrier_depends`/`READ_ONCE` (CVE-2017-15265 ALSA seq, CVE-2023-32233
`nft_set`). Effort: three days, dominated by member-access lifting and lexical
RCU-read-side-critical-section detection. Substrate today: insufficient: same
deref-site gap as #1, plus the field-binding gap.

### 3. `__force` casts that erase a `__user` or `__rcu` discipline

Source kind: every `(__force T)x` cast in the existing `pk_c_source_facts`, paired with
the source operand's annotation. Today the lifter does not even emit `__force`; the
fix is to extend `emit_sparse_contracts()` with a `__force` branch that records the
source operand's address-space annotation (if any) and the destination type. Predicate
shape: `SELECT cast.expr_cid, cast.src_annotation, cast.dst_annotation FROM force_casts
cast WHERE cast.src_annotation IN ('__user','__rcu','__iomem','__percpu') AND
cast.dst_annotation = '' AND cast.function_cid NOT IN (SELECT cid FROM
trusted_force_sites)`. Bug class: contract-laundering: the kernel pattern that
suppresses Sparse warnings by casting away an address space, sometimes correctly (uaccess
helpers, RCU-protected publish), sometimes a real bug (`fmode_t` arithmetic that loses
the `__force` re-cast; the genuinely-unsafe `(void *)(unsigned long)user_addr`
pattern). Most hits will be intentional, so the value is the *manifest*: a substrate-wide
ledger of every type-discipline escape, ranked by call-site context. Effort: half a day
to emit the cast facts, half a day for the predicate, one day to land a curated
allow-list of legitimate sites (`uaccess.h`, `compat_ioctl`, `bpf_jit`). Substrate
today: insufficient: `__force` is currently dropped on the floor.

### 4. `__iomem` direct deref (hardware-crash starter)

Source kind: `__iomem` annotation, currently *not emitted*. Add a sixth branch:
`__iomem -> c-sparse.iomem-pointer`. Predicate shape mirrors #1 with a different safe-API
set: `readb/readw/readl/readq`, `writeb/writew/writel/writeq`, `memcpy_fromio`,
`memcpy_toio`, `memset_io`, `ioread*`, `iowrite*`, `__raw_readl`, the `accessor` macros,
and the `__iomem`-aware `ioremap_*` family. Bug class: bus errors / machine-check on
non-x86 (ARM64, POWER) where a direct `*addr` over MMIO triggers an SError; on x86 it
is more often silent but contributes to the read-ordering bug class (CVE-2019-19332 KVM
`vmx_emulate_invvpid`, the `pcie_aspm` MMIO-ordering family). Effort: matches #1 once
deref-site lifting exists; small additional cost is one new annotation branch. Substrate
today: insufficient on two axes: need `__iomem` emission *and* deref sites.

### 5. `__bitwise` cross-type contamination (audit not safety)

Source kind: `__bitwise` typedefs (`gfp_t`, `fmode_t`, `__sum16`, `__be32`,
`pci_channel_state_t`). Sparse uses `__bitwise` to forbid implicit mixing across
distinct flag namespaces; the lifter does not yet recognize it. Predicate shape:
`SELECT b.expr_cid FROM binary_ops b JOIN typedef_facts l ON l.name=b.lhs_type JOIN
typedef_facts r ON r.name=b.rhs_type WHERE l.bitwise=1 AND r.bitwise=1 AND l.name <>
r.name AND b.op IN ('|','&','^','==','!=') AND NOT EXISTS (SELECT 1 FROM force_casts c
WHERE c.expr_cid IN (b.lhs_cid,b.rhs_cid))`. Bug class: silent flag-namespace
collision (e.g., a `__GFP_*` flag accidentally OR'd with a `FMODE_*` flag, byte-order
confusion when a `__be32` is compared with a host-order `__le32`). Far rarer than the
deref classes but uniquely *only findable through Sparse*: no other static analyzer
preserves `__bitwise`. Effort: one day to emit `typedef_facts` with `bitwise=1`, one day
for the predicate, half a day to triage. Substrate today: insufficient: needs a
`typedefs` fact stream and `binary_ops` with operand types.

## Priority recommendation

#1 (`__user` deref) gives the biggest CVE-class blast radius and forces the substrate
extension every other predicate also needs (parameter-binding + deref-site facts). Land
that scaffolding first; #2 and #4 then collapse to one-day predicates each. #3 is
cheap and high-value as a *manifest* even before bug detection. #5 is the lowest
priority but is the canonical demonstration that Sugar preserves discipline no other
tool sees.
