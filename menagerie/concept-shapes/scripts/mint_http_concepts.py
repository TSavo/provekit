#!/usr/bin/env python3
"""Mint HTTP request and response concept shape entries."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

HTTP_METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]

HTTP_REQUEST_LOSS_DIMS = [
    "sync-vs-async",
    "cancellation",
    "streaming-body",
    "body-encoding",
    "retries",
    "timeout",
    "cookie-jar",
    "redirect-policy",
    "tls-pinning",
]

HTTP_RESPONSE_LOSS_DIMS = [
    "streaming-body",
    "header-case-sensitivity",
    "body-decoding",
]

SPEC_FILENAMES = {
    "url": "url_shape.spec.json",
    "header-map": "header-map_shape.spec.json",
    "byte-stream": "byte-stream_shape.spec.json",
    "http-request": "http-request_shape.spec.json",
    "http-response": "http-response_shape.spec.json",
}


def ctor(name: str) -> dict:
    return {"args": [], "kind": "ctor", "name": name}


def var(name: str) -> dict:
    return {"kind": "var", "name": name}


def const(value: object, sort_name: str) -> dict:
    return {"kind": "const", "sort": {"kind": "primitive", "name": sort_name}, "value": value}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


def atomic(name: str, args: list[dict] | None = None) -> dict:
    return {"args": args or [], "kind": "atomic", "name": name}


def operation_contract(operator: str, arity: list[str], result: str, slots: list[dict], wp_note: str) -> dict:
    return {
        "arity": arity,
        "arity_shape": {"kind": "named", "slots": slots},
        "kind": "operation-contract",
        "operator": operator,
        "result": result,
        "wp_note": wp_note,
    }


def shape_spec(
    fn_name: str,
    formals: list[str],
    formal_sorts: list[str],
    return_sort: str,
    pre: dict,
    post: dict,
    effects: dict | None = None,
    loss_dimensions: list[str] | None = None,
) -> dict:
    spec: dict = {
        "effects": effects or {"effects": []},
        "fn_name": fn_name,
        "formal_sorts": [ctor(sort) for sort in formal_sorts],
        "formals": formals,
        "kind": "algorithm",
        "post": post,
        "pre": pre,
        "return_sort": ctor(return_sort),
    }
    if loss_dimensions is not None:
        # `loss_dimensions` is catalog metadata: the named axes along which
        # different realizations of this concept may diverge. The concrete
        # per-realization loss values live on RealizationDesugaringMemento
        # / LossyMorphismMemento (Rust type LossRecord at
        # provekit-ir-types/src/lib.rs L508). The concept shape carries
        # only the dimension names, sorted for byte stability.
        spec["loss_dimensions"] = sorted(loss_dimensions)
    return spec


def build_shape_specs() -> dict[str, dict]:
    url_post = operation_contract(
        "url",
        ["Scheme", "Authority", "Path", "Optional<Query>", "Optional<Fragment>"],
        "Url",
        [
            {"name": "scheme"},
            {"name": "authority"},
            {"name": "path"},
            {"name": "query"},
            {"name": "fragment"},
        ],
        "URL components are carried as parsed fields; normalization policy is left to the binding.",
    )

    header_map_post = operation_contract(
        "header-map",
        ["ListOfHeaderEntry"],
        "HeaderMap",
        [{"name": "entries", "shape": {"kind": "set"}}],
        "A header map is a multimap from field name to field values; duplicate field names are preserved.",
    )

    byte_stream_post = operation_contract(
        "byte-stream",
        ["ListOfBytes", "Optional<Int>"],
        "ByteStream",
        [{"name": "chunks"}, {"name": "length_hint"}],
        "A byte stream is an ordered sequence of byte chunks with an optional total length hint.",
    )

    request_pre = atomic(
        "in",
        [
            var("method"),
            {"args": [const(method, "HttpMethod") for method in HTTP_METHODS], "kind": "ctor", "name": "http_method_set"},
        ],
    )
    request_post = operation_contract(
        "http-request",
        ["HttpMethod", "Url", "HeaderMap", "Optional<ByteStreamOrBytes>"],
        "HttpResponse",
        [
            {"name": "method"},
            {"name": "url"},
            {"name": "headers"},
            {"name": "body"},
        ],
        (
            "Performs the HTTP request and returns concept:http-response. May raise an http error or refuse along any documented loss dimension. "
            "Library callsites like libcurl perform, Java HttpClient send, Python urllib.request.urlopen, JS fetch, Python requests.get, and Rust reqwest::get all bind to this operation and produce response data. "
            "Loss dimensions (catalog metadata): "
            + ", ".join(sorted(HTTP_REQUEST_LOSS_DIMS))
            + ". Per-realization values for these dimensions live on the LossyMorphismMemento / RealizationDesugaringMemento for each (concept, language, library) cell, not on this shape."
        ),
    )

    response_pre = atomic("http_status_code", [var("status")])
    response_post = operation_contract(
        "http-response",
        ["HttpStatus", "HeaderMap", "ByteStreamOrBytes"],
        "HttpResponse",
        [
            {"name": "status"},
            {"name": "headers"},
            {"name": "body"},
        ],
        (
            "Constructs an HTTP response from status, concept:header-map, and body bytes or a concept:byte-stream. "
            "Loss dimensions (catalog metadata): "
            + ", ".join(sorted(HTTP_RESPONSE_LOSS_DIMS))
            + ". Per-realization values live on the morphism mementos for each (concept, language, library) cell, not on this shape."
        ),
    )

    return {
        "url": shape_spec(
            "concept:url",
            ["scheme", "authority", "path", "query", "fragment"],
            ["Scheme", "Authority", "Path", "Optional<Query>", "Optional<Fragment>"],
            "Url",
            true_formula(),
            url_post,
        ),
        "header-map": shape_spec(
            "concept:header-map",
            ["entries"],
            ["ListOfHeaderEntry"],
            "HeaderMap",
            true_formula(),
            header_map_post,
        ),
        "byte-stream": shape_spec(
            "concept:byte-stream",
            ["chunks", "length_hint"],
            ["ListOfBytes", "Optional<Int>"],
            "ByteStream",
            true_formula(),
            byte_stream_post,
        ),
        "http-request": shape_spec(
            "concept:http-request",
            ["method", "url", "headers", "body"],
            ["HttpMethod", "Url", "HeaderMap", "Optional<ByteStreamOrBytes>"],
            "HttpResponse",
            request_pre,
            request_post,
            {"effects": [{"kind": "effect-signature", "name": "NetworkRequest"}]},
            HTTP_REQUEST_LOSS_DIMS,
        ),
        "http-response": shape_spec(
            "concept:http-response",
            ["status", "headers", "body"],
            ["HttpStatus", "HeaderMap", "ByteStreamOrBytes"],
            "HttpResponse",
            response_pre,
            response_post,
            {"effects": [{"kind": "effect-signature", "name": "NetworkResponse"}]},
            HTTP_RESPONSE_LOSS_DIMS,
        ),
    }


def build_examples_md() -> str:
    return """# HTTP Concept Shape Examples

