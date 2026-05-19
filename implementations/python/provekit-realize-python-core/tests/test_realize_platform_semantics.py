from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.platform_semantics import declaration, dimension_values


EXPECTED_DIMENSIONS = {
    "ArithmeticOverflow": "ArbitraryPrecision",
    "IntegerDivisionRounding": "Floor",
    "ShiftMode": "Arithmetic",
    "NullSemantics": "RaiseZeroDivisionError",
    "BitwiseSemantics": "TwosComplement",
}

# Golden CIDs verified against Rust reference implementation
# (provekit-ir-types DimensionValueMemento::recompute_cid, kit_cid elided).
GOLDEN_DIM_VALUE_CIDS: dict[tuple[str, str], str] = {
    ("ArithmeticOverflow", "ArbitraryPrecision"):
        "blake3-512:d528ffa68485e200a65ac1119b3561aa28b56f52a04a31059ff41afeeff812843c4b9b12be9682445481b304e30d166baa64bc03fdb5f0fe40e07a0b1091d373",
    ("IntegerDivisionRounding", "Floor"):
        "blake3-512:aaed397b2639bbbf644216c9c5b70c636c7791c90e4499fc0ed8a77f895d9a7427fee0b751a2f7bbeffc78533708ca906c25937b0941b5a25481c7512f6cf786",
    ("ShiftMode", "Arithmetic"):
        "blake3-512:4dd1d40866beefad0672ae19d9dc0f967a9cedbca31ad50f3e2d57376ccfe9b730155c0974801d93c849541387ace7ae648716397b12f8bb69c2c6ae4c48b5d5",
    ("NullSemantics", "RaiseZeroDivisionError"):
        "blake3-512:676366cdedf3f53cdf4eade664dd570644217c86f596bb4a18d609723f43512ca100ca1903ef5cd9e2f10cb7309288054cafdcc22734439f9509b9bcad667628",
    ("BitwiseSemantics", "TwosComplement"):
        "blake3-512:01a8d218214a9344ac9f0a1a9b25d429eb8b0a72bd7d535a6377794f7769b3ded74f28b0073e5b64a5e5a64276d2cdeb4fb5ea64c2c5b007dfe5859b9cb13a45",
}

# Golden tag CIDs (all 12 tags share the same dimension map, so each has a unique CID
# from the op_cid field).
GOLDEN_TAG_CIDS: dict[str, str] = {
    "blake3-512:95fc70e63a5550fd2e25142f13932919c59d085654ab387789c798886b0111c61d28fe533fc98b50df70eea9428a9af8aa75372c8b1c1deb3acc1a4094790468":
        "blake3-512:63e60467c70e6b34af42c9b3ebbd3e4db9688714ecad57bf3e2376a7d36bfb7ac289474a8d0ddba754c2aede056106a6d5f0721307cf3fe676ec32a77b84a4d7",
    "blake3-512:b7c54558573348bb3a9297732547a8e6e9d152403d292df7426b6bb8a248f705b4b030bf2a22ba547a17d6f1bfaf8e75a6843e02e8f23a8226ebc09e2a8622af":
        "blake3-512:657fdc5f667064c8b5c3193501204b52dfbb10af0c447c59dbdc1e0c737f6391d3f82885a66c00a394be65189b3843886533f5c5763346fe606d77c7e4ee1e96",
    "blake3-512:46cd627de058c8d4f7d087ea33f4904af65ad4b2e3cfd3aff8f44bf27db96b33c2dae39cd30f53898c233c9465ba8d2701c69e5903d48935113103b4db00fd03":
        "blake3-512:5011065703d6d870d17ceda78490bfc0988df1a174e85be1fd078856af666ad46d2830487b9cdafe3be3c8e909a280fb754378815c883afb8a2d136b8b64687b",
    "blake3-512:c6a13abbcafdf83edcff49d883a7c7440faadd8af896da0ad46e2bcb177ed0649d005b4ddecd4689cf565b10679219a07c784399bafe5c6174642e1b808d7839":
        "blake3-512:695abfb248e8591b1438fe6143c1b22dd5bb59a0883cf76f4ff7ffd7364c61c101f31166c3251d9f9908c71666542ba74d26108ac8f7c238e31dbfce1fdf3006",
    "blake3-512:92340897b43965e01454b00a6a43ec54b2bf0e01213a45fa2311f730dde18adf8da97a22458c1a2a0fb23ce85ef3ad9b22e704804c74f41997aba3ba02cefe0d":
        "blake3-512:60d7c816863a1a5bc2d43b08f3948c7dd33ecc0845be031781e1a8c33c7375c0a6ff3222be5b5b78ba0fe10ac6f53b4a8dbbf9c92ff7b34c4ec71f9f7be3d45a",
    "blake3-512:f9cdfcba8d0e223803126504a2a6ed10005fa61acb5c55b74b270bc66d963eb7648ab6763f0510760df93145c0f6670087a403417e8b3100c7142e121111807a":
        "blake3-512:fbb23ea99fff7777f54e1a4a7777fa537405f9d75613de202e1d9b2fef844e609ab0a4b72840a28e8a9df5e0cb2f011686de2a4e859b441e738aa567f7633698",
    "blake3-512:c90e3c159b25e4c4c7f9c899da5aa3ee048a548719ced7360f3e514450811096b21cd5473f22d7a05df088f92210bbc916e65970b9fa1e1511c193ed969f112b":
        "blake3-512:5791fc0c94066d6e5b67e2093a654beb4f3512cb2643e28711108ee1598cd308a9a2867d982b4c57fb9399c3023b3e565583138aec595aed74d8f051552bee1b",
    "blake3-512:9e96c2445bad6bb1e5a6f902ad7f733e3f4619829b9c0e232361fbf50b978c8332029212ed895762e604d1df009fce58848cda33524a697df798233eae30a14b":
        "blake3-512:79f9d07e955b9a8afb3afe6bf9675abb83c7153d68d194b230cf95d9fc6b98a7181bbcb5bb74bdadf4482fc22b33dd630ea646e4322216f0e2747c9a498c6eca",
    "blake3-512:d57b54bffe698ed804a4a49486b73a1a8a3e7bd84fb12babaad01ce22d8b7bcb5a35f3476324063f8de9f8090846d0d4fbeb48d78475d07e16f7925b4f264de3":
        "blake3-512:751e5353ce6bbb58ecf956b3656bf8c8ca5458e31b59da23cadaae3629e0da690dc7f9db7ad4ac77cdbf507154e6d31cba33a55614775491096faf176214e66f",
    "blake3-512:343b1f9faa98218467d810e0a2bb1b1eebeaf921c71a1bc52141f885220afff482c631c52e2157a6067640f4830f928add53ef7aa0386c6a27ee3c8bab6dc353":
        "blake3-512:bffc15859312d84608e2728e116e5c1eb4bc4a9b0fed8a3963a1bdf71c4bdd37c3ffad6390882fbf604095ca7317d41aaf325e6b20819c74e95bb3c6e94d2ea0",
    "blake3-512:5e788f0d551081f4e709e4418e01017fa9ae1c04963e7be2862fadad8a8434fafa204629fbec53e2e44624c195ac2e32c0410df25cf8ff3a4be672582f89109f":
        "blake3-512:210b62a4782185b311f8791e3886c6432e3a486250621398eb4f520f1b84b11f16c68908c9e2d9549b0937a4760f3e45e1d5fa386c119cf48646762af4f2216d",
    "blake3-512:ad958847b50cf07ddbb92d85ae488a5f983d5619e108476b42e519174cfcce883ecd637544a372b946bb45a1c22893c710bc9b08ea0569ad0e035b3babb6a409":
        "blake3-512:c75facd259c4e04eeb096c715c2507f0040a2043a3cb9f342d47923f069c1bde2c96d0b067b6b160fe5d9a07508d0f416ff1e3e7801d60fef77f271c959bc578",
}


