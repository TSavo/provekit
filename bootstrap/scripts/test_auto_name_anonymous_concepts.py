from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / "bootstrap/scripts/auto_name_anonymous_concepts.py"
RECEIPT = ROOT / "bootstrap/auto-named-concepts/receipt.json"


class AutoNameAnonymousConceptsTest(unittest.TestCase):
    def test_fake_llm_replaces_anonymous_concept_comments(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            source_root = Path(tmp)
            source = source_root / "sample.rs"
            source.write_text(
                "\n".join(
                    [
                        "// concept: UNNAMED-CONCEPT-1",
                        "// sugar:concept[blake3-512:0123456789abcdef](UNNAMED-CONCEPT-1)",
                        "pub fn deposit_then_balance(balance: i64, amount: i64) -> i64 {",
                        "    let next = balance + amount;",
                        "    next",
                        "}",
                        "",
                        "// concept: UNNAMED-CONCEPT-2",
                        "pub fn checked_withdraw(balance: i64, amount: i64) -> i64 {",
                        "    if balance >= amount { balance - amount } else { balance }",
                        "}",
                        "",
                    ]
                ),
                encoding="utf-8",
            )

            if RECEIPT.exists():
                RECEIPT.unlink()

            proc = subprocess.run(
                [
                    sys.executable,
                    str(DRIVER),
                    str(source_root),
                    "--llm-mode",
                    "deterministic",
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(
                proc.returncode,
                0,
                msg=f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}",
            )

            edited = source.read_text(encoding="utf-8")
            self.assertIn("// concept: deposit-then-balance", edited)
            self.assertIn(
                "// sugar:concept[blake3-512:0123456789abcdef](deposit-then-balance)",
                edited,
            )
            self.assertIn("// concept: checked-withdraw", edited)
            self.assertNotIn("UNNAMED-CONCEPT-1", edited)
            self.assertNotIn("UNNAMED-CONCEPT-2", edited)
            self.assertNotIn("\u2014", edited)

            receipt = json.loads(RECEIPT.read_text(encoding="utf-8"))
            self.assertEqual(receipt["annotated_source_root"], str(source_root.resolve()))
            self.assertEqual(len(receipt["entries"]), 3)
            self.assertEqual(
                [entry["proposed_name"] for entry in receipt["entries"]],
                ["deposit-then-balance", "deposit-then-balance", "checked-withdraw"],
            )
            self.assertTrue(all(entry["edit_succeeded"] for entry in receipt["entries"]))


if __name__ == "__main__":
    unittest.main()
