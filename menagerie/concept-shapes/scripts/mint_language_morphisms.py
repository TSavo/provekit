#!/usr/bin/env python3
"""Mint common-imperative concept shapes and discharged language morphisms.

The generator builds the concept hub from existing operation contracts, then
attempts to mint a refinement morphism from each real lifter-emitted op to the
corresponding concept hub op.  Two discharge strategies are attempted in order:

1. canonicalizer-alpha-equivalence-plus-representation-map: the canonicalizer
   image of the transformed source contract lands exactly on the concept shape CID.

2. Structural ⊑ discharge (sound conservative widening, no SMT required):
   a. wp-text abstraction: if specs differ only in post.wp (documentation prose),
      the morphism is discharged.  wp carries no semantic weight in the obligation.
   b. pre-weakening: if lang_pre = true (trivially weak) and specs differ only in
      {pre, post.wp}, the morphism is discharged.  Soundness: concept_pre → true
      is a tautology, so in every calling context where concept:op is valid, the
      lang:op (which accepts all inputs) is also valid.

Ops that do not discharge under either strategy are recorded in transport-gaps.md
with the structural reason for the mismatch.
"""
import copy
import json
import re
import sys
from pathlib import Path

import discharge

BASE = discharge.BASE
ROOT = discharge.ROOT
SPEC_DIR = discharge.SPEC_DIR
RECEIPT_DIR = discharge.RECEIPT_DIR
DISCHARGE_DIR = discharge.DISCHARGE_DIR
CATALOG_REAL = discharge.CATALOG_REAL
CID_FILE = discharge.CID_FILE

LANGUAGES = [
    {"id": "c11", "prefix": "c11", "dir": "c11-language-signature"},
    {"id": "csharp", "prefix": "csharp", "dir": "csharp-language-signature"},
    {"id": "go", "prefix": "go", "dir": "go-language-signature"},
    {"id": "python", "prefix": "python", "dir": "python-language-signature"},
    {"id": "typescript", "prefix": "ts", "dir": "typescript-language-signature"},
    {"id": "zig", "prefix": "zig", "dir": "zig-language-signature"},
    {"id": "ruby", "prefix": "ruby", "dir": "ruby-language-signature"},
    {"id": "php", "prefix": "php", "dir": "php-language-signature"},
    {"id": "java", "prefix": "java", "dir": "java-language-signature"},
    {"id": "rust", "prefix": "rust", "dir": "rust-language-signature"},
]
LANG_BY_ID = {item["id"]: item for item in LANGUAGES}

PRIMITIVE_STEMS = {
    "morphism_c11_if_to_conditional",
    "morphism_rust_if_to_conditional",
    "morphism_c11_seq_to_seq",
    "morphism_rust_seq_to_seq",
    "morphism_c11_return_to_return",
    "morphism_rust_return_to_return",
    "morphism_c11_eq_to_eq",
    "morphism_rust_eq_to_eq",
    "morphism_c11_skip_to_skip",
    "morphism_rust_skip_to_skip",
}

