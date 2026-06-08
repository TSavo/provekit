# recognize-demo-python - Python recognize-verb consumer

This package is the Python analog of `examples/voltron-demo/`: a small
M x N consumer that proves idiomatic user code can be recognized against
published shim sugar templates without authoring carrier comments.

- M = 3 module files: `ingest.py`, `persist.py`, and `report.py`, plus a
  thin package spine in `__init__.py` and a runnable `__main__.py`.
- N = 2 vendors: `requests` and stdlib `sqlite3`.
- 5 recognized boundary call sites: two HTTP request wrappers in
  `ingest.py`, and three SQL wrappers in `persist.py`.
- `report.py` is pure user code crossing the vendor seam: it reads a SQL
  row, decodes the JSON payload column, and returns the final report.

The recognized user functions are alpha-equivalent to the shim sugar
bodies after the Python Phase A AST template canonicalizer:

- `fetch_event_response` matches `sugar-shim-python-requests.request`.
- `fetch_event_status` matches `sugar-shim-python-requests.get_status`.
- `connect_db` matches `sugar-shim-python-sqlite3.open_db`.
- `execute_sql` matches `sugar-shim-python-sqlite3.execute`.
- `query_one` matches `sugar-shim-python-sqlite3.query_row`.

## Green path

```
$ PYTHONPATH=examples/recognize-demo-python/src python3 -m pytest examples/recognize-demo-python/tests -q
.....                                                                    [100%]
5 passed in 0.06s
```

The tests are hermetic: they install a fake `requests` module through
`monkeypatch`, then run the full HTTP JSON to SQLite to SQL row to JSON
report chain against a temporary SQLite database.

## Recognizer pilot

The CLI default recognize surface is still `rust-bind`, so the Python
demo invokes the Python bind surface explicitly.

```
$ PATH=/Users/tsavo/sugar-demo-py/implementations/rust/target/release:$PATH sugar recognize \
    --surface python-bind \
    --target python \
    --project /Users/tsavo/sugar-demo-py/examples/recognize-demo-python \
    --source src/recognize_demo_python/ingest.py \
    --source src/recognize_demo_python/persist.py \
    --source src/recognize_demo_python/report.py \
    --binding /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-requests/blake3-512:5c59c1475312c3b1afb94c225fd70479da5f439eecf20aae4fb4401989bc17b7f7d895119c9ea8720865e50110cbe8742bde32510ff425d15fe75bd83bd8c9c8.proof \
    --binding /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-sqlite3/blake3-512:619a91986dbd98e8a3a3c12fc4a43c20f529d77a3973f1a5f6e7a92295b0b408070c140af8402c8734a61262c48fc7651fbda822d61c0e709061c3037df0f975.proof

dispatch: surface=`python-bind` bindings=55 sources=3
recognize: 5 tag(s) emitted
  [0] concept:http-request @ src/recognize_demo_python/ingest.py:8 (fn=fetch_event_response, exact)
  [1] concept:http-request @ src/recognize_demo_python/ingest.py:19 (fn=fetch_event_status, exact)
  [2] concept:sql-connection-open @ src/recognize_demo_python/persist.py:10 (fn=connect_db, exact)
  [3] concept:sql-execute @ src/recognize_demo_python/persist.py:14 (fn=execute_sql, exact)
  [4] concept:sql-query-row @ src/recognize_demo_python/persist.py:18 (fn=query_one, exact)
```

Five tags from idiomatic Python user functions, derived purely from the
shims' published sugar templates. The user parameter names differ from
the shim names (`endpoint` vs. `url`, `statement` vs. `sql`, `values`
vs. `params`), and recognize still matches because the canonicalizer
collapses parameter-name alpha-renaming into indexed param references.

## Recognize write

`--write` mints the bridge and implication contract mementos into the
demo's proof pool:

```
$ PATH=/Users/tsavo/sugar-demo-py/implementations/rust/target/release:$PATH sugar recognize \
    --write \
    --surface python-bind \
    --target python \
    --project /Users/tsavo/sugar-demo-py/examples/recognize-demo-python \
    --source src/recognize_demo_python/ingest.py \
    --source src/recognize_demo_python/persist.py \
    --source src/recognize_demo_python/report.py \
    --binding /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-requests/blake3-512:5c59c1475312c3b1afb94c225fd70479da5f439eecf20aae4fb4401989bc17b7f7d895119c9ea8720865e50110cbe8742bde32510ff425d15fe75bd83bd8c9c8.proof \
    --binding /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-sqlite3/blake3-512:619a91986dbd98e8a3a3c12fc4a43c20f529d77a3973f1a5f6e7a92295b0b408070c140af8402c8734a61262c48fc7651fbda822d61c0e709061c3037df0f975.proof

dispatch: surface=`python-bind` bindings=55 sources=3
recognize: 5 tag(s) emitted
  [0] concept:http-request @ src/recognize_demo_python/ingest.py:8 (fn=fetch_event_response, exact)
  [1] concept:http-request @ src/recognize_demo_python/ingest.py:19 (fn=fetch_event_status, exact)
  [2] concept:sql-connection-open @ src/recognize_demo_python/persist.py:10 (fn=connect_db, exact)
  [3] concept:sql-execute @ src/recognize_demo_python/persist.py:14 (fn=execute_sql, exact)
  [4] concept:sql-query-row @ src/recognize_demo_python/persist.py:18 (fn=query_one, exact)
write: minted 5 bridge(s) + 5 implication contract(s) into /Users/tsavo/sugar-demo-py/examples/recognize-demo-python/.sugar/recognize/blake3-512:85db159d9d760bf4552886752829af8875a7203e3f630cdf29c80044bb3f14dc0e8d8bb3f3082c4aeb10bc5d99f2190b84a69e93c9882ff414737553680c49ae.proof
```

## Prove

With the demo proof pool plus both shim proof directories loaded, the
recognize-emitted call sites enumerate and discharge:

```
$ PATH=/Users/tsavo/sugar-demo-py/implementations/rust/target/release:$PATH sugar prove /Users/tsavo/sugar-demo-py/examples/recognize-demo-python \
    --with /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-requests \
    --with /Users/tsavo/sugar-demo-py/examples/sugar-shim-python-sqlite3

dependency proof resolver ["sugar-bind-lift-python", "--rpc"] does not implement sugar.plugin.resolve_dependency_proofs
warning: bridge query_one has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge execute_sql has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge fetch_event_response has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge connect_db has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge fetch_event_status has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
Sugar verifier report
  total callsites : 5
  discharged      : 5
  violations      : 0
  load errors     : 0

  [discharged] query_one  (python -> sqlite3)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] fetch_event_response  (python -> requests)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] execute_sql  (python -> sqlite3)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] fetch_event_status  (python -> requests)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] connect_db  (python -> sqlite3)
      reason: vacuous: no precondition on target (publisher post-only)
```

## Gap exposed

The older `.proof` envelopes already present in the Python shim example
directories decoded successfully, but recognize loaded zero bindings
from them because their sugar entries predate `body_source.ast_template`
and `body_source.template_cid`. This demo therefore consumes freshly
minted, content-addressed proof envelopes from the same shim source.
The older envelopes remain in place for compatibility; the command
above cites the fresh proof paths explicitly.
