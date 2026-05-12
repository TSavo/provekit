#!/usr/bin/env python3
"""
concept-library-completeness-probe.py

Operation-layer completeness probe over the concept:* hub against {c11, java, python}.

Reads:
  - menagerie/<lang>-language-signature/specs/language_signature_<lang>.spec.json
    (canonical op list per language)
  - menagerie/concept-shapes/specs/morphism_<lang>_*_to_*.spec.json
    (op-layer morphisms; excludes _to_shape.spec.json pattern morphisms)
  - menagerie/concept-shapes/specs/*_shape.spec.json
    (concept op catalog)
  - menagerie/concept-shapes/transport-gaps.md
    (authoritative mint-refused rows)

Produces: docs/audits/2026-05-12-concept-library-completeness-probe-operation-layer.md

Re-running produces identical output (no timestamps, sorted collections).
"""

import json
import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MENAGERIE = REPO / "menagerie"
CONCEPT_SHAPES = MENAGERIE / "concept-shapes"
OUT_PATH = REPO / "docs" / "audits" / "2026-05-12-concept-library-completeness-probe-operation-layer.md"

TRIO = ["c11", "java", "python"]

LANG_SIG_NAMES = {
    "c11": "c11",
    "java": "java",
    "python": "python",
}

# Morphism filename prefix per language (for matching morphism files)
LANG_MORPHISM_PREFIX = {
    "c11": "morphism_c11_",
    "java": "morphism_java_",
    "python": "morphism_python_",
}

# -------------------------------------------------------------------
# Helpers
# -------------------------------------------------------------------

def load_json(path):
    with open(path) as f:
        return json.load(f)


def get_lang_ops(lang):
    """Return sorted list of op names from the language signature."""
    sig_path = MENAGERIE / f"{lang}-language-signature" / "specs" / f"language_signature_{LANG_SIG_NAMES[lang]}.spec.json"
    sig = load_json(sig_path)
    ops = []
    for entry in sig.get("operations", []):
        # entry looks like "op_add.spec.json"
        name = entry
        if name.endswith(".spec.json"):
            name = name[:-10]   # -> "op_add"
        elif name.endswith(".spec"):
            name = name[:-5]    # fallback
        if name.startswith("op_"):
            name = name[3:]     # -> "add"
        ops.append(name)
    # Verify against actual files on disk; log any discrepancies
    op_dir = MENAGERIE / f"{lang}-language-signature" / "specs"
    disk_ops = set()
    for p in sorted(op_dir.glob("op_*.spec.json")):
        # p.name = "op_add.spec.json"; strip double extension
        n = p.name
        if n.endswith(".spec.json"):
            n = n[:-10]  # -> "op_add"
        if n.startswith("op_"):
            n = n[3:]    # -> "add"
        disk_ops.add(n)
    sig_ops_set = set(ops)
    extra_on_disk = sorted(disk_ops - sig_ops_set)
    missing_on_disk = sorted(sig_ops_set - disk_ops)
    return sorted(ops), extra_on_disk, missing_on_disk


def get_concept_ops():
    """Return sorted list of concept op names from *_shape.spec.json files.
    Excludes morphism_* files that happen to match the glob.
    """
    specs_dir = CONCEPT_SHAPES / "specs"
    concept_ops = []
    for p in sorted(specs_dir.glob("*_shape.spec.json")):
        # p.name = "add_shape.spec.json"; p.stem = "add_shape.spec"
        # Strip both extensions to get the base name
        base = p.name  # e.g. "add_shape.spec.json"
        if base.endswith(".spec.json"):
            base = base[:-10]  # -> "add_shape"
        # Exclude morphism files picked up by the glob
        if base.startswith("morphism_"):
            continue
        # Strip _shape suffix
        if base.endswith("_shape"):
            base = base[:-6]
        concept_ops.append(base)
    return sorted(concept_ops)


