#!/usr/bin/env python3
import copy
import json
from pathlib import Path

import discharge

BASE = discharge.BASE
ROOT = discharge.ROOT
SPEC_DIR = discharge.SPEC_DIR
RECEIPT_DIR = discharge.RECEIPT_DIR
DISCHARGE_DIR = discharge.DISCHARGE_DIR
CATALOG_REAL = discharge.CATALOG_REAL
CATALOG_ARG = discharge.CATALOG_ARG
CID_FILE = discharge.CID_FILE
PROVEKIT = discharge.PROVEKIT

C11_COMPONENTS = ROOT / "menagerie" / "c11-language-signature" / "component-cids.json"
RUST_COMPONENTS = ROOT / "menagerie" / "rust-language-signature" / "component-cids.json"
C11_SPECS = ROOT / "menagerie" / "c11-language-signature" / "specs"
RUST_SPECS = ROOT / "menagerie" / "rust-language-signature" / "specs"

OPS = [
    {
        "slug": "conditional",
        "concept_fn": "concept:conditional",
        "concept_operator": "conditional",
        "sources": {
            "c11": {"spec": "op_if.spec.json", "source_name": "c11:if", "operator_map": {"if": "conditional"}},
            "rust": {"spec": "op_if.spec.json", "source_name": "rust:if", "operator_map": {"if": "conditional"}},
        },
    },
    {
        "slug": "seq",
        "concept_fn": "concept:seq",
        "concept_operator": "seq",
        "sources": {
            "c11": {"spec": "op_seq.spec.json", "source_name": "c11:seq", "operator_map": {"seq": "seq"}},
            "rust": {"spec": "op_seq.spec.json", "source_name": "rust:seq", "operator_map": {"seq": "seq"}},
        },
    },
    {
        "slug": "return",
        "concept_fn": "concept:return",
        "concept_operator": "return",
        "sources": {
            "c11": {"spec": "op_return.spec.json", "source_name": "c11:return", "operator_map": {"return": "return"}},
            "rust": {"spec": "op_return.spec.json", "source_name": "rust:return", "operator_map": {"return": "return"}},
        },
    },
    {
        "slug": "eq",
        "concept_fn": "concept:eq",
        "concept_operator": "eq",
        "sources": {
            "c11": {"spec": "op_eq.spec.json", "source_name": "c11:eq", "operator_map": {"eq": "eq"}},
            "rust": {"spec": "op_eq.spec.json", "source_name": "rust:eq", "operator_map": {"eq": "eq"}},
        },
    },
    {
        "slug": "skip",
        "concept_fn": "concept:skip",
        "concept_operator": "skip",
        "sources": {
            "c11": {"spec": "op_skip.spec.json", "source_name": "c11:skip", "operator_map": {"skip": "skip"}},
            "rust": {"spec": "op_skip.spec.json", "source_name": "rust:skip", "operator_map": {"skip": "skip"}},
        },
    },
]


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, value):
    discharge.write_json(path, value)


def component_cid(component_file, spec_name):
    for row in read_json(component_file):
        if row.get("kind") == "algorithm" and row.get("spec") == spec_name:
            return row["cid"]
    raise SystemExit(f"missing component cid for {spec_name} in {component_file}")


def source_dir(language):
    if language == "c11":
        return C11_SPECS
    if language == "rust":
        return RUST_SPECS
    raise AssertionError(language)


def component_file(language):
    if language == "c11":
        return C11_COMPONENTS
    if language == "rust":
        return RUST_COMPONENTS
    raise AssertionError(language)


def concept_spec_from_source(source_spec, concept_fn, concept_operator):
    data = copy.deepcopy(source_spec)
    data.pop("locus", None)
    data["fn_name"] = concept_fn
    data["post"]["operator"] = concept_operator
    return data


def transformed_source_spec(source_spec, concept_fn, operator_map):
    data = copy.deepcopy(source_spec)
    data.pop("locus", None)
    data["fn_name"] = concept_fn
    post = data.get("post", {})
    operator = post.get("operator")
    if operator in operator_map:
        post["operator"] = operator_map[operator]
    return data


def mint(kind, spec_name):
    return discharge.mint(kind, spec_name)


def algorithm_payload(spec):
    payload = {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": spec.get("fn_name"),
        "formals": spec.get("formals", []),
        "formal_sorts": spec.get("formal_sorts", []),
        "pre": spec.get("pre", {"kind": "atomic", "name": "true", "args": []}),
        "post": spec["post"],
        "effects": spec.get("effects", {"effects": []}),
        "auto_minted_mementos": [],
        "return_sort": spec.get("return_sort", {"kind": "primitive", "name": "Bool"}),
    }
    if "locus" in spec:
        payload["locus"] = spec["locus"]
    if "body_cid" in spec:
        payload["body_cid"] = spec["body_cid"]
    if "input_cids" in spec:
        payload["input_cids"] = spec["input_cids"]
    if "refines" in spec:
        payload["refines"] = spec["refines"]
    return payload


def mint_for_cid(spec):
    return discharge.canonical_cid_value(algorithm_payload(spec))


