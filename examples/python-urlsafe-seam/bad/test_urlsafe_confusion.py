# BAD TWIN: the urlsafe confusion -- asserting the STANDARD-alphabet
# value ('+' at position 12) for the urlsafe encoder, on an input the
# vendor never tested. No vendor vector exists to catch this point-wise;
# the walked universe (output never contains '+' or '/') refutes it
# statically: two byte literals in the vendor's own source convict.
import base64


def test_urlsafe_confusion():
    assert base64.urlsafe_b64encode(b"provekit~seam") == b"cHJvdmVraXR+c2VhbQ=="
