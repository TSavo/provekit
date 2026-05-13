#!/usr/bin/env python3
"""
test_assert_cell_pin.py -- validation test for concept:assert cell pins.

This test verifies that the concept:assert abstraction and its C realization
are correctly minted to the catalog with proper content-addressing.

Tests:
1. concept:assert abstraction file exists and is valid JSON
2. concept:assert->c:one-line-macro realization file exists and is valid JSON
3. Realization CID is correctly referenced in the abstraction
"""
import json
import sys
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG = BASE / "catalog"
ABST_DIR = CATALOG / "abstractions"
REAL_DIR = CATALOG / "realizations"


def test_assert_abstraction_pin():
    """Test that concept:assert abstraction is minted correctly."""
    entries = list(ABST_DIR.glob("concept:assert.*.json"))
    assert len(entries) > 0, "concept:assert abstraction not found in catalog"

    # Use the most recent one (by sorting)
    abst_file = sorted(entries)[-1]
    content = abst_file.read_text(encoding="utf-8")
    data = json.loads(content)

    assert data["memento"]["operator"] == "concept:assert", "operator field mismatch"
    assert isinstance(data["memento"]["realizations"], list), "realizations must be a list"
    assert "cid" in data, "cid field must be present"

    print(f"✓ concept:assert abstraction: {data['cid'][:40]}...")
    return data["memento"]["realizations"]


def test_assert_realization_pin():
    """Test that concept:assert->c:one-line-macro realization is minted correctly."""
    entries = list(REAL_DIR.glob("concept:assert->c:one-line-macro.*.json"))
    assert len(entries) > 0, "concept:assert->c:one-line-macro realization not found in catalog"

    # Use the most recent one (by sorting)
    real_file = sorted(entries)[-1]
    content = real_file.read_text(encoding="utf-8")
    data = json.loads(content)

    assert data["memento"]["fn_name"] == "concept:assert->c:one-line-macro", "fn_name mismatch"
    assert data["memento"]["target_lang"] == "c", "target_lang must be c"
    assert isinstance(data["memento"]["loss_record"], dict), "loss_record must be a dict"
    assert "cid" in data, "cid field must be present"

    print(f"✓ concept:assert->c:one-line-macro realization: {data['cid'][:40]}...")
    return data["cid"]


def test_realization_referenced_in_abstraction(abst_realizations, real_cid):
    """Test that the realization CID is correctly referenced in the abstraction."""
    assert real_cid in abst_realizations, (
        f"realization CID {real_cid[:40]}... not found in abstraction "
        f"realizations list: {[c[:40] + '...' for c in abst_realizations]}"
    )
    print(f"✓ Realization CID correctly referenced in abstraction")


def main():
    print("[TEST] assert_c_cell_pin validation...")
    try:
        abst_realizations = test_assert_abstraction_pin()
        real_cid = test_assert_realization_pin()
        test_realization_referenced_in_abstraction(abst_realizations, real_cid)
        print("\n[PASS] All tests passed")
        return 0
    except AssertionError as e:
        print(f"\n[FAIL] {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
