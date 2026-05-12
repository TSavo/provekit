import sys
import unittest
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import discharge

CID_FILE = Path(__file__).resolve().parents[1] / "cids.tsv"


class CidInvariantTests(unittest.TestCase):
    """One-name-one-CID invariant: every (kind, name) pair must map to exactly one CID.

    Duplicates indicate that two mint passes produced different CIDs for the same logical
    object.  This class of silent corruption was the root cause of the Blocker 2 in PR3
    (primitive_ops.py and mint_language_morphisms.py both minted c11/rust if/seq/skip with
    stale vs live source CIDs, and append_cids silently kept the first-registered stale CID).

    The test runs on the cids.tsv from the PREVIOUS mint run, so it catches regressions on
    the second and subsequent runs.  If cids.tsv is absent (first run) the test passes
    trivially.
    """

    def test_one_name_one_cid_in_cids_tsv(self):
        if not CID_FILE.exists():
            self.skipTest("cids.tsv not yet generated (first run)")
        name_to_cids: dict[tuple[str, str], set[str]] = defaultdict(set)
        for line in CID_FILE.read_text(encoding="utf-8").splitlines()[1:]:
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            key = (parts[0], parts[1])
            name_to_cids[key].add(parts[2])
        violations = {k: v for k, v in name_to_cids.items() if len(v) > 1}
        self.assertEqual(
            violations,
            {},
            msg=(
                "One-name-one-CID violation in cids.tsv.  The following (kind, name) pairs "
                "map to more than one CID, meaning two mint passes produced conflicting "
                "content-addresses for the same logical object:\n"
                + "\n".join(
                    f"  {kind}:{name} -> {sorted(cids)}"
                    for (kind, name), cids in sorted(violations.items())
                )
            ),
        )


class ConceptShapeDischargeTests(unittest.TestCase):
    def test_branch_operator_and_slot_renaming_lands_on_shape(self):
        target = discharge.branch_shape()
        source = discharge.branch_source_c()
        normalized = discharge.after_substitution_payload(
            source,
            target,
            {"status": "x", "fail_value": "err_val", "result": "ret"},
            {"int": "Value"},
            {"c_status_is_error": "err_cond"},
            {},
        )
        self.assertEqual(target, normalized)

    def test_alloc_rust_representation_lands_on_shape(self):
        target = discharge.allocation_shape()
        source = discharge.allocation_source_rust()
        normalized = discharge.after_substitution_payload(
            source,
            target,
            {
                "len": "n",
                "err_code": "err",
                "ok_value": "continuation_value",
                "buf": "p",
                "out": "ret",
            },
            {"usize": "Size", "isize": "ReturnValue", "RawVec": "Buffer"},
            {"try_reserve_failed": "alloc_failed", "raw_vec_capacity_ge": "valid_buffer"},
            {},
        )
        self.assertEqual(target, normalized)

    def test_validate_c_source_lands_on_commit_then_error_shape(self):
        target = discharge.validate_shape()
        source = discharge.validate_source_c()
        normalized = discharge.after_substitution_payload(
            source,
            target,
            {
                "record": "x",
                "error_rc": "err",
                "new_state": "committed_state",
                "result": "ret",
                "global_state": "state",
            },
            {"record_ptr": "Candidate", "int": "Outcome", "state_handle": "Outcome"},
            {"c_valid_record": "valid", "c_commit_applied": "commit_applied"},
            {},
        )
        self.assertEqual(target, normalized)


class WriteJsonCanonicalTests(unittest.TestCase):
    """Regression: Blocker 2 -- write_json must produce JCS-canonical (sorted-key) output.

    Previously write_json used json.dump without sort_keys=True, so the on-disk
    key order was Python insertion order.  Any consumer that recomputes a CID from
    the on-disk bytes would get a different BLAKE3 than the registered CID.
    """

    def test_write_json_produces_sorted_key_order(self):
        import json
        import tempfile

        # A dict with intentionally non-alphabetical insertion order
        obj = {"z_key": 1, "a_key": 2, "m_key": 3}
        with tempfile.NamedTemporaryFile(
            mode="r", suffix=".json", delete=False
        ) as tmp:
            tmp_path = Path(tmp.name)

        try:
            discharge.write_json(tmp_path, obj)
            on_disk = tmp_path.read_text(encoding="utf-8")
            parsed = json.loads(on_disk)
            canonical = json.dumps(parsed, sort_keys=True, indent=2, ensure_ascii=True) + "\n"
            self.assertEqual(
                on_disk,
                canonical,
                "write_json must produce sort_keys=True output so on-disk bytes are "
                "identical to re-serialized canonical form; "
                f"got:\n{on_disk!r}\nwant:\n{canonical!r}",
            )
        finally:
            tmp_path.unlink(missing_ok=True)

    def test_write_json_gap_memento_key_order_matches_canonical(self):
        """A gap memento written via write_json must have alphabetically-sorted keys."""
        import json
        import tempfile

        memento = {
            "fn_name": "gap:python:add:to:concept:add",
            "gap_kind": "polymorphic-source-op",
            "kind": "TransportGapMemento",
            "resolution_options": [],
            "schema_version": "1",
            "source_lang": "python",
            "source_op_cid": "blake3-512:aaaa",
            "target_concept_op": "concept:add",
        }
        with tempfile.NamedTemporaryFile(
            mode="r", suffix=".json", delete=False
        ) as tmp:
            tmp_path = Path(tmp.name)

        try:
            discharge.write_json(tmp_path, memento)
            on_disk_keys = list(json.loads(tmp_path.read_text(encoding="utf-8")).keys())
            expected_keys = sorted(on_disk_keys)
            self.assertEqual(
                on_disk_keys,
                expected_keys,
                f"Keys must be in alphabetical order; got {on_disk_keys}",
            )
        finally:
            tmp_path.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
