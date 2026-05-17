"""BoundaryContractMemento catalog seed tests."""

from __future__ import annotations

import json
import shutil
import subprocess
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / "menagerie" / "concept-shapes"
CONTRACT_DIR = BASE / "boundary-contracts"
SPEC_PATH = BASE / "specs" / "boundary-contract_shape.spec.json"
INDEX_PATH = BASE / "catalog" / "index.json"

EXPECTED_CONTRACTS = {
    "boundary:http-1.1": {
        "effects": ["NetworkRequest"],
        "version": "1.1",
    },
    "boundary:http-2": {
        "effects": ["NetworkRequest", "Streaming"],
        "version": "2",
    },
    "boundary:sql-92": {
        "effects": ["DatabaseIO"],
        "version": "92",
    },
    "boundary:sql-postgres-dialect": {
        "effects": ["DatabaseIO"],
        "version": "postgres",
    },
    "boundary:sql-sqlite-dialect": {
        "effects": ["DatabaseIO"],
        "version": "sqlite",
    },
}


def _jcs_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def _blake3_512(data: bytes) -> str:
    try:
        import blake3  # type: ignore

        return f"blake3-512:{blake3.blake3(data).digest(length=64).hex()}"
    except ModuleNotFoundError:
        b3sum = shutil.which("b3sum")
        if b3sum is None:
            raise RuntimeError("BLAKE3 unavailable: install python blake3 or provide b3sum")
        proc = subprocess.run(
            [b3sum, "--length", "64"],
            input=data,
            check=True,
            capture_output=True,
        )
        return f"blake3-512:{proc.stdout.decode('utf-8').split()[0]}"


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


class BoundaryContractCatalogTests(unittest.TestCase):
    def test_shape_spec_declares_pure_boundary_contract_operation(self) -> None:
        spec = _load_json(SPEC_PATH)

        self.assertEqual(SPEC_PATH.read_bytes(), _jcs_bytes(spec))
        self.assertEqual(spec["fn_name"], "concept:boundary-contract")
        self.assertEqual(spec["kind"], "algorithm")
        self.assertEqual(spec["effects"], {"effects": []})
        self.assertEqual(spec["loss_dimensions"], [])
        self.assertEqual(spec["pre"], {"args": [], "kind": "atomic", "name": "true"})
        self.assertEqual(spec["formals"], ["contract_id", "version", "operations"])
        self.assertEqual(
            spec["post"],
            {
                "arity": ["BoundaryContractId", "Version", "List<OperationSpec>"],
                "arity_shape": {
                    "kind": "named",
                    "slots": [
                        {"name": "contract_id"},
                        {"name": "version"},
                        {"name": "operations"},
                    ],
                },
                "kind": "operation-contract",
                "operator": "boundary-contract",
                "result": "BoundaryContractMemento",
                "wp_note": "Declares the exact operation surface exposed by a boundary contract. Realization-specific loss belongs on BoundaryRealizationMemento.",
            },
        )

    def test_seed_mementos_parse_and_have_matching_content_cids(self) -> None:
        paths = sorted(CONTRACT_DIR.glob("boundary:*.blake3-512:*.json"))
        self.assertEqual(len(paths), len(EXPECTED_CONTRACTS))

        seen = set()
        for path in paths:
            memento = _load_json(path)
            self.assertEqual(path.read_bytes(), _jcs_bytes(memento))
            self.assertEqual(memento["metadata"], {"schemaVersion": "provekit-boundary-contract/v1"})

            content = memento["header"]["content"]
            cid = memento["header"]["cid"]
            name = content["name"]
            seen.add(name)

            self.assertEqual(path.name, f"{name}.{cid}.json")
            self.assertEqual(
                _blake3_512(_jcs_bytes({"content": content, "metadata": memento["metadata"]})),
                cid,
            )
            self.assertEqual(content["version"], EXPECTED_CONTRACTS[name]["version"])
            self.assertIsInstance(content["semantic_notes"], str)
            self.assertTrue(content["semantic_notes"])

            operations = content["operations"]
            self.assertGreaterEqual(len(operations), 1)
            self.assertEqual([operation["name"] for operation in operations], sorted(operation["name"] for operation in operations))
            for operation in operations:
                self.assertIsInstance(operation["arity"], list)
                self.assertTrue(operation["arity"])
                self.assertEqual(operation["effects"], EXPECTED_CONTRACTS[name]["effects"])
                self.assertIsInstance(operation["notes"], str)
                self.assertTrue(operation["notes"])
                self.assertIsInstance(operation["return_sort"], str)

        self.assertEqual(seen, set(EXPECTED_CONTRACTS))

    def test_catalog_index_registers_boundary_contracts_sorted_by_cid(self) -> None:
        index = _load_json(INDEX_PATH)
        entries = index["entries"]
        self.assertEqual(list(entries), sorted(entries))

        by_name = {entry["name"]: entry for entry in entries.values() if entry["kind"] == "boundary-contract"}
        self.assertEqual(set(by_name), set(EXPECTED_CONTRACTS))

        for name, entry in by_name.items():
            cid = entry["cid"]
            self.assertEqual(entry["path"], f"boundary-contracts/{name}.{cid}.json")
            self.assertEqual(entries[cid], entry)
            path = BASE / entry["path"]
            self.assertTrue(path.exists())
            self.assertEqual(_load_json(path)["header"]["cid"], cid)


if __name__ == "__main__":
    unittest.main()