COMMON_ALIASES = {
    "conditional": {lang: ["op_if.spec.json"] for lang in ["c11", "csharp", "go", "python", "typescript", "zig", "ruby", "php", "rust", "java"]},
    "ite": {"c11": ["op_conditional.spec.json"], "csharp": ["op_ite.spec.json"], "python": ["op_ite-bool.spec.json"], "typescript": ["op_ite.spec.json"], "php": ["op_ternary.spec.json"], "ruby": ["op_ternary.spec.json"], "rust": ["op_ite.spec.json"], "java": ["op_ternary.spec.json"]},
    "mod": {"rust": ["op_rem.spec.json"]},
    "bitand": {"c11": ["op_bit_and.spec.json"], "rust": ["op_bit_and.spec.json"]},
    "bitor": {"c11": ["op_bit_or.spec.json"], "rust": ["op_bit_or.spec.json"]},
    "bitxor": {"c11": ["op_bit_xor.spec.json"], "rust": ["op_bit_xor.spec.json"]},
    "bitnot": {"c11": ["op_bit_not.spec.json"], "rust": ["op_bit_not.spec.json"]},
    "addr": {"c11": ["op_addr_of.spec.json"], "go": ["op_addr.spec.json"], "zig": ["op_addr.spec.json"], "rust": ["op_borrow.spec.json"]},
    "index": {"c11": ["op_array_subscript.spec.json"], "python": ["op_subscript.spec.json"]},
    "member": {"rust": ["op_field.spec.json", "op_member.spec.json"], "zig": ["op_field.spec.json"], "php": ["op_propfetch.spec.json"]},
    "decl": {"rust": ["op_let.spec.json"]},
    "do": {"php": ["op_dowhile.spec.json"]},
    "throw": {"rust": ["op_panic.spec.json"], "python": ["op_raise.spec.json"], "ruby": ["op_raise.spec.json"], "zig": ["op_panic.spec.json"]},
    "source-unit": {"c11": ["op_source_unit.spec.json"], "python": ["op_source-unit.spec.json"], "csharp": ["op_source-unit.spec.json"], "typescript": ["op_source-unit.spec.json"], "zig": ["op_source-unit.spec.json"], "ruby": ["op_source-unit.spec.json"], "php": ["op_source-unit.spec.json"], "go": ["op_source_unit.spec.json"], "rust": ["op_source-unit.spec.json"], "java": ["op_source-unit.spec.json"]},
    "postinc": {"c11": ["op_post_inc.spec.json"], "typescript": ["op_postinc.spec.json"], "csharp": ["op_postinc.spec.json"]},
    "postdec": {"c11": ["op_post_dec.spec.json"], "typescript": ["op_postdec.spec.json"], "csharp": ["op_postdec.spec.json"]},
    "preinc": {"c11": ["op_pre_inc.spec.json"], "typescript": ["op_preinc.spec.json"], "csharp": ["op_preinc.spec.json"]},
    "predec": {"c11": ["op_pre_dec.spec.json"], "typescript": ["op_predec.spec.json"], "csharp": ["op_predec.spec.json"]},
}


def op(slug, base, concept_operator=None, base_operator=None, renaming=None, aliases=None, patches=None, notes=""):
    return {
        "slug": slug,
        "concept_fn": f"concept:{slug}",
        "concept_operator": concept_operator or slug,
        "base": base,
        "base_operator": base_operator,
        "renaming": renaming or {},
        "aliases": aliases or {},
        "patches": patches or {},
        "notes": notes,
    }


OPS = [
    op("add", ("c11", "op_add.spec.json")),
    op("sub", ("c11", "op_sub.spec.json")),
    op("mul", ("c11", "op_mul.spec.json")),
    op("div", ("c11", "op_div.spec.json"), notes="integer division only; floating division is out of scope"),
    op("mod", ("c11", "op_mod.spec.json")),
    op("neg", ("c11", "op_neg.spec.json")),
    op("bitand", ("c11", "op_bit_and.spec.json"), base_operator="bit_and"),
    op("bitor", ("c11", "op_bit_or.spec.json"), base_operator="bit_or"),
    op("bitxor", ("c11", "op_bit_xor.spec.json"), base_operator="bit_xor"),
    op("bitnot", ("c11", "op_bit_not.spec.json"), base_operator="bit_not"),
    op("shl", ("c11", "op_shl.spec.json")),
    op("shr", ("c11", "op_shr.spec.json"), notes="arithmetic right shift on the restricted Int core"),
    op("ushr", ("typescript", "op_ushr.spec.json"), notes="logical zero-fill right shift"),
    op("eq", ("c11", "op_eq.spec.json")),
    op("ne", ("c11", "op_ne.spec.json")),
    op("lt", ("c11", "op_lt.spec.json")),
    op("le", ("c11", "op_le.spec.json")),
    op("gt", ("c11", "op_gt.spec.json")),
    op("ge", ("c11", "op_ge.spec.json")),
    op("and", ("c11", "op_and.spec.json"), patches={"unevaluated_slots": ["right"]}),
    op("or", ("c11", "op_or.spec.json"), patches={"unevaluated_slots": ["right"]}),
    op("not", ("c11", "op_not.spec.json")),
    op("assign", ("c11", "op_assign.spec.json"), renaming={"lvalue": "target", "rvalue": "value"}),
    op("decl", ("c11", "op_decl.spec.json")),
    op("seq", ("c11", "op_seq.spec.json")),
    op("skip", ("c11", "op_skip.spec.json")),
    op("conditional", ("c11", "op_if.spec.json"), concept_operator="conditional", base_operator="if"),
    op("ite", ("c11", "op_conditional.spec.json"), base_operator="conditional", patches={"unevaluated_slots": ["then_expr", "else_expr"]}),
    op("while", ("c11", "op_while.spec.json")),
    op("do", ("c11", "op_do.spec.json")),
    op("for", ("c11", "op_for.spec.json")),
    op("foreach", ("csharp", "op_foreach.spec.json"), patches={"arity_shape": {"kind": "named", "slots": [{"name": "var_name", "slot_sort": "identifier"}, {"name": "collection"}, {"name": "body", "evaluation": "unevaluated"}]}}),
    op("break", ("c11", "op_break.spec.json")),
    op("continue", ("c11", "op_continue.spec.json")),
    op("return", ("c11", "op_return.spec.json")),
    op("call", ("c11", "op_call.spec.json")),
    op("index", ("c11", "op_array_subscript.spec.json"), base_operator="array-subscript"),
    op("member", ("c11", "op_member.spec.json")),
    op("deref", ("c11", "op_deref.spec.json")),
    op("addr", ("c11", "op_addr_of.spec.json"), base_operator="addr_of"),
    op("new", ("csharp", "op_new.spec.json")),
    op("cast", ("c11", "op_cast.spec.json")),
    op("throw", ("php", "op_throw.spec.json")),
    op("postinc", ("c11", "op_post_inc.spec.json"), base_operator="post_inc"),
    op("postdec", ("c11", "op_post_dec.spec.json"), base_operator="post_dec"),
    op("preinc", ("c11", "op_pre_inc.spec.json"), base_operator="pre_inc"),
    op("predec", ("c11", "op_pre_dec.spec.json"), base_operator="pre_dec"),
    op("source-unit", ("c11", "op_source_unit.spec.json"), concept_operator="source-unit"),
]

