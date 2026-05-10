import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import discharge


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
