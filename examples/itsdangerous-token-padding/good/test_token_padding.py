# GOOD TWIN: itsdangerous.encoding.base64_encode strips ALL trailing '='
# (return urlsafe_b64encode(s).rstrip(b"=")). The unpadded value is the
# vendor's sworn behavior; the walked no-suffix universe agrees.
import itsdangerous.encoding as enc


def test_token_padding():
    assert enc.base64_encode(b"provekit") == b"cHJvdmVraXQ"