PORTABLE_ALL = {
    "add", "sub", "mul", "mod", "neg", "bitand", "bitor", "bitxor", "bitnot", "shl", "shr",
    "eq", "ne", "lt", "le", "gt", "ge", "and", "or", "not", "assign", "decl", "seq", "skip",
    "conditional", "ite", "while", "for", "break", "continue", "return", "call", "index", "member",
    "cast", "postinc", "postdec", "preinc", "predec", "source-unit",
}
LANG_SUPPORTED = {
    "c11": PORTABLE_ALL | {"div", "do", "deref", "addr"},
    "csharp": PORTABLE_ALL | {"div", "do", "foreach", "new", "throw", "ushr"},
    "go": (PORTABLE_ALL | {"div", "deref", "addr", "new"}) - {"postinc", "postdec", "preinc", "predec", "ite"},
    "python": (PORTABLE_ALL | {"div", "foreach", "throw"}) - {"div", "bitnot", "shl", "shr", "postinc", "postdec", "preinc", "predec", "cast"},
    "typescript": (PORTABLE_ALL | {"foreach", "new", "throw", "ushr"}) - {"div", "deref", "addr"},
    "zig": PORTABLE_ALL | {"div", "do", "foreach", "deref", "addr", "new", "throw"},
    "ruby": (PORTABLE_ALL | {"foreach", "throw"}) - {"bitnot", "shl", "shr", "postinc", "postdec", "preinc", "predec", "cast"},
    "php": (PORTABLE_ALL | {"foreach", "new", "throw"}) - {"deref", "addr"},
    "java": (PORTABLE_ALL | {"div", "do", "foreach", "new", "throw", "ushr"}) - {"deref", "addr"},
    "rust": PORTABLE_ALL | {"div", "deref", "addr", "new", "throw"},
}

SOURCE_OP_NAME = {
    "conditional": "if",
    "bitand": "bitand",
    "bitor": "bitor",
    "bitxor": "bitxor",
    "bitnot": "bitnot",
    "source-unit": "source-unit",
}


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    discharge.write_json(path, value)


def specs_dir(language_id):
    return ROOT / "menagerie" / LANG_BY_ID[language_id]["dir"] / "specs"


def sanitize(value):
    return re.sub(r"[^A-Za-z0-9]+", "_", value).strip("_")


def primitive(name):
    return {"kind": "primitive", "name": name}


def fn_sort(name):
    return {"kind": "ctor", "name": name, "args": []}


def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def empty_effects():
    return {"effects": []}


def algorithm_payload(spec):
    payload = {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": spec.get("fn_name"),
        "formals": spec.get("formals", []),
        "formal_sorts": spec.get("formal_sorts", []),
        "pre": spec.get("pre", true_formula()),
        "post": spec["post"],
        "effects": spec.get("effects", empty_effects()),
        "auto_minted_mementos": [],
        "return_sort": spec.get("return_sort", primitive("Bool")),
    }
    for key in ["locus", "body_cid", "input_cids", "refines"]:
        if key in spec:
            payload[key] = spec[key]
    return payload


