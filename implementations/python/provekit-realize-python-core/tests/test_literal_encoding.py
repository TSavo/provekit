from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.literal_encoding import answers
from provekit_realize_python_core.rpc import dispatch

# Canonical sort CIDs (from #1282)
SORT_INT_CID = "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58"
SORT_FLOAT_CID = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10"
SORT_BOOL_CID = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"
SORT_BYTES_CID = "blake3-512:7116ef6e62e6739b213a8394f975a53c771b89f08c36d27143827acfcfebc0e39e5b82c530be668c3cfd5ec6966ccaa42930b37fdb1f4ac25652a970be10fb6b"
SORT_NULL_CID = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5"

# Golden LiteralEncodingMemento CIDs (kit_cid elided per #1262 / #1271)
# Python uses json.dumps(sort_keys=True) for CID computation, so these
# differ from the Rust golden CIDs (which use proper JCS via provekit-canonicalizer).
GOLDEN_CIDS: dict[str, str] = {
    SORT_INT_CID: "blake3-512:9f52add0c36ff83c1f5605cf67ddcc4858729573ba972fb1f3d2605aa519ecd74405579993ad6772053d5d14d1cd0376a398f84fe45411790d6ff72b44757134",
    SORT_FLOAT_CID: "blake3-512:f6977da3b1edbc3b82e97bfec23877438ea75ec8467a57bbe4eec3a3ce65cd83856956acaf8c92c2fc5dbb4bd85fd0f26a4a41689c7fb680d71920e95fa62759",
    SORT_STRING_CID: "blake3-512:978e9c766dc1c45e0abf38c00982c30194556922d809235f317f5f8b474a18898703dcc51d6b9efb58ce173facb7af42cb8b939d729a37a42c1ee567b5f19421",
    SORT_BOOL_CID: "blake3-512:076494a0ea47110a33b287c5f9e2f421925d4662e1f485540462dc513dff49b6c7e8875407c03699e20bce2bdd849ddc09f944993231a2288ba0817ab3cb4962",
    SORT_BYTES_CID: "blake3-512:5eec8ac37173118a18c4df393ea5e88ff04a7fbc9ae01432c1b46a9550d36f816fd425338adf496035286262e75df2970c6c8e6cc447427f5231d50f489340e4",
    SORT_NULL_CID: "blake3-512:b3f4521c46f00c649c379a4d935e40e126a567a41518707e616847a22c3baf1d8122123ee0092ac95875f302738f7632ad9e734b983309986a5aab287b8355e6",
}


def test_python_literal_encoding_answers_count():
    a = answers()
    assert len(a) == 6, "Python admits Int, Float, String, Bool, Bytes, Null (6 sorts)"


def test_python_literal_encoding_answers_sort_cids():
    a = answers()
    sort_cids = {m["sort_cid"] for m in a}
    assert SORT_INT_CID in sort_cids
    assert SORT_FLOAT_CID in sort_cids
    assert SORT_STRING_CID in sort_cids
    assert SORT_BOOL_CID in sort_cids
    assert SORT_BYTES_CID in sort_cids
    assert SORT_NULL_CID in sort_cids


def test_python_literal_encoding_answers_language():
    a = answers()
    for m in a:
        assert m["language"] == "python", f"language must be python, got {m['language']}"


def test_python_literal_encoding_answers_kind():
    a = answers()
    for m in a:
        assert m["kind"] == "literal-encoding-memento"


def test_python_literal_encoding_answers_cid_format():
    a = answers()
    for m in a:
        assert m["cid"].startswith("blake3-512:"), f"CID must start with blake3-512:"
        assert len(m["cid"]) > 20, "CID must not be empty"


def test_python_literal_encoding_answers_golden_cids():
    # Regression: golden CIDs must not change.
    a = answers()
    by_sort = {m["sort_cid"]: m["cid"] for m in a}
    for sort_cid, expected_cid in GOLDEN_CIDS.items():
        assert by_sort[sort_cid] == expected_cid, (
            f"Golden CID mismatch for sort {sort_cid[:20]}: "
            f"got {by_sort[sort_cid]}, expected {expected_cid}"
        )


def test_python_literal_encoding_answers_rpc_dispatch():
    response = dispatch({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.literal_encoding_answers",
        "params": {},
    })
    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 1
    assert "result" in response
    assert "answers" in response["result"]
    assert len(response["result"]["answers"]) == 6