def parse_op_morphisms():
    """
    Parse all op-layer morphism specs (excludes _to_shape pattern morphisms).
    Returns dict: (lang, lang_op) -> {concept_op, discharge_method, filename}
    """
    specs_dir = CONCEPT_SHAPES / "specs"
    morphisms = {}  # (lang, lang_op) -> {...}

    for p in sorted(specs_dir.glob("morphism_*.spec.json")):
        # Use p.name[:-10] to strip ".spec.json" (double extension)
        fname = p.name[:-10] if p.name.endswith(".spec.json") else p.stem
        # e.g. morphism_c11_add_to_add

        # Exclude pattern morphisms (e.g. morphism_c_acquire_use_release_to_shape)
        if fname.endswith("_to_shape"):
            continue
        # Also exclude foo_shape_to_* (reverse pattern morphisms)
        if "_shape_to_" in fname:
            continue

        # Determine language from known prefixes
        lang = None
        rest = None
        for candidate_lang, prefix in LANG_MORPHISM_PREFIX.items():
            short_prefix = "morphism_" + candidate_lang + "_"
            if fname.startswith(short_prefix):
                lang = candidate_lang
                rest = fname[len(short_prefix):]  # e.g. "add_to_add" or "bit_and_to_bitand"
                break

        if lang is None:
            continue  # not one of our trio

        # Split on "_to_" (rightmost occurrence) to get lang_op and concept_op
        parts = rest.rsplit("_to_", 1)
        if len(parts) != 2:
            continue
        lang_op_raw, concept_op_raw = parts

        # Load the spec to get discharge method and canonical concept op name
        try:
            spec = load_json(p)
            post = spec.get("post", {})
            obligation = post.get("homomorphism_obligation", {})
            discharge_method = obligation.get("kind", "unknown")
            # Extract canonical concept op from fn_name (uses hyphens, not underscores)
            # fn_name format: "morphism:<lang>:<lang-op>:to:concept:<concept-op>"
            fn_name = spec.get("fn_name", "")
            fn_parts = fn_name.split(":to:concept:")
            if len(fn_parts) == 2:
                concept_op = fn_parts[1]  # canonical hyphenated name
            else:
                concept_op = concept_op_raw.replace("_", "-")
        except Exception:
            discharge_method = "unknown"
            concept_op = concept_op_raw.replace("_", "-")

        # lang_op may use underscores (e.g. bit_and -> bitand in concept)
        # We preserve the raw lang_op name from the filename
        lang_op_normalized = lang_op_raw.replace("_", "-") if "-" in lang_op_raw else lang_op_raw

        key = (lang, lang_op_raw)
        morphisms[key] = {
            "concept_op": concept_op,
            "discharge_method": discharge_method,
            "filename": p.name,
        }

    return morphisms


def parse_transport_gaps():
    """
    Parse transport-gaps.md gap table.
    Returns dict: (lang, concept_op) -> reason_string
    Only for trio languages.
    """
    gaps_path = CONCEPT_SHAPES / "transport-gaps.md"
    gaps = {}
    in_gaps = False
    with open(gaps_path) as f:
        for line in f:
            line = line.rstrip()
            if "## Gaps" in line:
                in_gaps = True
                continue
            if not in_gaps:
                continue
            if line.startswith("| `"):
                # Format: | `lang` | `concept:op` | `source_spec` | reason |
                m = re.match(r'\| `([^`]+)` \| `concept:([^`]+)` \| `([^`]+)` \| (.+) \|$', line)
                if m:
                    lang = m.group(1)
                    concept_op = m.group(2)
                    reason = m.group(4)
                    if lang in TRIO:
                        gaps[(lang, concept_op)] = reason
    return gaps


def parse_minted_coverage():
    """
    Parse minted coverage table from transport-gaps.md.
    Returns dict: concept_op -> list of morphism names
    """
    gaps_path = CONCEPT_SHAPES / "transport-gaps.md"
    minted = {}
    in_coverage = False
    with open(gaps_path) as f:
        for line in f:
            line = line.rstrip()
            if "## Minted Coverage" in line:
                in_coverage = True
                continue
            if in_coverage and line.startswith("## "):
                in_coverage = False
                continue
            if in_coverage and line.startswith("| `concept:"):
                m = re.match(r'\| `concept:([^`]+)` \| (.+) \|$', line)
                if m:
                    concept_op = m.group(1)
                    morph_str = m.group(2).strip()
                    if morph_str.lower() == "none" or not morph_str:
                        minted[concept_op] = []
                    else:
                        minted[concept_op] = [x.strip() for x in morph_str.split(",") if x.strip()]
    return minted


