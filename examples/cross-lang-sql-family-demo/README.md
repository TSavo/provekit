# cross-lang-sql-family-demo

End-to-end demonstration of the floating-axes substrate (epic #1355).
A single rust source file with 3 SQL `@boundary` stubs resolves to its
sister-library implementation in 3 other languages — python (sqlite3 /
aiosqlite), typescript (better-sqlite3 / pg), and java (sqlite-jdbc) —
WITHOUT modifying the source. The substrate dispatches by
`(family, concept_name)` rather than by language-specific library name.

## Run the discovery report

```sh
# Build the CLI first.
cd implementations/rust && cargo build -p provekit-cli

# Discovery mode — rust source, python target.
./target/debug/provekit materialize \
  --library sqlite3 \
  --source-lang rust \
  --source-dir ../../examples/cross-lang-sql-family-demo/src \
  --project ../.. \
  --target python

# Same source, different target languages.
./target/debug/provekit materialize --library better-sqlite3 --source-lang rust \
  --source-dir ../../examples/cross-lang-sql-family-demo/src --project ../.. --target typescript

./target/debug/provekit materialize --library sqlite-jdbc --source-lang rust \
  --source-dir ../../examples/cross-lang-sql-family-demo/src --project ../.. --target java
```

## Expected output

```
rust   → python:     2 ambiguous (sql-query / sql-execute → python-sqlite3 + python-aiosqlite)
                     1 resolve   (sql-connection-open → python-sqlite3)
rust   → typescript: 2 ambiguous (sql-query / sql-execute → better-sqlite3 + pg)
                     1 refuse    (sql-connection-open not in either provides_concepts)
rust   → java:       3 resolve   (all three → sqlite-jdbc)
```

The substrate-honest interpretation:

- **RESOLVE** = exactly one target manifest declares this concept under
  the matching family. The dispatch is unambiguous; in part B
  (signature translation + code emission) this site would materialize
  to target-language code.
- **AMBIGUOUS** = multiple target manifests match. Caller must
  disambiguate via `--library`. This is the family-floating-library
  case where the substrate has multiple sister implementations and
  the user picks.
- **REFUSE** = no target manifest declares this concept. The sister
  shim is missing in the target language. Substrate-honest: doesn't
  silently pick a near-match; surfaces the gap.

## How it works

Each `@boundary` declares the four floating axes (language, family,
library, version) plus the concept. The lift binary (walk_rpc) parses
these into per-site carrier comments. Materialize's cross-language
mode walks the carriers, calls `find_target_manifests(project_root,
target_lang, concept_name, family)` to enumerate matching realize
manifests in the catalog, and reports the per-site outcome.

The catalog lookups are powered by `provides_concepts` declarations on
each realize manifest (added in #1364 chunks 1+3) and `family` /
`library_version` pins (added in #1357 / #1359). All declarations
landed on origin/main in the floating-axes epic; this demo is the
proof that the cross-language wire actually works as advertised.

## Status

This is the discovery-mode foundation (#1361 chunk 2 part A). Part B
adds target-language code emission — signature translation (rust
`fn(&i64, &str) -> i64` → python `def(conn, sql) -> int`) plus the
per-target realize binary invocation. The discovery report is
itself useful: it surfaces coverage gaps and ambiguity at the
substrate level, BEFORE any code is generated.
