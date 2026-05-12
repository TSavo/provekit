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


if __name__ == "__main__":
    unittest.main()