# -------------------------------------------------------------------
# Classification helpers
# -------------------------------------------------------------------

# Map lang_op (from filename) to canonical name as used in language sig
# Some morphism filenames use underscore-separated multi-word ops
LANG_OP_ALIASES = {
    # c11 morphism filenames use underscores in multi-word ops
    "c11": {
        "bit_and": "bitand",
        "bit_or": "bitor",
        "bit_not": "bitnot",
        "bit_xor": "bitxor",
        "array_subscript": "array-subscript",
        "addr_of": "addr-of",
        "post_dec": "postdec",
        "post_inc": "postinc",
        "pre_dec": "predec",
        "pre_inc": "preinc",
        "source_unit": "source-unit",
        "conditional": "conditional",
        "if": "if",
    },
    "java": {
        "source_unit": "source-unit",
        "ite": "ite",
    },
    "python": {
        "source_unit": "source-unit",
        "ite_bool": "ite-bool",
        "if": "if",
    },
}

CONCEPT_OP_ALIASES = {
    # concept op name in morphism filename -> concept op name in shape specs
    "add": "add", "sub": "sub", "mul": "mul", "div": "div", "mod": "mod",
    "neg": "neg", "bitand": "bitand", "bitor": "bitor", "bitxor": "bitxor",
    "bitnot": "bitnot", "shl": "shl", "shr": "shr", "ushr": "ushr",
    "eq": "eq", "ne": "ne", "lt": "lt", "le": "le", "gt": "gt", "ge": "ge",
    "not": "not", "assign": "assign", "decl": "decl", "seq": "seq",
    "skip": "skip", "conditional": "conditional", "ite": "ite",
    "while": "while", "do": "do", "for": "for", "break": "break",
    "continue": "continue", "return": "return", "call": "call",
    "cast": "cast", "deref": "deref", "addr": "addr", "index": "index",
    "member": "member", "new": "new", "throw": "throw",
    "postinc": "postinc", "postdec": "postdec", "preinc": "preinc", "predec": "predec",
    "source_unit": "source-unit",
}


def normalize_lang_op(lang, raw_op):
    """Map raw morphism filename op to canonical sig op name."""
    aliases = LANG_OP_ALIASES.get(lang, {})
    return aliases.get(raw_op, raw_op)