def morphism_spec(language, source_name, source_cid, concept_fn, shape_cid, renaming, operator_map):
    morphism_name = f"morphism:{source_name}:to:{concept_fn}"
    return {
        "kind": "algorithm",
        "fn_name": morphism_name,
        "formals": ["source_contract"],
        "formal_sorts": [{"kind": "ctor", "name": "FunctionContractMemento", "args": []}],
        "return_sort": {"kind": "ctor", "name": "FunctionContractMemento", "args": []},
        "pre": {"kind": "atomic", "name": "true", "args": []},
        "post": {
            "kind": "contract-renaming-morphism",
            "source_contract_cid": source_cid,
            "target_shape_cid": shape_cid,
            "renaming_map": renaming,
            "representation_map": {},
            "operator_map": operator_map,
            "literal_map": {},
            "homomorphism_obligation": {
                "kind": "canonicalizer-alpha-equivalence-plus-representation-map",
                "source": source_cid,
                "target": shape_cid,
            },
        },
        "effects": {"effects": []},
        "input_cids": [source_cid, shape_cid],
    }


def append_cids(rows):
    with CID_FILE.open("a", encoding="utf-8") as handle:
        for row in rows:
            handle.write(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}\n")


def update_readme(records):
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")
    rows = []
    for record in records:
        morphisms = ", ".join(item["name"] for item in record["morphisms"])
        rows.append(
            f"| `{record['concept']}` | `{record['shape_cid']}` | C11, Rust | {morphisms} |"
        )
    row_block = "\n".join(rows)
    marker = "| `allocate-or-bail` |"
    if marker in text and "| `concept:conditional` |" not in text:
        text = text.replace(marker, row_block + "\n" + marker, 1)

    section_lines = [
        "## Primitive Operation Hubs",
        "",
        "These are primitive operation concept nodes, not idiom shapes. They are the minimal hub used by `provekit transport` for the C-to-Rust `foo` path.",
        "",
    ]
    for record in records:
        section_lines.append(f"### {record['concept']}")
        section_lines.append("")
        section_lines.append(f"- Shape: `{record['shape_cid']}`")
        for morphism in record["morphisms"]:
            section_lines.append(f"- {morphism['label']} morphism: `{morphism['morphism_cid']}`")
            section_lines.append(f"- {morphism['label']} receipt: `{morphism['receipt_cid']}`")
        section_lines.append("")
    section = "\n".join(section_lines)
    if "## Primitive Operation Hubs" not in text:
        text = text.replace("## Concept Details", section + "\n## Concept Details", 1)
    readme.write_text(text, encoding="utf-8")


def main():
    discharge.build_tools()
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    RECEIPT_DIR.mkdir(parents=True, exist_ok=True)
    DISCHARGE_DIR.mkdir(parents=True, exist_ok=True)
    CATALOG_REAL.mkdir(parents=True, exist_ok=True)

    cids = []
    records = []
    for op in OPS:
        c11_spec_name = op["sources"]["c11"]["spec"]
        source_spec = read_json(C11_SPECS / c11_spec_name)
        concept_spec = concept_spec_from_source(source_spec, op["concept_fn"], op["concept_operator"])
        spec_name = f"{op['slug']}_shape.spec.json"
        write_json(SPEC_DIR / spec_name, concept_spec)
        shape_cid, shape_path = mint("algorithm", spec_name)
        cids.append({"kind": "shape", "name": op["concept_fn"], "cid": shape_cid, "path": shape_path})

        record = {"concept": op["concept_fn"], "shape_cid": shape_cid, "morphisms": []}
        for language in ("c11", "rust"):
            source = op["sources"][language]
            source_spec = read_json(source_dir(language) / source["spec"])
            source_cid = component_cid(component_file(language), source["spec"])
            after_spec = transformed_source_spec(source_spec, op["concept_fn"], source["operator_map"])
            after_cid = mint_for_cid(after_spec)
            if after_cid != shape_cid:
                raise SystemExit(
                    f"{source['source_name']} discharge landed on {after_cid}, not {shape_cid}"
                )
            after_name = f"{language}_{op['slug']}_after_substitution.json"
            write_json(DISCHARGE_DIR / after_name, after_spec)

            renaming = {name: name for name in source_spec.get("formals", [])}
            spec_stem = f"morphism_{language}_{source['source_name'].split(':', 1)[1]}_to_{op['slug']}"
            spec_name = f"{spec_stem}.spec.json"
            m_spec = morphism_spec(
                language,
                source["source_name"],
                source_cid,
                op["concept_fn"],
                shape_cid,
                renaming,
                source["operator_map"],
            )
            write_json(SPEC_DIR / spec_name, m_spec)
            morphism_cid, morphism_path = mint("algorithm", spec_name)
            cids.append({"kind": "morphism", "name": spec_stem, "cid": morphism_cid, "path": morphism_path})

            receipt = {
                "schema_version": "1",
                "kind": "MorphismDischargeReceipt",
                "morphism_cid": morphism_cid,
                "source_contract_cid": source_cid,
                "renaming_map": renaming,
                "representation_map": {},
                "operator_map": source["operator_map"],
                "literal_map": {},
                "after_substitution_cid": after_cid,
                "shape_cid": shape_cid,
                "discharged": True,
                "method": "canonicalizer-alpha-equivalence-plus-representation-map",
            }
            receipt_cid, receipt_path = discharge.store_receipt(spec_stem, receipt)
            cids.append({"kind": "receipt", "name": spec_stem, "cid": receipt_cid, "path": receipt_path})
            record["morphisms"].append(
                {
                    "label": "C11" if language == "c11" else "Rust",
                    "name": spec_stem,
                    "morphism_cid": morphism_cid,
                    "receipt_cid": receipt_cid,
                }
            )
        records.append(record)

    append_cids(cids)
    update_readme(records)
    discharge.scan_created_text()
    for record in records:
        print(f"primitive_shape_cid\t{record['concept']}\t{record['shape_cid']}")


if __name__ == "__main__":
    main()