def canonical_cid_spec(spec):
    return discharge.canonical_cid_value(algorithm_payload(spec))


def strip_locus(spec):
    data = copy.deepcopy(spec)
    data.pop("locus", None)
    data.pop("version", None)
    return data


def apply_patches(spec, patches):
    if "arity_shape" in patches:
        spec["post"]["arity_shape"] = copy.deepcopy(patches["arity_shape"])
    for name in patches.get("unevaluated_slots", []):
        slots = spec.get("post", {}).get("arity_shape", {}).get("slots", [])
        for slot in slots:
            if slot.get("name") == name:
                slot["evaluation"] = "unevaluated"


def normalize_node(value, renaming, representation, operators, literals):
    return discharge.normalize_node(value, renaming, representation, operators, literals)


def concept_spec_from_base(op_def):
    language_id, spec_name = op_def["base"]
    source = read_json(specs_dir(language_id) / spec_name)
    operators = {}
    if op_def.get("base_operator"):
        operators[op_def["base_operator"]] = op_def["concept_operator"]
    operators[source.get("post", {}).get("operator", "")] = op_def["concept_operator"]
    data = normalize_node(strip_locus(source), op_def["renaming"], {}, operators, {})
    data["fn_name"] = op_def["concept_fn"]
    data["post"]["operator"] = op_def["concept_operator"]
    apply_patches(data, op_def["patches"])
    return data


def source_op_slug(op_def, language_id):
    if op_def["slug"] == "conditional":
        return "if"
    if language_id == "rust" and op_def["slug"] == "mod":
        return "rem"
    if language_id == "c11" and op_def["slug"] == "addr":
        return "addr_of"
    if language_id == "c11" and op_def["slug"] == "index":
        return "array-subscript"
    if language_id == "rust" and op_def["slug"] == "addr":
        return "borrow"
    return SOURCE_OP_NAME.get(op_def["slug"], op_def["slug"])


def source_fn_name(op_def, language):
    return f"{language['prefix']}:{source_op_slug(op_def, language['id'])}"


def source_operator(op_def, language_id):
    return source_op_slug(op_def, language_id)


def spec_candidates(op_def, language):
    out = []
    out.append(f"op_{op_def['slug'].replace('-', '_')}.spec.json")
    for item in COMMON_ALIASES.get(op_def["slug"], {}).get(language["id"], []):
        if item not in out:
            out.append(item)
    for item in op_def["aliases"].get(language["id"], []):
        if item not in out:
            out.append(item)
    return out


def operator_map_for(op_def, source_spec, language):
    source_operator = source_spec.get("post", {}).get("operator")
    concept_operator = op_def["concept_operator"]
    if not source_operator:
        return {}
    out = {}
    prefix = f"{language['prefix']}:"
    if isinstance(source_operator, str) and source_operator.startswith(prefix):
        out[source_operator] = source_operator[len(prefix):]
    if op_def.get("base_operator") and source_operator == op_def["base_operator"]:
        out[source_operator] = concept_operator
    if out.get(source_operator, source_operator) != concept_operator:
        out[source_operator] = concept_operator
    return out


def transformed_source_spec(op_def, source_spec, language):
    operators = operator_map_for(op_def, source_spec, language)
    data = normalize_node(strip_locus(source_spec), op_def["renaming"], {}, operators, {})
    data.pop("transport_core", None)
    data["fn_name"] = op_def["concept_fn"]
    data["post"]["operator"] = op_def["concept_operator"]
    return data, operators


def diff_reason(after, concept):
    for key, label in [("pre", "precondition"), ("formal_sorts", "formal sort"), ("return_sort", "return sort"), ("effects", "effect signature")]:
        if after.get(key) != concept.get(key):
            got = json.dumps(after.get(key), separators=(",", ":"))
            want = json.dumps(concept.get(key), separators=(",", ":"))
            return f"{label} mismatch: got `{got}` want `{want}`"
    if after.get("post") != concept.get("post"):
        post = after.get("post", {})
        target = concept.get("post", {})
        if post.get("wp") != target.get("wp"):
            got = json.dumps(post.get("wp"), separators=(",", ":"))
            want = json.dumps(target.get("wp"), separators=(",", ":"))
            return f"wp mismatch: got `{got}` want `{want}`"
        if post.get("arity_shape") != target.get("arity_shape"):
            got = json.dumps(post.get("arity_shape"), separators=(",", ":"))
            want = json.dumps(target.get("arity_shape"), separators=(",", ":"))
            return f"arity_shape or slot policy mismatch: got `{got}` want `{want}`"
        return "operation-contract mismatch"
    return "canonical payload mismatch"