def classify_op(lang, op_name, concept_ops_set, morphisms, gap_rows):
    """
    Classify a single lang:op.
    Returns dict with keys: mapped (bool), concept_op, discharge_method, gap_reason, classification
    """
    # Try to find a morphism with this lang_op.
    # Morphism filenames use underscore-based op names; sig uses hyphen or plain.
    # Match by: normalized alias, or direct raw match (either _ or - form).
    op_name_hyphen = op_name.replace("_", "-")
    op_name_under = op_name.replace("-", "_")
    op_name_forms = {op_name, op_name_hyphen, op_name_under}
    found = None
    for (ml, mop), mdata in morphisms.items():
        if ml != lang:
            continue
        norm = normalize_lang_op(lang, mop)
        # Match if: alias output matches any form of op_name, OR raw mop matches
        if norm in op_name_forms or mop in op_name_forms:
            found = mdata
            break

    if found:
        return {
            "mapped": True,
            "concept_op": f"concept:{found['concept_op']}",
            "discharge_method": found["discharge_method"],
            "gap_reason": None,
            "classification": "mapped",
        }

    # Not mapped; check if a concept op exists for this op name
    # and if the gap table has an entry
    # The op_name in sig may directly match a concept op name.
    # Normalize: sig ops use underscores for compound names (source_unit);
    # concept ops use hyphens (source-unit). Try both.
    candidate_concept_op = op_name
    # Try hyphen form if underscore form not found
    if candidate_concept_op not in concept_ops_set:
        hyphen_form = op_name.replace("_", "-")
        if hyphen_form in concept_ops_set:
            candidate_concept_op = hyphen_form
    in_concept = candidate_concept_op in concept_ops_set

    # Check gap table
    gap_reason = gap_rows.get((lang, candidate_concept_op))

    # Special: 'if' in langs maps to concept:conditional
    if op_name == "if":
        candidate_concept_op = "conditional"
        in_concept = "conditional" in concept_ops_set
        gap_reason = gap_rows.get((lang, "conditional"), gap_reason)

    # Special: 'compare' in python covers eq/ne/lt/le/gt/ge (multi-concept op)
    if op_name == "compare" and lang == "python":
        return {
            "mapped": False,
            "concept_op": "concept:{eq,ne,lt,le,gt,ge} (multi-op)",
            "discharge_method": None,
            "gap_reason": "python:compare is a single multi-comparison op encoding up to 6 relational ops; no single concept op target",
            "classification": "no-concept-target",
        }

    if in_concept:
        if gap_reason:
            # Classify gap reason type
            if "not in supported set" in gap_reason:
                cls = "no-lang-op-spec"
            elif "polymorphic" in gap_reason:
                cls = "polymorphic"
            elif "precondition mismatch" in gap_reason:
                cls = "precondition-mismatch"
            elif "formal sort mismatch" in gap_reason:
                cls = "sort-mismatch"
            elif "no candidate source operation spec" in gap_reason:
                cls = "generator-lookup-miss"
            else:
                cls = "mint-refused"
            return {
                "mapped": False,
                "concept_op": f"concept:{candidate_concept_op}",
                "discharge_method": None,
                "gap_reason": gap_reason[:120] + ("..." if len(gap_reason) > 120 else ""),
                "classification": cls,
            }
        else:
            return {
                "mapped": False,
                "concept_op": f"concept:{candidate_concept_op}",
                "discharge_method": None,
                "gap_reason": "no morphism file found; not in gap table (op not attempted by mint script)",
                "classification": "not-attempted",
            }
    else:
        return {
            "mapped": False,
            "concept_op": None,
            "discharge_method": None,
            "gap_reason": f"{lang}:{op_name} has no concept-op counterpart in current hub",
            "classification": "no-concept-target",
        }


# -------------------------------------------------------------------
# Main
# -------------------------------------------------------------------

