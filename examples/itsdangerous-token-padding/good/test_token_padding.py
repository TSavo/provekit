# GOOD TWIN: itsdangerous.encoding.base64_encode strips ALL trailing '='
# (return urlsafe_b64encode(s).rstrip(b"=")). The unpadded value is the
# vendor's sworn behavior; the walked no-suffix universe agrees.
import itsdangerous.encoding as enc
import itsdangerous.exc as exc
import itsdangerous._json as compact_json
import itsdangerous.serializer as serializer_mod
import itsdangerous.signer as signer
import itsdangerous.timed as timed
from itsdangerous.exc import BadPayload
import pytest


def test_token_padding():
    assert enc.base64_encode(b"provekit") == b"cHJvdmVraXQ"


def test_int_to_bytes_canonical_form():
    assert enc.int_to_bytes(1) == b"\x01"


def test_none_algorithm_signature():
    alg = signer.NoneAlgorithm()
    assert alg.get_signature(b"k", b"v") == b""


def test_hmac_algorithm_default_digest_method():
    alg = signer.HMACAlgorithm()
    assert alg.digest_method == alg.default_digest_method


def test_signer_default_key_derivation():
    alg = signer.Signer("secret")
    assert alg.key_derivation == signer.Signer.default_key_derivation


def test_signer_secret_key_property():
    alg = signer.Signer("secret")
    assert alg.secret_key == b"secret"


def test_signer_validate_rejects_bad_signature():
    alg = signer.Signer("secret")
    assert alg.validate(b"bad") == False


def test_signer_none_key_derivation_returns_secret_key():
    alg = signer.Signer("secret", key_derivation="none")
    assert alg.derive_key(b"raaaa") == b"raaaa"


def test_signing_algorithm_get_signature_is_abstract():
    with pytest.raises(NotImplementedError):
        signer.SigningAlgorithm.get_signature(None, b"k", b"v")


def test_bad_data_message():
    err = exc.BadData("raaaa")
    assert err.__str__() == "raaaa"


def test_bad_signature_payload():
    err = exc.BadSignature("bad", payload=b"payload")
    assert err.payload == b"payload"


def test_bad_header_header():
    err = exc.BadHeader("bad", payload=b"payload", header={"kid": "k"})
    assert err.header == {"kid": "k"}


def test_bad_payload_default_original_error():
    err = exc.BadPayload("bad")
    assert err.original_error == None


def test_compact_json_loads():
    assert compact_json._CompactJSON.loads('{"ok": true}') == {"ok": True}


def test_compact_json_dumps():
    assert compact_json._CompactJSON.dumps({"ok": True}) == '{"ok":true}'


def test_default_serializer_is_text():
    assert (
        serializer_mod.is_text_serializer(serializer_mod.Serializer.default_serializer)
        == True
    )


def test_serializer_default_signer_kwargs():
    ser = serializer_mod.Serializer("secret")
    assert ser.signer_kwargs == {}


def test_serializer_load_payload_bad_payload():
    with pytest.raises(BadPayload):
        serializer_mod.Serializer.load_payload(
            serializer_mod.Serializer("secret"), b"bad"
        )


def test_timed_serializer_loads_unsafe_bad_payload():
    assert (
        timed.TimedSerializer.loads_unsafe(
            timed.TimedSerializer("secret"), "bad"
        )
        == (False, None)
    )


def test_timestamp_signer_validate_rejects_bad_signature():
    alg = timed.TimestampSigner("secret")
    assert alg.validate(b"bad") == False
