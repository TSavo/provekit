# libsugar self-bind inventory

This directory records the first `sugar bind` run over libsugar's own Rust source.
Target crate: `libsugar`.
Source root: `implementations/rust/libsugar/src/`.
Bind run CID: `blake3-512:f40b65364b1541a894dd66a19b295413fd3a7299461eb84806de812e5d286c5c4de37bfd02b5430329a8a25c98f17e025b5b4dcf98c7cc68112455bd65e3b7f9`.

The shipped command completed after the existing Rust lift RPC binary was built.
The compiled CLI is sandbox-blocked from writing directly into this worktree.
Artifacts were therefore emitted to `/private/tmp/pk971-bind-raw/` and copied here unchanged.
No `cmd_bind.rs` logic was modified.
No new memento type or artifact family was added.

Raw artifacts are under `raw/`.
The raw tree contains `index.json` and `gaps.json`.
It also contains the existing bind families:
`evidence/`, `contracts/`, `policies/`, `promotion-decisions/`, and `sites/`.
The monitor-mode families are present but empty:
`realization-plans/`, `observation-wrappers/`, and `wrapper-fcms/`.

The bind index reports 230 bindings.
Discharge split: 4 exact, 196 loudly bounded lossy, 30 refused.
The index reports 45 clusters.
Anonymous clusters: 43.
Catalog-matched clusters: 2.
Gaps: 0.

Persisted raw artifact counts:
Evidence mementos: 434.
Compound contracts: 200.
Promotion decisions: 434.
Unique site mementos: 200.
Realization plans: 0.
Gap records: 0.

The top 5 clusters by index cardinality are:
1. `UNNAMED-CONCEPT-d`, 67 sites, CID `blake3-512:8612ec7c4a4b3a2620a67a71f3714aaa0c408aaa967aab475a67d8e0fdc6ce64f1cab2362d7fd885972aee6d3f132aa1c50b4ebab176888089c2f93585e84fb8`.
2. `UNNAMED-CONCEPT-6`, 35 sites, CID `blake3-512:fddc7761077c69f310f6158fdcdd405c6173c3ced3965a240593c28ed971604822736bef31edf382b66956949b080d79e7ae66ff011269779c60ccd981ea6d51`.
3. `concept:guard-then-commit`, 29 sites, CID `blake3-512:81c00cc7e33c2cb7da06842d69083e269b860f95bf83c1bd3701d217eed6a27cb166eb8d7a8c48fafbfb741bdebaad1254404ca490c5f3156875649ef7b03508`.
4. `concept:retry-with-bounded-attempts`, 21 sites, CID `blake3-512:eb5c3bf0c572ced28c7ecba119154e170eeadc695743bf9a2ca6575162a5e88ebaab955e10ecf5e981cdd7d9bebcc1d2b8ed410a46aebd0018c8699d7ed302cb`.
5. `UNNAMED-CONCEPT-1`, 13 sites, CID `blake3-512:9e51187b9e1f7b50789875c8ab8a62bd0935c614efdc25810862f353514072750f853e42e9e4b7f95e9889b41fa1eb70e706520943d54fdb9747ab4ff375a526`.

The two catalog matches are visible cross-language hub candidates.
`guard-then-commit` and `retry-with-bounded-attempts` both have nontrivial cardinality.
The two larger anonymous clusters are also hub candidates, but remain unnamed here by design.
Naming those anonymous clusters is out of scope for this issue.

One raw-output wrinkle is worth preserving.
The index counts bindings, while `sites/` is CID-addressed and stores unique site mementos.
That is why the index reports 230 bindings but the raw site family contains 200 files.
The receipt records both views where relevant.
