# BAD TWIN: the token-padding confusion -- asserting the PADDED standard
# base64url value for itsdangerous' stripped encoding. rstrip(b"=") is
# total: no output of base64_encode ever ends with '='. The walked
# universe (one byte literal in the vendor's own source) refutes this
# claim statically, for every input, including ones nobody ever tested.
import itsdangerous.encoding as enc


def test_token_padding():
    assert enc.base64_encode(b"provekit") == b"cHJvdmVraXQ="
