# GOOD TWIN: the URL-safe value for an input the vendor never tested
# (grep test.test_base64: "provekit~seam" appears nowhere). The walked
# translate universe (Lib/base64.py: bytes.maketrans(b'+/', b'-_')) and
# this sworn equality are consistent: '-' survives, '+' cannot appear.
import base64


def test_urlsafe_seam():
    assert base64.urlsafe_b64encode(b"provekit~seam") == b"cHJvdmVraXR-c2VhbQ=="