These examples are illustrative bindings for Bridge C. They do not mint sugar dictionaries.

| Language surface | Request binding | Response binding |
| --- | --- | --- |
| C libcurl | `CURLOPT_CUSTOMREQUEST` or default method maps to `method`; `CURLOPT_URL` maps to `url`; `curl_slist` request headers map to `headers`; upload callbacks or POST fields map to `body`. | `CURLINFO_RESPONSE_CODE` maps to `status`; received header callbacks map to `headers`; write callbacks map to `body`. |
| Java `java.net.http` | `HttpRequest.method`, `HttpRequest.uri`, `HttpRequest.headers`, and `BodyPublisher` bind to the four `concept:http-request` slots. | `HttpResponse.statusCode`, `HttpResponse.headers`, and `BodyHandler` output bind to the three `concept:http-response` slots. |
| Python `urllib.request` | `urllib.request.Request.get_method()`, `full_url`, `header_items()`, and `data` bind to the four `concept:http-request` slots. | `HTTPResponse.status`, `headers`, and `read()` or streaming reads bind to the three `concept:http-response` slots. |

The same `concept:http-request` CID is the shared input for these request surfaces. The same `concept:http-response` CID is the shared input for these response surfaces.
"""


def append_cids(rows: list[dict]) -> None:
    existing = CID_FILE.read_text(encoding="utf-8").splitlines() if CID_FILE.exists() else ["kind\tname\tcid\tpath"]
    seen: dict[tuple[str, str], str] = {}
    for line in existing[1:]:
        parts = line.split("\t")
        if len(parts) >= 3:
            seen[(parts[0], parts[1])] = parts[2]

    for row in rows:
        key = (row["kind"], row["name"])
        if key in seen:
            if seen[key] != row["cid"]:
                raise SystemExit(
                    f"one-name-one-CID violation: {row['kind']} {row['name']} "
                    f"already registered as {seen[key]!r} but new mint produced {row['cid']!r}"
                )
            continue
        existing.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
        seen[key] = row["cid"]
    CID_FILE.write_text("\n".join(existing) + "\n", encoding="utf-8")


def update_readme(cid_rows: list[dict]) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")
    start = "## HTTP Concept Shapes\n"
    if start in text:
        text = text[: text.index(start)].rstrip() + "\n"

    cid_by_name = {row["name"]: row["cid"] for row in cid_rows}
    section = [
        "## HTTP Concept Shapes",
        "",
        "Bridge B mints the HTTP request and response concept shapes used by later HTTP sugar and trinity payload work.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:http-request` | `{cid_by_name['concept:http-request']}` | method, URL, headers, optional stream or bytes body |",
        f"| `concept:http-response` | `{cid_by_name['concept:http-response']}` | status, headers, stream or bytes body |",
        f"| `concept:url` | `{cid_by_name['concept:url']}` | parsed URL component carrier |",
        f"| `concept:header-map` | `{cid_by_name['concept:header-map']}` | duplicate-preserving header multimap |",
        f"| `concept:byte-stream` | `{cid_by_name['concept:byte-stream']}` | ordered byte chunks with optional length hint |",
        "",
        "Examples: `examples.md` in this directory shows high-level bindings for libcurl, `java.net.http`, and `urllib.request`.",
    ]
    readme.write_text(text.rstrip() + "\n\n" + "\n".join(section) + "\n", encoding="utf-8")


def mint_all() -> list[dict]:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for slug, spec in build_shape_specs().items():
        spec_name = SPEC_FILENAMES[slug]
        discharge.write_json(SPEC_DIR / spec_name, spec)
        cid, path = discharge.mint("algorithm", spec_name)
        rows.append({"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path})

    append_cids(rows)
    (BASE / "examples.md").write_text(build_examples_md(), encoding="utf-8")
    update_readme(rows)
    discharge.scan_created_text()
    for row in rows:
        print(f"http_shape_cid\t{row['name']}\t{row['cid']}")
    return rows


if __name__ == "__main__":
    mint_all()