def _is_true_pre(pre):
    """Return True iff pre is the trivially-true formula {kind:atomic, name:true, args:[]}."""
    return (
        isinstance(pre, dict)
        and pre.get("kind") == "atomic"
        and pre.get("name") == "true"
        and pre.get("args") == []
    )


def try_structural_subsumption(after_spec, concept_spec):
    """Sound structural ⊑ discharge when byte-equality fails on documentation fields only.

    Two relaxations, both sound under the morphism contract:

    1. wp-text abstraction: the wp field is human-readable documentation; it carries
       no semantic weight in the discharge obligation.  If after_spec matches
       concept_spec in every field except post.wp, the morphism is discharged.
       Discharge method: "structural-wp-abstraction".

    2. pre-weakening: if lang_pre = true (the trivially-weak precondition) then
       `concept_pre → lang_pre` holds for any concept_pre, because anything implies
       true.  In every context where concept:op is invoked the concept precondition
       holds, so lang:op (which works for all inputs) can substitute soundly.
       Combined with (1) for wp, if after_spec matches concept_spec modulo {pre, wp},
       and after_spec.pre == true, the morphism is discharged.
       Discharge method: "structural-pre-weakening-and-wp-abstraction".

    Returns (method_string, pre_relaxed, wp_abstracted) on success, or None on failure.
    Sound: false-negatives (remaining gaps) are acceptable; false-positives are not
    emitted because every structural claim has a verified implication.
    """
    import copy as _copy

    after_pre = after_spec.get("pre")
    concept_pre = concept_spec.get("pre")
    after_wp = after_spec.get("post", {}).get("wp")
    concept_wp = concept_spec.get("post", {}).get("wp")

    pre_matches = after_pre == concept_pre
    wp_matches = after_wp == concept_wp

    if pre_matches and wp_matches:
        # Byte-equality should have caught this already; shouldn't reach here.
        return None

    # Try wp-only relaxation first (no pre change needed).
    if pre_matches and not wp_matches:
        relaxed = _copy.deepcopy(after_spec)
        if "post" in relaxed and "wp" in relaxed["post"]:
            relaxed["post"]["wp"] = concept_wp
        elif "post" in relaxed:
            relaxed["post"]["wp"] = concept_wp
        if canonical_cid_spec(relaxed) == canonical_cid_spec(concept_spec):
            return ("structural-wp-abstraction", False, True)
        return None

    # Try pre-weakening (lang pre must be true; also relax wp).
    if not pre_matches and _is_true_pre(after_pre):
        relaxed = _copy.deepcopy(after_spec)
        relaxed["pre"] = concept_pre
        if "post" in relaxed:
            relaxed["post"]["wp"] = concept_wp
        if canonical_cid_spec(relaxed) == canonical_cid_spec(concept_spec):
            return ("structural-pre-weakening-and-wp-abstraction", True, True)
        return None

    # Non-true lang pre that doesn't match concept pre: unsound to relax without SMT.
    return None


def morphism_spec(source_name, source_cid, concept_fn, shape_cid, renaming, operator_map, discharge_method="canonicalizer-alpha-equivalence-plus-representation-map"):
    return {
        "kind": "algorithm",
        "fn_name": f"morphism:{source_name}:to:{concept_fn}",
        "formals": ["source_contract"],
        "formal_sorts": [fn_sort("FunctionContractMemento")],
        "return_sort": fn_sort("FunctionContractMemento"),
        "pre": true_formula(),
        "post": {
            "kind": "contract-renaming-morphism",
            "source_contract_cid": source_cid,
            "target_shape_cid": shape_cid,
            "renaming_map": renaming,
            "representation_map": {},
            "operator_map": operator_map,
            "literal_map": {},
            "homomorphism_obligation": {
                "kind": discharge_method,
                "source": source_cid,
                "target": shape_cid,
            },
        },
        "effects": empty_effects(),
        "input_cids": [source_cid, shape_cid],
    }


