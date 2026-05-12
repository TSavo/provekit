#!/usr/bin/env python3
"""Mint common-imperative concept shapes and discharged language morphisms.

The generator builds the concept hub from existing operation contracts, then
attempts to mint a refinement morphism from each real lifter-emitted op to the
corresponding concept hub op.  Three discharge strategies are attempted in order:

1. canonicalizer-alpha-equivalence-plus-representation-map: the canonicalizer
   image of the transformed source contract lands exactly on the concept shape CID.

2. Structural ⊑ discharge (sound conservative widening, no SMT required):
   a. wp-text abstraction: if specs differ only in post.wp (documentation prose),
      the morphism is discharged.  wp carries no semantic weight in the obligation.
   b. pre-weakening: if lang_pre = true (trivially weak) and specs differ only in
      {pre, post.wp}, the morphism is discharged.  Soundness: concept_pre → true
      is a tautology, so in every calling context where concept:op is valid, the
      lang:op (which accepts all inputs) is also valid.
   c. effect-subset relaxation: if lang.effects (as a set) ⊆ concept.effects (as
      a set), the morphism is discharged.  Soundness: a consumer prepared for the
      concept op's full effect set is not surprised if the actual lang op does
      fewer effects.  The reverse (lang does MORE than concept promised) is never
      discharged.  Composes with wp-abstraction and pre-weakening.

Concept ops declare the UNION of all language effects for the same op, so the
effect-⊆ relaxation discharges language ops that do a proper subset of those effects.

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
GAP_DIR = CATALOG_REAL / "gaps"  # content-addressed gap mementos per §1.1

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
    "ite": {"c11": ["op_conditional.spec.json"], "csharp": ["op_ite.spec.json"], "python": ["op_ite-bool.spec.json"], "typescript": ["op_ite.spec.json"], "php": ["op_ternary.spec.json"], "ruby": ["op_ternary.spec.json"], "rust": ["op_ite.spec.json"], "java": ["op_ite.spec.json"]},
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
    op("not", ("c11", "op_not.spec.json")),
    op("assign", ("c11", "op_assign.spec.json"), renaming={"lvalue": "target", "rvalue": "value"}),
    op("decl", ("c11", "op_decl.spec.json"), renaming={"value": "initializer"}),
    op("seq", ("c11", "op_seq.spec.json")),
    op("skip", ("c11", "op_skip.spec.json")),
    op("conditional", ("c11", "op_if.spec.json"), concept_operator="conditional", base_operator="if"),
    op("ite", ("c11", "op_conditional.spec.json"), base_operator="conditional", renaming={"when_true": "then_expr", "when_false": "else_expr"}),
    op("while", ("c11", "op_while.spec.json")),
    op("do", ("c11", "op_do.spec.json")),
    op("for", ("c11", "op_for.spec.json")),
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
    "eq", "ne", "lt", "le", "gt", "ge", "not", "assign", "decl", "seq", "skip",
    "conditional", "ite", "while", "for", "break", "continue", "return", "call", "index", "member",
    "cast", "postinc", "postdec", "preinc", "predec", "source-unit",
}
# concept:and and concept:or are demoted: they are McCarthy desugarings of concept:ite
# (a && b = ite(a, b, false); a || b = ite(a, true, b)), not independent primitives.
# concept:foreach is demoted: no common iterator protocol across the 10 languages.
LANG_SUPPORTED = {
    "c11": PORTABLE_ALL | {"div", "do", "deref", "addr"},
    "csharp": PORTABLE_ALL | {"div", "do", "new", "throw", "ushr"},
    "go": (PORTABLE_ALL | {"div", "deref", "addr", "new"}) - {"postinc", "postdec", "preinc", "predec", "ite"},
    "python": (PORTABLE_ALL | {"div", "throw"}) - {"div", "bitnot", "shl", "shr", "postinc", "postdec", "preinc", "predec", "cast"},
    "typescript": (PORTABLE_ALL | {"new", "throw", "ushr"}) - {"div", "deref", "addr"},
    "zig": PORTABLE_ALL | {"div", "do", "deref", "addr", "new", "throw"},
    "ruby": (PORTABLE_ALL | {"throw"}) - {"bitnot", "shl", "shr", "postinc", "postdec", "preinc", "predec", "cast"},
    "php": (PORTABLE_ALL | {"new", "throw"}) - {"deref", "addr"},
    "java": (PORTABLE_ALL | {"div", "do", "new", "throw", "ushr"}) - {"deref", "addr"},
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


# Per-(lang, concept-slug) override reasons for known genuine semantic divergences.
# These are cases where transport will never discharge because the semantics differ,
# not just the encoding.
KNOWN_DIVERGENCE_REASONS = {
    ("python", "div"): "python:div is true division (5/2==2.5); concept:div is integer division (5/2==2)",
    ("python", "mod"): "python:mod is floored remainder (follows sign of divisor); concept:mod is truncated-toward-zero remainder (follows sign of dividend)",
    ("python", "add"): "python:add is polymorphic (dispatches on operand type: int, float, str, list); concept:add is integer-only",
    ("python", "sub"): "python:sub is polymorphic (dispatches on operand type); concept:sub is integer-only",
    ("python", "mul"): "python:mul is polymorphic (int * str repeats, etc.); concept:mul is integer-only",
    ("python", "neg"): "python:neg is polymorphic (dispatches on operand type); concept:neg is integer-only",
    ("python", "lt"): "python:lt is polymorphic (dispatches on operand type); concept:lt is integer-only",
    ("python", "le"): "python:le is polymorphic (dispatches on operand type); concept:le is integer-only",
    ("python", "gt"): "python:gt is polymorphic (dispatches on operand type); concept:gt is integer-only",
    ("typescript", "add"): "ts:+ is polymorphic (number | string concatenation); concept:add is integer-only",
}

# Per-(lang, concept-slug) known divergence classifications for gap_kind.
# Maps to (gap_kind, divergent_tag_or_None) for use in TransportGapMemento.
KNOWN_GAP_KINDS = {
    ("python", "div"): ("divergent-semantics", "integer-vs-true-division"),
    ("python", "mod"): ("divergent-semantics", "truncated-vs-floored-modulo"),
    ("python", "add"): ("polymorphic-source-op", None),
    ("python", "sub"): ("polymorphic-source-op", None),
    ("python", "mul"): ("polymorphic-source-op", None),
    ("python", "neg"): ("polymorphic-source-op", None),
    ("python", "lt"):  ("polymorphic-source-op", None),
    ("python", "le"):  ("polymorphic-source-op", None),
    ("python", "gt"):  ("polymorphic-source-op", None),
    ("typescript", "add"): ("polymorphic-source-op", None),
}


def classify_gap_kind(lang_id, slug, diff_text, after_spec=None, concept_spec=None):
    """Return (gap_kind, divergent_tag_or_None, structured_reason_dict).

    gap_kind is one of the TransportGapMemento gap-kind enum values.
    structured_reason mirrors the CDDL gap-reason type.
    """
    # Check known genuine divergences first (authoritative classification).
    key = (lang_id, slug) if lang_id and slug else None
    if key and key in KNOWN_GAP_KINDS:
        gap_kind, tag = KNOWN_GAP_KINDS[key]
        reason = {}
        if tag:
            reason["divergent_tag"] = tag
        return gap_kind, reason

    # Infer from diff_text and available specs.
    if diff_text == "no candidate source operation spec":
        reason = {"source_supported": False}
        return "missing-source-op", reason
    if diff_text == "operation not in supported set for this language":
        reason = {"source_supported": False}
        return "missing-source-op", reason
    if diff_text == "language signature directory is absent":
        # Spec ambiguity note: this is closer to "language not modeled" than
        # "missing-source-op".  Using missing-source-op with source_supported=false
        # per advisor guidance; flagged in PR body.
        reason = {"source_supported": False}
        return "missing-source-op", reason
    if "formal sort" in diff_text:
        reason = {}
        if after_spec and concept_spec:
            reason["formal_sorts_delta"] = {
                "got": after_spec.get("formal_sorts", []),
                "want": concept_spec.get("formal_sorts", []),
            }
        return "sort-mismatch", reason
    if "precondition" in diff_text:
        reason = {}
        if after_spec and concept_spec:
            reason["pre_delta"] = {
                "got": after_spec.get("pre"),
                "want": concept_spec.get("pre"),
            }
        return "sort-mismatch", reason
    if "effect signature" in diff_text:
        reason = {}
        if after_spec and concept_spec:
            reason["effects_delta"] = {
                "got": after_spec.get("effects"),
                "want": concept_spec.get("effects"),
            }
        return "effect-mismatch", reason
    if "arity_shape" in diff_text or "slot policy" in diff_text:
        reason = {}
        if after_spec and concept_spec:
            reason["post_delta"] = {
                "got": after_spec.get("post"),
                "want": concept_spec.get("post"),
            }
        return "arity-shape-mismatch", reason
    if "wp mismatch" in diff_text:
        reason = {}
        if after_spec and concept_spec:
            reason["wp_rule_delta"] = {
                "got": after_spec.get("post", {}).get("wp"),
                "want": concept_spec.get("post", {}).get("wp"),
            }
        return "wp-rule-mismatch", reason
    # Generic contract mismatch -- unspecified for now.
    return "unspecified", {}


def _default_resolution_options_for(gap_kind, lang_id, slug, reason_text):
    """Return a minimal resolution_options list for an auto-generated gap memento.

    The generator cannot compute full PartialMorphismMemento / LossyMorphismMemento
    CIDs (those are PR 2/N work).  So options here have no *_cid fields; they
    record the gap kind and tradeoff at minimum, with status: "recommended" for
    the most appropriate option.
    """
    if gap_kind == "divergent-semantics":
        return [
            {
                "option_kind": "lossy-morphism",
                "tradeoff": f"Accept the semantic divergence recorded in reason; ship a characterizably lossy bridge.",
                "status": "recommended",
            },
            {
                "option_kind": "partial-morphism",
                "tradeoff": "Restrict to the subdomain where the semantics agree.",
                "status": "deferred",
            },
            {
                "option_kind": "accept-permanent",
                "tradeoff": "Decline any bridge for this op pair.",
                "status": "rejected",
            },
        ]
    if gap_kind == "polymorphic-source-op":
        return [
            {
                "option_kind": "partial-morphism",
                "tradeoff": "Restrict to call-sites where operands are statically the integer sub-sort.",
                "status": "recommended",
            },
            {
                "option_kind": "split-target-op",
                "split_targets": [f"concept:{slug}-int", f"concept:{slug}-str"],
                "tradeoff": "Split the concept hub op into per-sort variants; morphisms discharge per variant.",
                "status": "deferred",
            },
            {
                "option_kind": "accept-permanent",
                "tradeoff": "Polymorphic dispatch is not bridgeable to a monomorphic hub op.",
                "status": "deferred",
            },
        ]
    if gap_kind == "missing-source-op":
        return [
            {
                "option_kind": "accept-permanent",
                "tradeoff": reason_text,
                "status": "recommended",
            },
        ]
    if gap_kind == "no-such-concept-op":
        return [
            {
                "option_kind": "extend-concept-hub",
                "tradeoff": "Add a new concept op to the hub to model this language primitive.",
                "status": "recommended",
            },
        ]
    if gap_kind in ("effect-mismatch",):
        return [
            {
                "option_kind": "re-spec-target-op",
                "tradeoff": "Widen the concept op's effect set to include the language op's effects.",
                "status": "recommended",
            },
            {
                "option_kind": "accept-permanent",
                "tradeoff": reason_text,
                "status": "deferred",
            },
        ]
    # Default: accept-permanent with reason as tradeoff.
    return [
        {
            "option_kind": "accept-permanent",
            "tradeoff": reason_text,
            "status": "recommended",
        },
    ]


def _gap_fn_name(lang_id, concept_op, slug=None):
    """Canonical fn_name for a TransportGapMemento."""
    if slug:
        return f"gap:{lang_id}:{slug}:to:{concept_op}"
    return f"gap:{lang_id}:to:{concept_op}"


def mint_gap_memento(lang_id, concept_op, slug, reason_text, gap_kind, structured_reason,
                     source_op_cid=None, shape_cid=None, gap_rows=None):
    """Write a TransportGapMemento JSON to GAP_DIR and return its CID.

    The memento is content-addressed by its JCS bytes via discharge.canonical_cid_value.
    The file is stored as gap_mementos/<stem>.<cid>.json.
    Returns (cid, path_str) or (None, None) on failure.
    """
    GAP_DIR.mkdir(parents=True, exist_ok=True)
    options = _default_resolution_options_for(gap_kind, lang_id, slug or "", reason_text)
    fn_name = _gap_fn_name(lang_id, concept_op, slug)
    memento = {
        "fn_name": fn_name,
        "gap_kind": gap_kind,
        "kind": "TransportGapMemento",
        "reason": structured_reason,
        "reason_note": reason_text,
        "resolution_options": options,
        "schema_version": "1",
        "signature": None,
        "source_lang": lang_id,
        "source_op_cid": source_op_cid,
        "target_concept_op": concept_op,
        "target_op_cid": shape_cid,
    }
    # JCS requires lexicographic key order; json.dumps with sort_keys=True achieves this
    # for the canonical bytes.  write_json uses indent=2 for the stored file (non-canonical
    # pretty-print for readability); the CID is computed from the canonical bytes.
    try:
        cid = discharge.canonical_cid_value(memento)
        stem = sanitize(fn_name)
        path = GAP_DIR / f"{stem}.{cid}.json"
        write_json(path, {"cid": cid, "memento": memento, "signature": None})
        if gap_rows is not None:
            gap_rows.append({"kind": "gap", "name": fn_name, "cid": cid, "path": str(path)})
        return cid, str(path)
    except Exception as exc:
        # Do not crash the generator on gap memento failure; emit unspecified fallback.
        print(f"WARNING: mint_gap_memento failed for {fn_name}: {exc}", file=sys.stderr)
        return None, None


def diff_reason(after, concept, lang_id=None, slug=None):
    # Check for known genuine divergences first.
    if lang_id and slug:
        known = KNOWN_DIVERGENCE_REASONS.get((lang_id, slug))
        if known:
            return known
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


def _effect_key(effect):
    """Canonical JSON key for an effect entry (for set-membership comparison)."""
    return json.dumps(effect, sort_keys=True, separators=(",", ":"))


def _effect_set(spec):
    """Return the set of canonical JSON keys for all effects in a spec."""
    return {_effect_key(e) for e in spec.get("effects", {}).get("effects", [])}


def _effects_subset(lang_effects_set, concept_effects_set):
    """Return True iff lang_effects_set ⊆ concept_effects_set.

    Sound direction: concept declares 'may do at most these effects'; lang does
    a subset, so a consumer prepared for the concept's worst case is fine.
    Never discharges the reverse (lang does MORE than concept promised).
    """
    return lang_effects_set <= concept_effects_set


def try_structural_subsumption(after_spec, concept_spec):
    """Sound structural ⊑ discharge when byte-equality fails on relaxable fields only.

    Three relaxations, all sound under the morphism contract:

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

    3. effect-subset relaxation: if lang.effects (as a set) ⊆ concept.effects (as a
       set), the morphism is discharged.  Soundness: the concept op promises 'may do
       at most these effects'; a lang op with fewer (or equal) effects refines it.
       A consumer prepared for the worst case is fine if actual does less.
       NEVER discharges if lang.effects ⊄ concept.effects (lang does more than promised).
       Composes with wp-abstraction (and pre-weakening).
       Discharge methods: "structural-effect-subset",
                          "structural-wp-abstraction-and-effect-subset",
                          "structural-pre-weakening-and-wp-abstraction-and-effect-subset".

    Returns (method_string, pre_relaxed, wp_abstracted) on success, or None on failure.
    Sound: false-negatives (remaining gaps) are acceptable; false-positives are not
    emitted because every structural claim has a verified implication.
    """
    import copy as _copy

    after_pre = after_spec.get("pre")
    concept_pre = concept_spec.get("pre")
    after_wp = after_spec.get("post", {}).get("wp")
    concept_wp = concept_spec.get("post", {}).get("wp")
    after_effects = _effect_set(after_spec)
    concept_effects = _effect_set(concept_spec)

    pre_matches = after_pre == concept_pre
    wp_matches = after_wp == concept_wp
    effects_match = after_effects == concept_effects
    effects_ok = _effects_subset(after_effects, concept_effects)

    if pre_matches and wp_matches and effects_match:
        # Byte-equality should have caught this already; shouldn't reach here.
        return None

    # Build a relaxed copy replacing just the fields we're allowed to relax.
    def _make_relaxed(relax_pre, relax_wp, relax_effects):
        relaxed = _copy.deepcopy(after_spec)
        if relax_pre:
            relaxed["pre"] = concept_pre
        if relax_wp:
            if "post" in relaxed:
                relaxed["post"]["wp"] = concept_wp
        if relax_effects:
            relaxed["effects"] = _copy.deepcopy(concept_spec.get("effects", empty_effects()))
        return relaxed

    # Helper: determine discharge method name from what was relaxed.
    def _method(relax_pre, relax_wp, relax_effects):
        parts = []
        if relax_pre:
            parts.append("pre-weakening")
        if relax_wp:
            parts.append("wp-abstraction")
        if relax_effects:
            parts.append("effect-subset")
        return "structural-" + "-and-".join(parts)

    # Only attempt discharge if effect direction is sound (lang ⊆ concept).
    if not effects_ok:
        return None

    # Try all useful combinations of relaxations.
    # Order: tightest first (fewest relaxations), then broader.
    for relax_pre, relax_wp, relax_effects in [
        # wp-only
        (False, True, False),
        # effects-only
        (False, False, True),
        # wp + effects
        (False, True, True),
        # pre + wp (lang pre must be true)
        (True, True, False),
        # pre + effects (lang pre must be true)
        (True, False, True),
        # pre + wp + effects (lang pre must be true)
        (True, True, True),
    ]:
        # pre-weakening requires lang pre to be the trivially-true formula.
        if relax_pre and not _is_true_pre(after_pre):
            continue
        # Skip if we're not actually relaxing anything.
        if not relax_pre and not relax_wp and not relax_effects:
            continue
        # Skip if the fields we'd relax already match (no benefit, and the
        # byte-equal fast-path should have caught full-match already).
        needs = (
            (relax_pre and not pre_matches)
            or (relax_wp and not wp_matches)
            or (relax_effects and not effects_match)
        )
        if not needs:
            continue
        relaxed = _make_relaxed(relax_pre, relax_wp, relax_effects)
        if canonical_cid_spec(relaxed) == canonical_cid_spec(concept_spec):
            method = _method(relax_pre, relax_wp, relax_effects)
            return (method, relax_pre, relax_wp)

    # No combination worked.
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


def _load_gap_mementos_from_disk():
    """Read all TransportGapMemento files from GAP_DIR and return as a sorted list.

    Each entry is the parsed memento dict (the inner 'memento' field of the catalog file).
    Sorted by fn_name for byte-stable output across runs.

    This is the source of truth for write_gap_report.  The on-disk files were written by
    mint_gap_memento() during this run (or a prior run).  Reading from disk (not in-memory
    list) ensures the second-run byte-stability test has teeth: if the in-memory dict shape
    changes without rewriting the file, the report would diverge.
    """
    if not GAP_DIR.exists():
        return []
    entries = []
    for path in sorted(GAP_DIR.glob("*.json")):
        try:
            data = read_json(path)
            m = data.get("memento", data)
            if m.get("kind") == "TransportGapMemento":
                entries.append(m)
        except Exception:
            continue
    return sorted(entries, key=lambda m: m.get("fn_name", ""))


def write_gap_report(gaps, records):
    # Load the on-disk gap mementos as the authoritative source for the Gaps table.
    # The in-memory `gaps` list is kept as a fallback for the spec/reason columns
    # (those fields are in the memento's reason_note).
    on_disk = _load_gap_mementos_from_disk()

    lines = [
        "# Program Transport Gaps",
        "",
        "Generated by `scripts/mint_language_morphisms.py`.",
        "Rows here are refusals to mint a morphism because the canonicalizer discharge did not land on the concept shape CID, or because the language has no op spec for that concept node.",
        "Each gap records the structural reason for the mismatch with actual vs. expected values.",
        "Gap mementos are content-addressed JSON files in `catalog/gaps/` (schema: `protocol/transport-gap-mementos.cddl`).",
        "",
        "## Semantic Restrictions",
        "",
        "- `concept:div` is integer division only. Floating-point division is out of scope for this node. `python:div` (true division, 5/2==2.5) and `js:`-style polymorphic division do not transport to `concept:div`.",
        "- `concept:mod` is truncated-toward-zero remainder. `python:%` / `python:mod` is floored remainder (follows sign of divisor, not dividend) and does not transport to `concept:mod`.",
        "- `concept:Int` is a fixed-width integer type. Languages with arbitrary-precision integers (`python:Int`, JS-style BigInt) do not transport to the fixed-width concept ops.",
        "- Polymorphic `python:add` / `js:+` dispatch on operand type (integer, float, string); `concept:add` is integer-only. These do not transport.",
        "- `concept:and` and `concept:or` are demoted from the hub: they are McCarthy desugarings of `concept:ite`, not independent primitives (`a && b = ite(a, b, false)`; `a || b = ite(a, true, b)`). Per-language `eq_and_to_ite_desugar` / `eq_or_to_ite_desugar` mementos record this. Languages with a boolean ternary transport `and`/`or` at the `ite` level after desugaring.",
        "- `concept:foreach` is demoted: no common iterator protocol across the 10 languages; cross-language `foreach` transport requires per-language iterator-op morphisms (`<lang>:iter` / `has_next` / `next`) that lifters do not currently emit. `foreach`-using programs correctly produce transport refusals.",
        "- `concept:ushr` is the logical zero-fill shift. It is separate from arithmetic `concept:shr`.",
        "- `concept:source-unit` is a lossless source-bytes plus operational-term wrapper.",
        "- Effect-subset relaxation: if `lang.effects` (as a set) is a subset of `concept.effects`, the morphism is discharged. Concept ops declare the union of all language effects for the same op. The reverse (lang does more than concept promised) is never discharged.",
        "",
        "## Minted Coverage", "", "| Concept op | Minted morphisms |", "| --- | --- |"]
    for record in records:
        lines.append(f"| `{record['concept']}` | {', '.join(m['name'] for m in record['morphisms']) or 'none'} |")

    lines += ["", "## Gaps", "", "| Language | Concept op | Gap kind | Source spec | Reason |", "| --- | --- | --- | --- | --- |"]

    if on_disk:
        # Primary path: render from content-addressed on-disk mementos.
        for m in on_disk:
            lang = m.get("source_lang", "")
            concept = m.get("target_concept_op", "")
            gap_kind = m.get("gap_kind", "unspecified")
            note = m.get("reason_note", "")
            # Extract source spec hint from reason_note (prose in the note for display).
            spec_hint = ""
            in_mem = next((g for g in gaps if g["language"] == lang and g["concept"] == concept), None)
            if in_mem:
                spec_hint = in_mem.get("spec", "")
            lines.append(f"| `{lang}` | `{concept}` | `{gap_kind}` | `{spec_hint}` | {note} |")
    else:
        # Fallback: no on-disk mementos yet (first run before GAP_DIR populated).
        for gap in gaps:
            lines.append(f"| `{gap['language']}` | `{gap['concept']}` | `unspecified` | `{gap['spec']}` | {gap['reason']} |")

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


def _collect_union_effects(op_def):
    """Return the union of effects.effects lists across all language specs for this op.

    Concept ops declare the UNION of all language effects for the same op so that the
    effect-subset relaxation can discharge language ops that do a proper subset.
    An effect entry is a dict; we dedup by canonical JSON (sort_keys=True).
    """
    seen_keys = {}
    for language in LANGUAGES:
        directory = specs_dir(language["id"])
        if not directory.is_dir():
            continue
        if op_def["slug"] not in LANG_SUPPORTED.get(language["id"], set()):
            continue
        for candidate in spec_candidates(op_def, language):
            path = directory / candidate
            if not path.exists():
                continue
            try:
                source_spec = read_json(path)
            except Exception:
                continue
            for effect in source_spec.get("effects", {}).get("effects", []):
                key = json.dumps(effect, sort_keys=True, separators=(",", ":"))
                if key not in seen_keys:
                    seen_keys[key] = effect
            break  # First candidate found is enough.
    return list(seen_keys.values())


def main():
    discharge.build_tools()
    for path in [SPEC_DIR, RECEIPT_DIR, DISCHARGE_DIR, CATALOG_REAL]:
        path.mkdir(parents=True, exist_ok=True)

    concept_specs = {op_def["slug"]: concept_spec_from_base(op_def) for op_def in OPS}

    # Task C: set each concept op's effects to the UNION of all language effects for
    # that op.  This ensures the effect-subset relaxation can discharge language ops
    # whose effect sets are proper subsets of the concept op's declared effects.
    for op_def in OPS:
        union_effects = _collect_union_effects(op_def)
        if union_effects:
            concept_specs[op_def["slug"]]["effects"] = {"effects": union_effects}

    rows, records, gaps = [], [], []
    gap_rows = []  # CID rows for gap mementos, appended to cids.tsv
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
                reason_text = "language signature directory is absent"
                gap_kind, structured_reason = classify_gap_kind(
                    language["id"], op_def["slug"], reason_text)
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": f"menagerie/{language['dir']}/specs", "reason": reason_text})
                mint_gap_memento(language["id"], op_def["concept_fn"], op_def["slug"],
                                 reason_text, gap_kind, structured_reason,
                                 source_op_cid=None, shape_cid=shape_cid, gap_rows=gap_rows)
                continue
            found = False
            if op_def["slug"] not in LANG_SUPPORTED.get(language["id"], set()):
                reason_text = "operation not in supported set for this language"
                gap_kind, structured_reason = classify_gap_kind(
                    language["id"], op_def["slug"], reason_text)
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": "not-supported", "reason": reason_text})
                mint_gap_memento(language["id"], op_def["concept_fn"], op_def["slug"],
                                 reason_text, gap_kind, structured_reason,
                                 source_op_cid=None, shape_cid=shape_cid, gap_rows=gap_rows)
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
                        reason_text = diff_reason(after_spec, concept_spec, lang_id=language["id"], slug=op_def["slug"])
                        gap_kind, structured_reason = classify_gap_kind(
                            language["id"], op_def["slug"], reason_text,
                            after_spec=after_spec, concept_spec=concept_spec)
                        gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": candidate, "reason": reason_text})
                        mint_gap_memento(language["id"], op_def["concept_fn"], op_def["slug"],
                                         reason_text, gap_kind, structured_reason,
                                         source_op_cid=source_cid, shape_cid=shape_cid, gap_rows=gap_rows)
                        continue
                    discharge_method, pre_relaxed, wp_abstracted = subsumption
                    effect_subset_relaxed = "effect-subset" in discharge_method
                else:
                    discharge_method = "canonicalizer-alpha-equivalence-plus-representation-map"
                    pre_relaxed = False
                    wp_abstracted = False
                    effect_subset_relaxed = False
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
                if effect_subset_relaxed:
                    receipt["effect_subset_relaxed"] = True
                receipt_cid, receipt_path = discharge.store_receipt(stem, receipt)
                rows.append({"kind": "receipt", "name": stem, "cid": receipt_cid, "path": receipt_path})
                record["morphisms"].append({"language": language["id"], "name": stem, "morphism_cid": morphism_cid, "receipt_cid": receipt_cid})
                break
            if not found:
                reason_text = "no candidate source operation spec"
                gap_kind, structured_reason = classify_gap_kind(
                    language["id"], op_def["slug"], reason_text)
                gaps.append({"language": language["id"], "concept": op_def["concept_fn"], "spec": f"op_{op_def['slug'].replace('-', '_')}.spec.json", "reason": reason_text})
                mint_gap_memento(language["id"], op_def["concept_fn"], op_def["slug"],
                                 reason_text, gap_kind, structured_reason,
                                 source_op_cid=None, shape_cid=shape_cid, gap_rows=gap_rows)
        records.append(record)
    append_cids(rows + gap_rows)
    write_gap_report(gaps, records)
    update_readme(records)
    discharge.scan_created_text()
    # Count gap mementos by kind from disk.
    gap_kind_counts = {}
    for m in _load_gap_mementos_from_disk():
        k = m.get("gap_kind", "unspecified")
        gap_kind_counts[k] = gap_kind_counts.get(k, 0) + 1
    print(f"concept_op_count\t{len(OPS)}")
    print(f"morphism_count\t{sum(len(r['morphisms']) for r in records)}")
    print(f"gap_count\t{len(gaps)}")
    print(f"gap_memento_count\t{sum(gap_kind_counts.values())}")
    for k in sorted(gap_kind_counts):
        print(f"gap_kind:{k}\t{gap_kind_counts[k]}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        raise
