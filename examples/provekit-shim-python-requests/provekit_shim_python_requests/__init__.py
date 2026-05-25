# SPDX-License-Identifier: Apache-2.0
#
# provekit-shim-python-requests: substrate-honest concept bindings for the
# Python `requests` HTTP library.
#
# Every claim this kit makes is in this file. There are no sidecar files. The
# lift kit reads this source, extracts the structural shape of each annotated
# function body, attaches the per-binding loss declarations directly from the
# annotation arguments, attaches refusal-memento IR for each @refuse annotation,
# and cmd_mint consumes that IR over JSON-RPC to produce a signed .proof
# envelope.
#
# Three speech acts per paper 24:
#   1. @sugar.bind(... loss=[])           materialize
#   2. @sugar.bind(... loss=["<dims>"])   loudly-bounded-lossy
#   3. @refuse(...)                       refuse with reason
#
# HTTP catalog alignment:
#   * concept:http-request has arity
#     (HttpMethod, Url, HeaderMap, Optional<ByteStreamOrBytes>) -> HttpResponse.
#   * concept:http-response has arity
#     (HttpStatus, HeaderMap, ByteStreamOrBytes) -> HttpResponse.
#   * concept:url, concept:header-map, and concept:byte-stream are nearby
#     carriers in the catalog. The requests surface mostly consumes Python
#     primitives for those carriers; carrier surfaces that cannot preserve the
#     catalog shape are refused below instead of minted as silent wrong output.

from typing import Any, Mapping, Optional

import requests

from provekit import refuse, sugar


# =============================================================================
# A. HTTP request operation
# =============================================================================

@sugar.bind(
    concept="concept:http-request",
    library="requests",
    family="concept:family:http",
    version="2",
    loss=[
        "sync-vs-async",
        "body-encoding",
        "cookie-jar",
        "redirect-policy",
        "retries",
        "streaming-body",
        "timeout",
        "tls-pinning",
    ],
)
def request(
    method: str,
    url: str,
    hdrs: Optional[Mapping[str, str]],
    body: Optional[Any],
) -> requests.Response:
    import requests
    kwargs = {"headers": hdrs, "data": body}
    return requests.request(method, url, **kwargs)


@sugar.bind(
    concept="concept:http-request",
    library="requests",
    family="concept:family:http",
    version="2",
    loss=[
        "sync-vs-async",
        "method-fixed-to-get",
        "response-projected-to-status",
        "response-headers",
        "response-body",
        "cookie-jar",
        "redirect-policy",
        "retries",
        "streaming-body",
        "timeout",
        "tls-pinning",
    ],
)
def get_status(url: str) -> int:
    import requests
    response = requests.get(url)
    return response.status_code


@sugar.bind(
    concept="concept:http-request",
    library="requests",
    family="concept:family:http",
    version="2",
    loss=[
        "sync-vs-async",
        "method-fixed-to-post",
        "body-encoding",
        "json-encoding",
        "response-projected-to-status",
        "response-headers",
        "response-body",
        "cookie-jar",
        "redirect-policy",
        "retries",
        "streaming-body",
        "timeout",
        "tls-pinning",
    ],
)
def post_json_status(url: str, payload: Any) -> int:
    import requests
    response = requests.post(url, json=payload)
    return response.status_code


# =============================================================================
# B. HTTP response operation
# =============================================================================

@sugar.bind(
    concept="concept:http-response",
    library="requests",
    family="concept:family:http",
    version="2",
    loss=["header-case-sensitivity", "streaming-body"],
)
def response_from_parts(
    status: int,
    hdrs: Optional[Mapping[str, str]],
    body: bytes,
) -> requests.Response:
    import requests
    response = requests.Response()
    response.status_code = status
    response.headers.update(hdrs or {})
    response._content = bytes(body)
    return response


@sugar.bind(
    concept="concept:http-response",
    library="requests",
    family="concept:family:http",
    version="2",
    loss=["response-projected-to-status", "response-headers", "response-body", "streaming-body"],
)
def status_code(response: requests.Response) -> int:
    return response.status_code


# =============================================================================
# C. Explicit refusals for nearby carrier surfaces
# =============================================================================

@refuse(
    surface="requests.structures.CaseInsensitiveDict",
    concept="concept:header-map",
    reason="The catalog header-map is a duplicate-preserving multimap. requests CaseInsensitiveDict collapses duplicate field names and performs case-insensitive lookup, so it cannot exactly carry the catalog HeaderMap shape.",
    would_close_with_cluster="A requests-level header carrier that preserves duplicate header entries and original field-name bytes",
)
class RefusedCaseInsensitiveDictHeaderMap:
    pass


@refuse(
    surface="requests.PreparedRequest.prepare_url",
    concept="concept:url",
    reason="prepare_url normalizes and percent-encodes a URL string for transmission; it does not expose the catalog concept:url constructor over Scheme, Authority, Path, Optional<Query>, and Optional<Fragment> without normalization loss.",
    would_close_with_cluster="A requests URL surface exposing the five catalog URL slots before normalization",
)
class RefusedPreparedRequestUrl:
    pass


@refuse(
    surface="requests.Response.iter_content",
    concept="concept:byte-stream",
    reason="iter_content is a response-consumption iterator whose chunking depends on runtime buffering and caller chunk_size. The catalog byte-stream carrier is an ordered byte-chunk value with an optional length hint, not a live network response iterator.",
    would_close_with_cluster="A stable requests byte-stream value surface with explicit chunks and length hint independent of network consumption",
)
class RefusedResponseIterContentByteStream:
    pass