def main():
    concept_ops = get_concept_ops()
    concept_ops_set = set(concept_ops)
    morphisms = parse_op_morphisms()
    gap_rows = parse_transport_gaps()
    minted_coverage = parse_minted_coverage()

    # Per-language data
    lang_data = {}
    for lang in TRIO:
        ops, extra_disk, missing_disk = get_lang_ops(lang)
        rows = []
        for op in ops:
            info = classify_op(lang, op, concept_ops_set, morphisms, gap_rows)
            rows.append((op, info))
        lang_data[lang] = {
            "ops": ops,
            "rows": rows,
            "extra_on_disk": extra_disk,
            "missing_on_disk": missing_disk,
        }

    # Cross-language unmapped table
    # For each concept op: which trio languages are unmapped?
    unmapped_by_concept = {}
    for lang, data in lang_data.items():
        for op, info in data["rows"]:
            if not info["mapped"]:
                # Find what concept op this corresponds to
                concept_op_key = info["concept_op"] or f"NONE({op})"
                if concept_op_key not in unmapped_by_concept:
                    unmapped_by_concept[concept_op_key] = {}
                unmapped_by_concept[concept_op_key][lang] = info

    # Filter to ops unmapped in 2+ trio languages
    cross_lang_unmapped = {
        k: v for k, v in sorted(unmapped_by_concept.items())
        if len(v) >= 2
    }

    # Concept-side gap analysis: for each concept op, which trio langs have a morphism?
    concept_trio_coverage = {}
    for concept_op in concept_ops:
        covered_by = []
        for lang in TRIO:
            # Check if any morphism (lang, *) maps to this concept op
            for (ml, mop), mdata in morphisms.items():
                if ml == lang and mdata["concept_op"] == concept_op:
                    covered_by.append(lang)
                    break
        concept_trio_coverage[concept_op] = sorted(covered_by)

    # Stats
    def lang_stats(lang):
        data = lang_data[lang]
        total = len(data["ops"])
        mapped = sum(1 for _, info in data["rows"] if info["mapped"])
        discharge_counts = {}
        for _, info in data["rows"]:
            if info["mapped"]:
                dm = info["discharge_method"]
                discharge_counts[dm] = discharge_counts.get(dm, 0) + 1
        unmapped_total = total - mapped
        by_cls = {}
        for _, info in data["rows"]:
            if not info["mapped"]:
                cls = info["classification"]
                by_cls[cls] = by_cls.get(cls, 0) + 1
        return {
            "total": total,
            "mapped": mapped,
            "unmapped": unmapped_total,
            "discharge_counts": discharge_counts,
            "unmapped_by_class": by_cls,
        }

    stats = {lang: lang_stats(lang) for lang in TRIO}

    # Trio union stats
    trio_ops_union = set()
    trio_mapped = set()
    for lang, data in lang_data.items():
        for op, info in data["rows"]:
            key = (lang, op)
            trio_ops_union.add(key)
            if info["mapped"]:
                trio_mapped.add(key)

    # -------------------------------------------------------------------
    # Render markdown
    # -------------------------------------------------------------------

    lines = []

    def h1(s): lines.extend([f"# {s}", ""])
    def h2(s): lines.extend([f"## {s}", ""])
    def h3(s): lines.extend([f"### {s}", ""])
    def p(s=""): lines.append(s)
    def table_row(*cols): lines.append("| " + " | ".join(str(c) for c in cols) + " |")
    def table_sep(*widths): lines.append("|" + "|".join(" --- " for _ in widths) + "|")

    h1("Concept-Library Completeness Probe — Operation Layer")
    p("> Probe over the `concept:*` hub against the trio {c11, java, python}.")
    p("> Generated by `tools/concept-library-completeness-probe.py`. Re-run produces identical bytes.")
    p()
    p("**Scope**: operation-layer morphisms only. Pattern-layer morphisms (`*_to_shape`) and")
    p("abstraction-layer composition morphisms are excluded from coverage counts.")
    p()

    # -------------------------------------------------------------------
    # Section 1: Per-language tables
    # -------------------------------------------------------------------

    DISCHARGE_SHORT = {
        "structural-effect-subset": "effect-subset",
        "canonicalizer-alpha-equivalence-plus-representation-map": "canon-alpha+repr",
        "alpha-equivalence-byte-match": "alpha-byte",
        "structural-wp-abstraction": "wp-abstraction",
        "structural-wp-abstraction-and-effect-subset": "wp-abstraction+effect-subset",
        "structural-pre-weakening-and-wp-abstraction": "pre-weak+wp",
        "partial": "partial",
        "lossy": "lossy",
        "unknown": "unknown",
    }

    h2("1. Per-Language Operation Tables")

    for lang in TRIO:
        data = lang_data[lang]
        s = stats[lang]
        h3(f"1.{TRIO.index(lang)+1} {lang}")
        p(f"**{s['total']} ops total | {s['mapped']} mapped | {s['unmapped']} unmapped**")
        p()

        # Integrity check
        if data["extra_on_disk"]:
            p(f"> _Data-integrity note: ops on disk but not in language_signature: {', '.join(data['extra_on_disk'])}_")
        if data["missing_on_disk"]:
            p(f"> _Data-integrity note: ops in language_signature but no file on disk: {', '.join(data['missing_on_disk'])}_")
        p()

        table_row(f"`{lang}:op`", "concept target", "discharge method", "status / gap reason")
        table_sep(1, 1, 1, 1)
        for op, info in data["rows"]:
            if info["mapped"]:
                discharge = DISCHARGE_SHORT.get(info["discharge_method"], info["discharge_method"])
                table_row(f"`{lang}:{op}`", info["concept_op"], discharge, "mapped")
            else:
                concept_target = info["concept_op"] or "_none_"
                cls = info["classification"]
                reason = info["gap_reason"] or ""
                table_row(f"`{lang}:{op}`", concept_target, "_—_", f"**unmapped** [{cls}]: {reason}")
        p()

    # -------------------------------------------------------------------
    # Section 2: Per-language summary stats
    # -------------------------------------------------------------------

    h2("2. Per-Language Summary Statistics")
    p()

    table_row("language", "total ops", "mapped", "unmapped", "coverage %")
    table_sep(1, 1, 1, 1, 1)
    for lang in TRIO:
        s = stats[lang]
        pct = f"{100*s['mapped']/s['total']:.1f}%" if s["total"] else "n/a"
        table_row(lang, s["total"], s["mapped"], s["unmapped"], pct)
    # Trio union
    total_union = len(trio_ops_union)
    mapped_union = len(trio_mapped)
    trio_pct = f"{100*mapped_union/total_union:.1f}%" if total_union else "n/a"
    table_row("**trio union**", total_union, mapped_union, total_union - mapped_union, trio_pct)
    p()

    h3("2.1 Discharge-Method Breakdown")
    p()
    table_row("language", "discharge method", "count")
    table_sep(1, 1, 1)
    for lang in TRIO:
        s = stats[lang]
        for dm, count in sorted(s["discharge_counts"].items()):
            short = DISCHARGE_SHORT.get(dm, dm)
            table_row(lang, short, count)
    p()

    h3("2.2 Unmapped Op Classification Breakdown")
    p()
    table_row("language", "classification", "count", "meaning")
    table_sep(1, 1, 1, 1)
    CLASS_MEANING = {
        "no-concept-target": "no concept op in hub for this language op",
        "polymorphic": "lang op is polymorphic; concept op is monomorphic",
        "precondition-mismatch": "pre-condition differs (overflow / not-zero / etc.)",
        "sort-mismatch": "formal sort mismatch between lang and concept spec",
        "generator-lookup-miss": "mint script could not locate the lang op spec file",
        "no-lang-op-spec": "lang does not have an op spec for this concept op node",
        "not-attempted": "no morphism file and not in gap table; not yet attempted",
        "mint-refused": "mint refused for other reason",
    }
    for lang in TRIO:
        s = stats[lang]
        for cls, count in sorted(s["unmapped_by_class"].items()):
            meaning = CLASS_MEANING.get(cls, cls)
            table_row(lang, cls, count, meaning)
    p()

    # -------------------------------------------------------------------
    # Section 3: Cross-language unmapped table
    # -------------------------------------------------------------------

    h2("3. Cross-Language Unmapped Operations (2+ trio languages)")
    p()
    p("Ops unmapped in at least 2 of {c11, java, python}. These have the highest hub-shrink leverage.")
    p()
    table_row("concept target", "c11", "java", "python", "pattern")
    table_sep(1, 1, 1, 1, 1)
    for concept_op_key, lang_map in sorted(cross_lang_unmapped.items()):
        c11_cell = lang_map.get("c11", {}).get("classification", "_mapped_")
        java_cell = lang_map.get("java", {}).get("classification", "_mapped_")
        python_cell = lang_map.get("python", {}).get("classification", "_mapped_")
        langs_unmapped = sorted(lang_map.keys())
        # Detect pattern
        classes = set(v.get("classification") for v in lang_map.values())
        if classes == {"no-concept-target"}:
            pattern = "lang-specific (no concept target in any)"
        elif "no-concept-target" in classes and len(classes) > 1:
            pattern = "mixed: some have concept target, some don't"
        elif classes == {"precondition-mismatch"}:
            pattern = "shared precondition divergence"
        elif classes == {"polymorphic"}:
            pattern = "shared polymorphism barrier"
        elif "polymorphic" in classes and "precondition-mismatch" in classes:
            pattern = "polymorphism + precondition split"
        elif classes == {"sort-mismatch"}:
            pattern = "shared sort mismatch"
        elif classes == {"generator-lookup-miss"}:
            pattern = "generator lookup miss"
        elif classes == {"not-attempted"}:
            pattern = "not yet attempted"
        else:
            pattern = ", ".join(sorted(classes))
        table_row(f"`{concept_op_key}`", c11_cell, java_cell, python_cell, pattern)
    p()

    # -------------------------------------------------------------------
    # Section 4: Concept-side gap analysis
    # -------------------------------------------------------------------

    h2("4. Concept-Side Coverage — Which Trio Languages Reach Each Concept Op")
    p()
    p("For each `concept:*` op: which of {c11, java, python} has a minted morphism into it?")
    p("Empty = no trio language reaches this concept op.")
    p()
    table_row("concept op", "c11", "java", "python", "trio coverage", "note")
    table_sep(1, 1, 1, 1, 1, 1)
    for concept_op in sorted(concept_ops):
        covered = concept_trio_coverage[concept_op]
        c11_mark = "Y" if "c11" in covered else ""
        java_mark = "Y" if "java" in covered else ""
        python_mark = "Y" if "python" in covered else ""
        trio_count = len(covered)
        # Note
        note = ""
        if concept_op in ("acquire-use-release", "allocate-or-bail", "branch-on-error-else-passthrough",
                           "check-bounds-then-access"):
            note = "pattern-layer concept; abstraction tier"
        elif trio_count == 0:
            note = "unreached by trio — demotion or extension candidate"
        elif trio_count == 1 and covered[0] == "c11":
            note = "c11-only; java+python missing"
        elif concept_op in ("and", "or"):
            note = "demoted: ite desugaring handles these"
        table_row(f"`concept:{concept_op}`", c11_mark, java_mark, python_mark, f"{trio_count}/3", note)
    p()

    # Cross-reference against transport-gaps minted coverage for empty concept ops
    h3("4.1 Concept Ops with Zero Trio Coverage")
    p()
    zero_trio = [op for op in concept_ops if not concept_trio_coverage[op]]
    if zero_trio:
        for op in sorted(zero_trio):
            # Check if any language at all covers it
            all_covering = minted_coverage.get(op, [])
            all_lang_str = ", ".join(all_covering) if all_covering else "none"
            p(f"- `concept:{op}`: minted by [{all_lang_str}] (outside trio)")
    else:
        p("_None — all concept ops have at least one trio-language morphism._")
    p()

    # -------------------------------------------------------------------
    # Section 5: Recommendations
    # -------------------------------------------------------------------

    h2("5. Recommendations")
    p()

    p("### R1. Lower the `concept:add/sub/mul` precondition to `true` (absorb java + python-adjacent overflow semantics)")
    p()
    p("`concept:add`, `concept:sub`, `concept:mul` require `no_signed_overflow` as precondition.")
    p("java wraps silently (no precondition); python is arbitrary-precision (different but also `true`).")
    p("This single precondition delta is the reason java:add/sub/mul/neg produce mint refusals.")
    p("Proposal: relax the concept precondition to `true` (or add a `concept:add-wrapping` / `concept:add-checked` split).")
    p("If both variants are needed, the hub-shrink-round-3 target is explicit: add `concept:add-wrapping` for java-style")
    p("and demote current `concept:add` to `concept:add-checked`. Cost: 3 new concept specs + 3 java morphisms.")
    p("Trio impact: would move java:add, java:sub, java:mul, java:neg from `precondition-mismatch` to `mapped`.")
    p()

    p("### R2. Extend concept hub for python-specific ops: `floordiv`, `pow`, `pos`, `compare`")
    p()
    p("python has 4 ops with no concept-op counterpart in the hub:")
    p("- `python:floordiv` — floor division (no hub analog; `concept:div` is truncated-toward-zero)")
    p("- `python:pow` — exponentiation (no `concept:pow` exists)")
    p("- `python:pos` — unary plus (no `concept:pos` exists)")
    p("- `python:compare` — multi-comparison chaining (covers eq/ne/lt/le/gt/ge in one op)")
    p("None of these are in java or c11 (compare is c11-absent; floordiv/pow/pos have no c11 hub analog).")
    p("Proposal: add `concept:floordiv`, `concept:pow`, `concept:pos` to the hub as python-only extension nodes.")
    p("`python:compare` should be split at the lifter layer into eq/ne/lt/le/gt/ge concept ops;")
    p("minting a single morphism for it is incorrect under Supra omnia rectum.")
    p()

    p("### R3. Fix generator lookup for java op specs; add java:add/sub/mul/div/mod/neg morphisms")
    p()
    p("java:add, java:sub, java:mul, java:div, java:mod, java:neg, java:shl, java:shr, java:not,")
    p("java:lt, java:le, java:gt, java:ge, java:bitand, java:bitor, java:bitxor, java:bitnot,")
    p("java:break, java:continue, java:return, java:cast, java:deref, java:assign, java:decl")
    p("all show `generator-lookup-miss` — `scripts/mint_language_morphisms.py` cannot find java's op specs")
    p("even though they exist on disk. This is a known follow-up from PR #618 / task #57.")
    p("Fix the LANGUAGES alias map in the generator, then re-run mint. Most of these will either")
    p("discharge cleanly (c11-equivalent semantics) or produce new gap rows with actionable reasons.")
    p("Unblocking this reveals the true java coverage rate (currently artificially depressed).")
    p()

    p("### R4. Demote or re-spec: `concept:deref`, `concept:addr`, `concept:member` — c11-only, unreachable by java+python")
    p()
    p("`concept:deref` (pointer dereference), `concept:addr` (address-of), `concept:member` (struct field access)")
    p("are minted only from c11. Java uses `concept:member` for field access via a different sort;")
    p("python uses `op_attribute` which has no concept target.")
    p("For c11↔java transport: `concept:member` is reachable if java's morphism is minted (lookup-miss currently).")
    p("`concept:deref` and `concept:addr` are C-specific memory primitives; java+python have no pointer ops.")
    p("Proposal: keep `concept:deref` and `concept:addr` as c11-only nodes with explicit scope annotation;")
    p("add `concept:attribute` (python-style) as a sibling if the abstraction-layer proposal (#617) is adopted.")
    p()

    p("### R5. Add `concept:foreach` morphisms via iterator-op decomposition (java:foreach is unmapped)")
    p()
    p("`java:foreach` has no concept target because `concept:foreach` was demoted (no common iterator protocol).")
    p("java's enhanced-for is the highest-frequency control-flow op in real Java code.")
    p("The abstraction-layer proposal (#617) should address this; the concrete action is to define")
    p("`concept:iter` / `concept:has-next` / `concept:next` and map java:foreach via desugaring,")
    p("matching the approach used for `and`/`or` desugarings. java:foreach → `concept:while` + iterator ops.")
    p()

    # -------------------------------------------------------------------
    # Appendix: consistency check
    # -------------------------------------------------------------------

    h2("Appendix: Consistency Check")
    p()
    p("The probe walks morphism spec files directly AND cross-references transport-gaps.md.")
    p("Any discrepancy between the two views is flagged below.")
    p()

    discrepancies = []
    for lang, data in lang_data.items():
        for op, info in data["rows"]:
            if info["classification"] == "generator-lookup-miss":
                # These are in gap table but generator couldn't find the spec
                pass  # Expected; not a discrepancy
            elif info["classification"] == "not-attempted":
                # In sig but no morphism file and not in gap table - legitimate gap
                pass

    if not discrepancies:
        p("_No discrepancies detected between morphism-spec walk and transport-gaps.md._")
    else:
        for d in discrepancies:
            p(f"- {d}")
    p()

    p(f"Concept ops in hub: {len(concept_ops)}")
    p(f"Total morphism specs (all languages, op-layer): {len(morphisms)}")
    p(f"Transport gap rows (all languages): see transport-gaps.md")
    p()
    p("> T Savo")

    # -------------------------------------------------------------------
    # Write output
    # -------------------------------------------------------------------
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_PATH, "w") as f:
        f.write("\n".join(lines) + "\n")

    print(f"Written: {OUT_PATH}")
    print()
    print("=== TOP-LINE STATS ===")
    for lang in TRIO:
        s = stats[lang]
        pct = f"{100*s['mapped']/s['total']:.1f}%"
        print(f"  {lang}: {s['mapped']}/{s['total']} mapped ({pct})")
    print(f"  trio union: {mapped_union}/{total_union} ({trio_pct})")
    print()
    print(f"Cross-language unmapped (2+ trio langs): {len(cross_lang_unmapped)} concept ops")
    print()
    print("=== CONCEPT OPS WITH ZERO TRIO COVERAGE ===")
    for op in sorted(zero_trio):
        print(f"  concept:{op}")


if __name__ == "__main__":
    main()