def test_python_realize_platform_semantics_declaration_shape() -> None:
    values = dimension_values()
    assert {item["dimension_name"]: item["value_name"] for item in values} == EXPECTED_DIMENSIONS
    for item in values:
        assert item["compare_to"] == {
            "kind": "atomic",
            "name": f"python:{item['value_name']}",
            "args": [],
        }
        assert item["cid"].startswith("blake3-512:")

    semantics = declaration()
    assert semantics["tags"]
    for tag in semantics["tags"]:
        assert set(tag["dimensions"]) == set(EXPECTED_DIMENSIONS)
        assert tag["op_cid"].startswith("blake3-512:")
        assert tag["cid"].startswith("blake3-512:")
    assert "dimension_values" in semantics
    assert len(semantics["dimension_values"]) == 5


def test_dimension_value_cids_match_golden() -> None:
    values = dimension_values()
    for item in values:
        key = (item["dimension_name"], item["value_name"])
        expected = GOLDEN_DIM_VALUE_CIDS[key]
        assert item["cid"] == expected, (
            f"CID mismatch for {key}: got {item['cid']}, expected {expected}"
        )


def test_tag_cids_match_golden() -> None:
    semantics = declaration()
    for tag in semantics["tags"]:
        op_cid = tag["op_cid"]
        expected = GOLDEN_TAG_CIDS[op_cid]
        assert tag["cid"] == expected, (
            f"tag CID mismatch for op_cid={op_cid[:30]}...: "
            f"got {tag['cid']}, expected {expected}"
        )


def test_rpc_dispatch_platform_semantics() -> None:
    import sys
    from pathlib import Path
    ROOT = Path(__file__).resolve().parents[4]
    PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
    if str(PKG_SRC) not in sys.path:
        sys.path.insert(0, str(PKG_SRC))
    from provekit_realize_python_core.rpc import dispatch

    request = {"jsonrpc": "2.0", "id": 1, "method": "provekit.plugin.platform_semantics", "params": {}}
    response = dispatch(request)
    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 1
    assert "result" in response
    result = response["result"]
    assert "tags" in result
    assert "dimension_values" in result
    assert len(result["tags"]) == 12
    assert len(result["dimension_values"]) == 5
