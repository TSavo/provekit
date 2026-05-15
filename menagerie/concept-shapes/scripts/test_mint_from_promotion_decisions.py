import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mint_from_promotion_decisions as mint


def cid(fill: str) -> str:
    return "blake3-512:" + (fill * 128)


def ctor(name: str) -> dict:
    return {"args": [], "kind": "ctor", "name": name}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


class MintFromPromotionDecisionsTests(unittest.TestCase):
    def test_admitted_promotion_mints_op_spec_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bind = root / "bind"
            decisions_dir = bind / "promotion-decisions"
            contracts_dir = bind / "contracts"
            specs_dir = root / "specs"
            cids_tsv = root / "cids.tsv"
            decisions_dir.mkdir(parents=True)
            contracts_dir.mkdir(parents=True)
            cids_tsv.write_text("kind\tname\tcid\tpath\n", encoding="utf-8")

            contract_cid = cid("a")
            post = {
                "arity": ["Sql", "SqlArgs"],
                "arity_shape": {
                    "kind": "named",
                    "slots": [{"name": "sql"}, {"name": "args"}],
                },
                "kind": "operation-contract",
                "operator": "auto-query",
                "result": "SqlRowSet",
                "wp_note": "returns rows for the query",
            }
            contract = {
                "aggregation_strategy": "conjunction",
                "cid": contract_cid,
                "composed_post": post,
                "composed_pre": true_formula(),
                "effects": {"effects": [{"kind": "effect-signature", "name": "IO"}]},
                "evidences": [],
                "formal_sorts": [ctor("Sql"), ctor("SqlArgs")],
                "formals": ["sql", "args"],
                "function_term_cid": cid("b"),
                "kind": "compound-contract",
                "post": post,
                "pre": true_formula(),
                "return_sort": ctor("SqlRowSet"),
                "schemaVersion": "1",
            }
            (contracts_dir / f"{contract_cid}.json").write_text(
                json.dumps(contract, sort_keys=True), encoding="utf-8"
            )

            reason = "witness consensus admitted concept:auto-query"
            decision = {
                "envelope": {
                    "declaredAt": "2026-05-13T00:00:00.000Z",
                    "signature": "",
                    "signer": cid("c"),
                },
                "header": {
                    "candidate_cid": cid("d"),
                    "cid": cid("e"),
                    "decider_cid": cid("f"),
                    "decision_payload": {
                        "promoted_op": "concept:auto-query",
                        "reason": reason,
                        "result": "admitted",
                    },
                    "evidence_cids": [contract_cid],
                    "gate": "threshold",
                    "kind": "promotion-decision",
                    "policy_cid": cid("1"),
                    "promoted_cid": cid("2"),
                    "result": "admitted",
                    "schemaVersion": "1",
                },
                "metadata": {},
            }
            (decisions_dir / "decision.json").write_text(
                json.dumps(decision, sort_keys=True), encoding="utf-8"
            )

            def fake_mint(kind: str, spec_name: str) -> tuple[str, str]:
                self.assertEqual(kind, "algorithm")
                self.assertEqual(spec_name, "op_auto_query.spec.json")
                return (
                    cid("9"),
                    f"menagerie/concept-shapes/catalog/algorithms/{spec_name}.json",
                )

            with (
                patch.object(mint, "SPEC_DIR", specs_dir),
                patch.object(mint, "CID_FILE", cids_tsv),
                patch.object(mint.discharge, "mint", side_effect=fake_mint),
                patch.object(mint.discharge, "scan_created_text"),
            ):
                summary = mint.main([str(bind)])
                spec_path = specs_dir / "op_auto_query.spec.json"
                self.assertEqual(summary.admitted_seen, 1)
                self.assertEqual(summary.written, 1)
                self.assertEqual(summary.skipped_existing, 0)

                spec = json.loads(spec_path.read_text(encoding="utf-8"))
                self.assertEqual(spec["kind"], "algorithm")
                self.assertEqual(spec["fn_name"], "concept:auto-query")
                self.assertEqual(spec["formals"], ["sql", "args"])
                self.assertEqual(spec["formal_sorts"], [ctor("Sql"), ctor("SqlArgs")])
                self.assertEqual(spec["return_sort"], ctor("SqlRowSet"))
                self.assertEqual(spec["pre"], true_formula())
                self.assertEqual(spec["post"], post)
                self.assertEqual(spec["effects"], {"effects": [{"kind": "effect-signature", "name": "IO"}]})
                self.assertEqual(spec["locus"], reason)

                on_disk = spec_path.read_text(encoding="utf-8")
                self.assertEqual(
                    on_disk,
                    json.dumps(spec, sort_keys=True, indent=2, ensure_ascii=True) + "\n",
                )

                first_snapshot = {
                    path.relative_to(root): path.read_text(encoding="utf-8")
                    for path in root.rglob("*")
                    if path.is_file()
                }
                second = mint.main([str(bind)])
                second_snapshot = {
                    path.relative_to(root): path.read_text(encoding="utf-8")
                    for path in root.rglob("*")
                    if path.is_file()
                }
                self.assertEqual(second.admitted_seen, 1)
                self.assertEqual(second.written, 0)
                self.assertEqual(second.skipped_existing, 1)
                self.assertEqual(second_snapshot, first_snapshot)


if __name__ == "__main__":
    unittest.main()
