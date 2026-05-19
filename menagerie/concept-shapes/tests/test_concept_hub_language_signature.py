"""Schema + content validation for the substrate-concept-hub LanguageSignatureMemento.

Per the ruling at `docs/plans/2026-05-19-concept-hub-language-signature-ruling.md`
and #1290, the concept-hub-signature anchors `target_language_signature_cid`
for cross-language morphism mementos. This test pins:

1. The signature spec file parses as JSON.
2. Its `cid` field matches the JCS+blake3-512 recompute over the `memento`.
3. The signature's `post.sorts` enumerates every sort under
   `menagerie/concept-shapes/catalog/sorts/`.
4. The signature's `post.operations` enumerates every `concept:*` op under
   `menagerie/concept-shapes/catalog/algorithms/`.
5. The minting script is deterministic: re-running produces byte-identical
   output.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SIG_PATH = (
    ROOT
    / "menagerie"
    / "concept-hub-language-signature"
    / "specs"
    / "language_signature_concept_hub.spec.json"
)
CATALOG_SORTS_DIR = ROOT / "menagerie" / "concept-shapes" / "catalog" / "sorts"
CATALOG_ALGORITHMS_DIR = ROOT / "menagerie" / "concept-shapes" / "catalog" / "algorithms"
CIDS_TSV = ROOT / "menagerie" / "concept-shapes" / "cids.tsv"
MINT_SCRIPT = (
    ROOT / "menagerie" / "concept-shapes" / "scripts" / "mint_concept_hub_signature.py"
)

CID_FILENAME_RE = re.compile(r"^(?P<name>.+)\.(?P<cid>blake3-512:[0-9a-f]+)\.json$")


class ConceptHubLanguageSignatureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.envelope = json.loads(SIG_PATH.read_text(encoding="utf-8"))
        cls.memento = cls.envelope["memento"]
        cls.cid = cls.envelope["cid"]

    def test_envelope_shape(self) -> None:
        self.assertIn("memento", self.envelope)
        self.assertIn("cid", self.envelope)
        self.assertIn("signature", self.envelope)
        self.assertTrue(self.cid.startswith("blake3-512:"))

    def test_memento_kind_and_fn_name(self) -> None:
        self.assertEqual(self.memento["kind"], "LanguageSignatureMemento")
        self.assertEqual(self.memento["fn_name"], "concept-hub:v1")
        self.assertEqual(self.memento["protocol"], "AMP")
        self.assertEqual(self.memento["schema_version"], "1")

    def test_post_bundle_shape(self) -> None:
        post = self.memento["post"]
        self.assertEqual(post["kind"], "language-signature-bundle")
        self.assertIn("sorts", post)
        self.assertIn("operations", post)
        self.assertEqual(post["equations"], [])
        self.assertEqual(post["effect_signatures"], [])

    def test_sorts_enumerate_catalog(self) -> None:
        """The signature's `post.sorts` must enumerate every catalog/sorts/ entry."""
        expected_cids = sorted(
            _extract_cid(p.name) for p in CATALOG_SORTS_DIR.glob("*.json")
        )
        actual_cids = sorted(self.memento["post"]["sorts"])
        self.assertEqual(
            actual_cids,
            expected_cids,
            "signature's sorts must enumerate exactly the catalog/sorts/ entries",
        )

    def test_operations_enumerate_concept_ops(self) -> None:
        """The signature's `post.operations` must enumerate every concept:* algorithm."""
        expected_cids = sorted(
            _extract_cid(p.name)
            for p in CATALOG_ALGORITHMS_DIR.glob("concept:*.json")
            if ":to:" not in p.name and ":to-shape" not in p.name
        )
        actual_cids = sorted(self.memento["post"]["operations"])
        self.assertEqual(
            actual_cids,
            expected_cids,
            "signature's operations must enumerate exactly the catalog/algorithms/concept:* entries",
        )

    def test_cid_recompute(self) -> None:
        """The signature CID must match JCS+blake3-512 over the memento content."""
        from blake3 import blake3

        canonical = json.dumps(
            self.memento, sort_keys=True, separators=(",", ":"), ensure_ascii=False
        ).encode("utf-8")
        recomputed = "blake3-512:" + blake3(canonical).digest(length=64).hex()
        self.assertEqual(
            self.cid,
            recomputed,
            "envelope.cid must match recomputed JCS+blake3-512 of memento",
        )

    def test_cids_tsv_pin(self) -> None:
        """cids.tsv must carry an entry pinning the current signature CID."""
        for line in CIDS_TSV.read_text(encoding="utf-8").splitlines():
            parts = line.split("\t")
            if len(parts) >= 3 and parts[1] == "language_signature_concept_hub":
                self.assertEqual(parts[2], self.cid)
                return
        self.fail("cids.tsv missing language_signature_concept_hub pin")

    def test_mint_script_deterministic(self) -> None:
        """Re-running the mint script must produce byte-identical output."""
        before = SIG_PATH.read_bytes()
        result = subprocess.run(
            [sys.executable, str(MINT_SCRIPT)],
            capture_output=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            f"mint script failed: stdout={result.stdout!r} stderr={result.stderr!r}",
        )
        after = SIG_PATH.read_bytes()
        self.assertEqual(
            before,
            after,
            "mint script must be deterministic; second run produced different bytes",
        )


def _extract_cid(filename: str) -> str:
    """Extract the blake3-512 CID portion from a `<name>.blake3-512:<hex>.json` filename."""
    match = CID_FILENAME_RE.match(filename)
    if not match:
        raise ValueError(f"catalog filename does not match CID convention: {filename}")
    return match.group("cid")


if __name__ == "__main__":
    unittest.main()
