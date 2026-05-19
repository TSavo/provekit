"""Schema + content validation for SortMorphismMementos targeting concept-hub sorts.

Per #1284 (sort-classification exam-manifest coverage) and the ruling at
`docs/plans/2026-05-19-concept-hub-language-signature-ruling.md`. This test
pins:

1. Each `sort-morphism:<lang>:<sort>:to:concept:<canonical>.<cid>.json`
   parses as a SortMorphismMemento per
   `protocol/specs/2026-05-13-sort-morphism-memento.md` §1.
2. `header.cid` recomputes deterministically from JCS({header, metadata})
   with `cid` elided.
3. `target_language_signature_cid` == concept-hub-signature CID (per
   #1290).
4. `target_sort_cid` == the substrate-canonical sort CID (concept:Float
   for this batch).
5. cids.tsv carries a pin for each minted morphism.
6. The minting script is deterministic: re-running produces byte-identical
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
ALGORITHMS_DIR = ROOT / "menagerie" / "concept-shapes" / "catalog" / "algorithms"
CIDS_TSV = ROOT / "menagerie" / "concept-shapes" / "cids.tsv"
MINT_SCRIPT = (
    ROOT
    / "menagerie"
    / "concept-shapes"
    / "scripts"
    / "mint_sort_morphisms_to_concept_hub.py"
)

CONCEPT_HUB_SIG_CID = (
    "blake3-512:1979babed41ad51ad8d7a28543815f74e24a9d4ee1ae3d52ccc6549f293aa635"
    "19abf5411a67b7882c73333b1b357e4863f6d7781f0b0776e5bd25f90ea7d793"
)
CONCEPT_FLOAT_CID = (
    "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df"
    "5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57"
)
CONCEPT_NULL_CID = (
    "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba"
    "771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5"
)

EXPECTED_FLOAT_LANGS = {"rust", "java", "go", "php", "typescript", "c11", "python", "ruby"}
EXPECTED_NULL_LANGS = {"typescript", "java", "go", "c11", "php", "python", "ruby"}
# Rust intentionally omitted from Null per #1284's null-free-language posture
# (Rust uses Option<T>, not Null).
NULL_FREE_LANGS = {"rust"}

CID_FILENAME_RE = re.compile(
    r"^sort-morphism:(?P<lang>[^:]+):(?P<sort>[^:]+):to:concept:(?P<canonical>[^.]+)"
    r"\.(?P<cid>blake3-512:[0-9a-f]+)\.json$"
)


class SortMorphismsToConceptHubTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.float_files = sorted(
            ALGORITHMS_DIR.glob("sort-morphism:*:to:concept:Float.*.json")
        )
        cls.null_files = sorted(
            ALGORITHMS_DIR.glob("sort-morphism:*:to:concept:Null.*.json")
        )
        cls.all_files = cls.float_files + cls.null_files

    def test_all_expected_float_languages_present(self) -> None:
        seen = {
            CID_FILENAME_RE.match(p.name).group("lang") for p in self.float_files
        }
        self.assertGreaterEqual(
            seen,
            EXPECTED_FLOAT_LANGS,
            f"expected Float morphisms for {EXPECTED_FLOAT_LANGS}; got {seen}",
        )

    def test_all_expected_null_languages_present(self) -> None:
        seen = {
            CID_FILENAME_RE.match(p.name).group("lang") for p in self.null_files
        }
        self.assertGreaterEqual(
            seen,
            EXPECTED_NULL_LANGS,
            f"expected Null morphisms for {EXPECTED_NULL_LANGS}; got {seen}",
        )

    def test_null_free_languages_have_no_null_morphism(self) -> None:
        """Rust (and any other null-free language) MUST NOT have a Null morphism.

        Substrate-honest absence: when a language's SortAdmission excludes a
        sort, no morphism is authored. The exam coverage check accepts the
        absence as the kit's declaration of non-admission.
        """
        seen = {
            CID_FILENAME_RE.match(p.name).group("lang") for p in self.null_files
        }
        for lang in NULL_FREE_LANGS:
            self.assertNotIn(
                lang,
                seen,
                f"{lang} declares no Null admission; must not author a Null morphism",
            )

    def test_envelope_shape(self) -> None:
        for path in self.all_files:
            with self.subTest(path=path.name):
                envelope = json.loads(path.read_text(encoding="utf-8"))
                self.assertIn("envelope", envelope)
                self.assertIn("header", envelope)
                self.assertIn("metadata", envelope)
                self.assertEqual(envelope["header"]["kind"], "sort-morphism")
                self.assertEqual(envelope["header"]["schemaVersion"], "1")

    def test_target_pins(self) -> None:
        """Every morphism must cite the concept-hub-signature CID and the correct concept sort CID."""
        for path in self.all_files:
            with self.subTest(path=path.name):
                envelope = json.loads(path.read_text(encoding="utf-8"))
                header = envelope["header"]
                self.assertEqual(
                    header["target_language_signature_cid"],
                    CONCEPT_HUB_SIG_CID,
                    "target_language_signature_cid must be the concept-hub-signature CID",
                )
                m = CID_FILENAME_RE.match(path.name)
                expected_target = (
                    CONCEPT_FLOAT_CID if m.group("canonical") == "Float" else CONCEPT_NULL_CID
                )
                self.assertEqual(
                    header["target_sort_cid"],
                    expected_target,
                    f"target_sort_cid must be concept:{m.group('canonical')} CID",
                )

    def test_narrowing_morphisms_carry_runtime_guards(self) -> None:
        """left-to-right + range_loss=narrowing morphisms MUST declare runtime_guards."""
        for path in self.all_files:
            with self.subTest(path=path.name):
                envelope = json.loads(path.read_text(encoding="utf-8"))
                header = envelope["header"]
                if (
                    header["direction"] == "left-to-right"
                    and header["range_loss"] == "narrowing"
                ):
                    self.assertGreaterEqual(
                        len(header["runtime_guards"]),
                        1,
                        f"narrowing morphism {path.name} must declare runtime_guards",
                    )

    def test_cid_recompute(self) -> None:
        """Each morphism's header.cid must match JCS+blake3-512 of {header, metadata} with cid elided."""
        from blake3 import blake3

        for path in self.all_files:
            with self.subTest(path=path.name):
                envelope = json.loads(path.read_text(encoding="utf-8"))
                header = envelope["header"]
                pinned_cid = header["cid"]
                cid_input = {
                    "header": {k: v for k, v in header.items() if k != "cid"},
                    "metadata": envelope["metadata"],
                }
                canonical = json.dumps(
                    cid_input, sort_keys=True, separators=(",", ":"), ensure_ascii=False
                ).encode("utf-8")
                recomputed = "blake3-512:" + blake3(canonical).digest(length=64).hex()
                self.assertEqual(
                    pinned_cid,
                    recomputed,
                    f"header.cid does not match recomputed JCS+blake3-512 for {path.name}",
                )

    def test_filename_cid_matches_header_cid(self) -> None:
        for path in self.all_files:
            with self.subTest(path=path.name):
                m = CID_FILENAME_RE.match(path.name)
                self.assertIsNotNone(m)
                envelope = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(
                    m.group("cid"),
                    envelope["header"]["cid"],
                    f"filename CID does not match envelope.header.cid for {path.name}",
                )

    def test_cids_tsv_pins(self) -> None:
        """Each morphism must have a cids.tsv row pinning its CID."""
        tsv_pins: dict[str, str] = {}
        for line in CIDS_TSV.read_text(encoding="utf-8").splitlines():
            parts = line.split("\t")
            if len(parts) >= 3 and parts[0] == "sort-morphism":
                tsv_pins[parts[1]] = parts[2]

        for path in self.all_files:
            m = CID_FILENAME_RE.match(path.name)
            assert m is not None
            lang = m.group("lang")
            sort = m.group("sort")
            canonical = m.group("canonical").lower()
            with self.subTest(path=path.name):
                envelope = json.loads(path.read_text(encoding="utf-8"))
                expected_pin_name = (
                    f"sort_morphism_{lang}_{sort}_to_concept_{canonical}"
                )
                self.assertIn(
                    expected_pin_name,
                    tsv_pins,
                    f"cids.tsv missing pin {expected_pin_name}",
                )
                self.assertEqual(
                    tsv_pins[expected_pin_name],
                    envelope["header"]["cid"],
                    f"cids.tsv CID mismatch for {expected_pin_name}",
                )

    def test_mint_script_deterministic(self) -> None:
        """Re-running the mint script must produce byte-identical morphism files."""
        before = {p: p.read_bytes() for p in self.all_files}
        result = subprocess.run(
            [sys.executable, str(MINT_SCRIPT)], capture_output=True, check=False
        )
        self.assertEqual(
            result.returncode,
            0,
            f"mint script failed: stdout={result.stdout!r} stderr={result.stderr!r}",
        )
        for p, before_bytes in before.items():
            after_bytes = p.read_bytes()
            self.assertEqual(
                before_bytes,
                after_bytes,
                f"mint script not deterministic for {p.name}",
            )

    def test_morphism_count(self) -> None:
        """Total morphism count must match the expected 8 Float + 7 Null = 15."""
        self.assertEqual(
            len(self.float_files),
            8,
            f"expected 8 Float morphisms; got {len(self.float_files)}",
        )
        self.assertEqual(
            len(self.null_files),
            7,
            f"expected 7 Null morphisms; got {len(self.null_files)}",
        )


if __name__ == "__main__":
    unittest.main()
