# `sugar materialize`

`sugar materialize` replaces Sugar concept-citation carrier comments in source files with source emitted by a selected realize kit.

The command is an orchestration surface only. It does not contain host-language renderers in the Rust CLI: language and library behavior lives behind `.sugar/realize/<surface>/manifest.toml` JSON-RPC realize plugins and body-template entries that explicitly declare their `(target_language, target_library_tag, concept_name)` tuple.

## Basic usage

Dry-run to stdout:

```bash
sugar materialize \
  --library typescript-better-sqlite3 \
  --source-dir src \
  --project .
```

Rewrite source files in place:

```bash
sugar materialize \
  --library python-requests \
  --source-dir src \
  --project . \
  --write
```

Write materialized copies under a separate tree:

```bash
sugar materialize \
  --library rust-reqwest \
  --source-dir src \
  --project . \
  --out-dir materialized-src
```

`--library` may be language-prefixed (`python-requests`, `rust-reqwest`, `typescript-better-sqlite3`). If it is not, pass `--target <language>` or provide project markers such as `Cargo.toml`, `pyproject.toml`, or `package.json` so the target language can be inferred.

## Carrier comments

A source file carries a JSON concept citation and, optionally, its payload CID:

```python
# sugar-concept: {"artifact_kind":"sugar-concept-citation-comment-sugar","concept_name":"concept:http-request","function":"fetch_status","params":["url"],"param_types":["str"],"return_type":"int"}
# sugar-concept-payload-cid: blake3-512:<jcs-payload-cid>
```

The command accepts line comments (`//`, `#`) and single-line block comments (`/* ... */`). If a payload CID is present, `materialize` recomputes JCS+BLAKE3-512 over the payload and rejects mismatches before dispatching to a realize kit.

## Examples

### Python requests shim

With `.sugar/realize/python-requests/manifest.toml` pointing at the Python requests realize kit:

```bash
sugar materialize --library python-requests --source-dir src --project .
```

A `concept:http-request` citation materializes through the Python requests shim and emits code using `requests.get(...)`. The Rust CLI does not know how to render Python or requests; it only selects the manifest surface and dispatches the request.

### Rust reqwest shim

With `.sugar/realize/rust/manifest.toml` pointing at `sugar-realize-rust`:

```bash
sugar materialize --library rust-reqwest --source-dir src --project .
```

A `concept:http-request` citation materializes through the Rust realizer and the `reqwest` library tag, emitting code using `reqwest::get(...)`.

### TypeScript better-sqlite3 shim

With `.sugar/realize/typescript-better-sqlite3/manifest.toml` pointing at the TypeScript better-sqlite3 realize kit:

```bash
sugar materialize --library typescript-better-sqlite3 --source-dir src --project .
```

A `concept:sql-query` citation materializes through the TypeScript better-sqlite3 shim and emits code using `db.prepare(sql).all(args)`.

## Body-template tuple normalization

Library-specific body templates live at:

```text
menagerie/<language>-language-signature/specs/body-templates/<language>-canonical-bodies-<library-tag>.json
```

Every entry in those files must explicitly declare the sugar tuple it realizes:

```json
{
  "target_library_tag": "requests",
  "concept_name": "concept:http-request"
}
```

The file-level `header.content.target_language` supplies the language component. Together, the normalized tuple is `(target_language, target_library_tag, concept_name)`. The selected `.sugar/realize/<surface>/manifest.toml` must agree with that tuple; the Rust CLI only selects the manifest and dispatches to the realize plugin.

## Scan behavior

`materialize` scans supported source extensions (`.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.rs`, `.java`) and prunes dependency/build directories such as `.git`, `node_modules`, `target`, `dist`, `build`, and `__pycache__`.

Files without concept carriers are left untouched. If no carriers are found, the command exits successfully and reports `found 0 concept citation(s)` on stderr unless `--quiet` is set.
