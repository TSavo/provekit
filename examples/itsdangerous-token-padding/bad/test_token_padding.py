# BAD TWIN: the token-padding confusion -- asserting the PADDED standard
# base64url value for itsdangerous' stripped encoding. rstrip(b"=") is
# total: no output of base64_encode ever ends with '='. The walked
# universe (one byte literal in the vendor's own source) refutes this
# claim statically, for every input, including ones nobody ever tested.
import itsdangerous.encoding as enc
import itsdangerous.exc as exc
import itsdangerous._json as compact_json
import itsdangerous.signer as signer


def test_token_padding():
    assert enc.base64_encode(b"provekit") == b"cHJvdmVraXQ="


def test_int_to_bytes_canonical_form():
    assert enc.int_to_bytes(1) == b"\x01"


def test_none_algorithm_signature():
    alg = signer.NoneAlgorithm()
    assert alg.get_signature(b"k", b"v") == b""


def test_bad_data_message():
    err = exc.BadData("raaaa")
    assert err.__str__() == "raaaa"


def test_bad_signature_payload():
    err = exc.BadSignature("bad", payload=b"payload")
    assert err.payload == b"payload"


def test_bad_header_header():
    err = exc.BadHeader("bad", payload=b"payload", header={"kid": "k"})
    assert err.header == {"kid": "k"}


def test_compact_json_loads():
    assert compact_json._CompactJSON.loads('{"ok": true}') == {"ok": True}