def existing_cid_rows():
    out = {}
    if not CID_FILE.exists():
        return out
    for line in CID_FILE.read_text(encoding="utf-8").splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) >= 4:
            out[(parts[0], parts[1])] = {"cid": parts[2], "path": parts[3]}
    return out


def append_cids(rows):
    existing = CID_FILE.read_text(encoding="utf-8").splitlines() if CID_FILE.exists() else ["kind\tname\tcid\tpath"]
    seen = set()
    for line in existing[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            seen.add((parts[0], parts[1]))
    for row in rows:
        key = (row["kind"], row["name"])
        if key in seen:
            continue
        existing.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
        seen.add(key)
    CID_FILE.write_text("\n".join(existing) + "\n", encoding="utf-8")


def write_gap_report(gaps, records):
    lines = [
        "# Program Transport Gaps",
        "",
        "Generated by `scripts/mint_language_morphisms.py`.",
        "Rows here are refusals to mint a morphism because the canonicalizer discharge did not land on the concept shape CID, or because the language has no op spec for that concept node.",
        "Each gap records the structural reason for the mismatch with actual vs. expected values.",
        "",
        "## Semantic Restrictions",
        "",
        "- `concept:div` is integer division only. Floating division is out of scope for this node.",
        "- `concept:ushr` is the logical zero-fill shift. It is separate from arithmetic `concept:shr`.",
        "- Short-circuit `concept:and`, `concept:or`, and `concept:ite` require explicit unevaluated slot policy on the short-circuited argument.",
        "- `concept:source-unit` is a lossless source-bytes plus operational-term wrapper.",
        "",
        "## Minted Coverage", "", "| Concept op | Minted morphisms |", "| --- | --- |"]
    for record in records:
        lines.append(f"| `{record['concept']}` | {', '.join(m['name'] for m in record['morphisms']) or 'none'} |")
    lines += ["", "## Gaps", "", "| Language | Concept op | Source spec | Reason |", "| --- | --- | --- | --- |"]
    for gap in gaps:
        lines.append(f"| `{gap['language']}` | `{gap['concept']}` | `{gap['spec']}` | {gap['reason']} |")
    lines += ["", "T Savo", ""]
    (BASE / "transport-gaps.md").write_text("\n".join(lines), encoding="utf-8")


def update_readme(records):
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8") if readme.exists() else "# Concept Shape Catalog\n"
    section = [
        "## Common Imperative Program Transport Hub",
        "",
        "The `concept:*` operation nodes below are the common-imperative core used by program transport.",
        "They are operation-contract shape mementos, not language-prefixed operations. Per-language morphisms are minted from real lifter-emitted ops by `scripts/mint_language_morphisms.py`; ops that do not discharge are recorded in `transport-gaps.md`.",
        "",
        "| Concept op | Shape CID | Minted morphisms |",
        "| --- | --- | --- |",
    ]
    for record in records:
        section.append(f"| `{record['concept']}` | `{record['shape_cid']}` | {', '.join(m['name'] for m in record['morphisms']) or 'none'} |")
    section += ["", "T Savo", ""]
    marker = "## Common Imperative Program Transport Hub"
    text = (text[:text.index(marker)].rstrip() + "\n\n" if marker in text else text.rstrip() + "\n\n") + "\n".join(section)
    readme.write_text(text, encoding="utf-8")


def main():
    discharge.build_tools()
    for path in [SPEC_DIR, RECEIPT_DIR, DISCHARGE_DIR, CATALOG_REAL]:
        path.mkdir(parents=True, exist_ok=True)

    concept_specs = {op_def["slug"]: concept_spec_from_base(op_def) for op_def in OPS}

    rows, records, gaps = [], [], []
    cid_rows = existing_cid_rows()
    for op_def in OPS:
        concept_spec = concept_specs[op_def["slug"]]
        spec_name = f"{op_def['slug']}_shape.spec.json"
        write_json(SPEC_DIR / spec_name, concept_spec)
        shape_cid, shape_path = discharge.mint("algorithm", spec_name)
        expected = canonical_cid_spec(concept_spec)
        if shape_cid != expected:
            raise SystemExit(f"{op_def['slug']} shape CID mismatch: {shape_cid} != {expected}")
        rows.append({"kind": "shape", "name": op_def["concept_fn"], "cid": shape_cid, "path": shape_path})
        record = {"concept": op_def["concept_fn"], "shape_cid": shape_cid, "morphisms": []}
        for language in LANGUAGES:
            directory = specs_dir(language["id"])
            if not directory.is_dir():
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": f"menagerie/{language['dir']}/specs", "reason": "language signature directory is absent"})
                continue
            found = False
            if op_def["slug"] not in LANG_SUPPORTED.get(language["id"], set()):
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": "not-supported", "reason": "operation not in supported set for this language"})
                continue
            for candidate in spec_candidates(op_def, language):
                path = directory / candidate
                if not path.exists():
                    continue
                found = True
                source_spec = read_json(path)
                source_name = source_spec.get("fn_name", source_fn_name(op_def, language))
                stem = f"morphism_{sanitize(language['id'])}_{sanitize(source_name.split(':', 1)[-1])}_to_{sanitize(op_def['slug'])}"
                if stem in PRIMITIVE_STEMS and (SPEC_DIR / f"{stem}.spec.json").exists() and (RECEIPT_DIR / f"{stem}.receipt.json").exists():
                    morphism_row = cid_rows.get(("morphism", stem), {"cid": "already-minted", "path": f"specs/{stem}.spec.json"})
                    receipt_row = cid_rows.get(("receipt", stem), {"cid": "already-minted", "path": f"receipts/{stem}.receipt.json"})
                    record["morphisms"].append({"language": language["id"], "name": stem, "morphism_cid": morphism_row["cid"], "receipt_cid": receipt_row["cid"]})
                    break
                source_cid = canonical_cid_spec(source_spec)
                after_spec, operator_map = transformed_source_spec(op_def, source_spec, language)
                after_cid = canonical_cid_spec(after_spec)
                if after_cid != shape_cid:
                    subsumption = try_structural_subsumption(after_spec, concept_spec)
                    if subsumption is None:
                        gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": candidate, "reason": diff_reason(after_spec, concept_spec)})
                        continue
                    discharge_method, pre_relaxed, wp_abstracted = subsumption
                else:
                    discharge_method = "canonicalizer-alpha-equivalence-plus-representation-map"
                    pre_relaxed = False
                    wp_abstracted = False
                after_name = f"{sanitize(language['id'])}_{sanitize(source_name.split(':', 1)[-1])}_to_{sanitize(op_def['slug'])}_after_substitution.json"
                write_json(DISCHARGE_DIR / after_name, after_spec)
                m_spec = morphism_spec(source_name, source_cid, op_def["concept_fn"], shape_cid, op_def["renaming"], operator_map, discharge_method)
                write_json(SPEC_DIR / f"{stem}.spec.json", m_spec)
                morphism_cid, morphism_path = discharge.mint("algorithm", f"{stem}.spec.json")
                rows.append({"kind": "morphism", "name": stem, "cid": morphism_cid, "path": morphism_path})
                receipt = {
                    "schema_version": "1",
                    "kind": "MorphismDischargeReceipt",
                    "morphism_cid": morphism_cid,
                    "source_contract_cid": source_cid,
                    "renaming_map": op_def["renaming"],
                    "representation_map": {},
                    "operator_map": operator_map,
                    "literal_map": {},
                    "after_substitution_cid": after_cid,
                    "shape_cid": shape_cid,
                    "discharged": True,
                    "method": discharge_method,
                    "signature": None,
                }
                # Only annotate structural relaxation fields when actually used.
                # Omitting them from byte-equality receipts preserves backward-compatible CIDs.
                if pre_relaxed:
                    receipt["pre_relaxed"] = True
                if wp_abstracted:
                    receipt["wp_abstracted"] = True
                receipt_cid, receipt_path = discharge.store_receipt(stem, receipt)
                rows.append({"kind": "receipt", "name": stem, "cid": receipt_cid, "path": receipt_path})
                record["morphisms"].append({"language": language["id"], "name": stem, "morphism_cid": morphism_cid, "receipt_cid": receipt_cid})
                break
            if not found:
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": f"op_{op_def['slug'].replace('-', '_')}.spec.json", "reason": "no candidate source operation spec"})
        records.append(record)
    append_cids(rows)
    write_gap_report(gaps, records)
    update_readme(records)
    discharge.scan_created_text()
    print(f"concept_op_count\t{len(OPS)}")
    print(f"morphism_count\t{sum(len(r['morphisms']) for r in records)}")
    print(f"gap_count\t{len(gaps)}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        raise
