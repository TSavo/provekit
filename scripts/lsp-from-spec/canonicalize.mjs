// Inline canonicalizer + propertyHash, derived from
// docs/specs/2026-04-30-canonicalization-grammar.md.
//
// Scope of this prototype: a CORRECT implementation of passes 1..6 + pass 7 JCS
// + pass 8 sha256-prefix-16 is mechanical but >300 lines. For the demo we
// implement:
//
//   - Pass 1 (de Bruijn): full implementation per §8.1.
//   - Pass 2 (predicate canonicalization): ALIAS_TABLE + flip-on-constants per §8.2.
//   - Pass 3 (sort canonicalization): identity per §8.3.
//   - Pass 4 (implies removal): per §8.4.
//   - Pass 5 (NNF): per §8.5, NEGATE_PREDICATE table per spec.
//   - Pass 6 (AC normalization): flatten, identity rules, dedup, sorted by lex byte order
//     of the JCS encoding (§6 sortKey, §8.6 normalizeAnd/normalizeOr).
//   - Pass 7 JCS: sorted-keys, no-whitespace, RFC 8785 strings, native int form.
//   - Pass 8: SHA-256, lowercase hex, first 16 chars.
//
// The §17 alignment items are honored:
//   - We default to JCS form (the spec's CBOR default is unimplemented here for
//     prototype scope; this is one of the gaps flagged in the analysis doc).
//   - Bignums emit as decimal digits per §7.3.5 (NOT "bigint:N" strings).
//   - Real(N.0) emits with ".0" per §7.3.6 (NOT bare "3").
//
// A from-spec implementation that follows these rules will produce hashes that
// DIFFER from the current TS reference at src/canonicalizer/ for the same
// inputs. Per §17, that is the protocol-leads posture working as intended.

import { createHash } from "node:crypto";

const ALIAS_TABLE = {
  "==": "=", "eq": "=", "equal": "=",
  "!=": "≠", "notEqual": "≠", "not-equal": "≠", "ne": "≠",
  "lt": "<", "lessThan": "<", "less-than": "<",
  "lte": "≤", "le": "≤", "lessThanOrEqual": "≤", "less-than-or-equal": "≤",
  "gt": ">", "greaterThan": ">", "greater-than": ">",
  "gte": "≥", "ge": "≥", "greaterThanOrEqual": "≥", "greater-than-or-equal": "≥",
  "∈": "member", "in": "member",
  "⊆": "subset", "subseteq": "subset",
  "kindOf": "kind-of", "kind_of": "kind-of",
  "dataFlowsTo": "data-flows-to", "data_flows_to": "data-flows-to",
  "postDominates": "post-dominates", "post_dominates": "post-dominates",
  "onPath": "on-path", "on_path": "on-path",
  "transitionFromTo": "transition-from-to", "transition_from_to": "transition-from-to",
};
const FLIP_PREDICATE = { "<": ">", ">": "<", "≤": "≥", "≥": "≤" };
const NEGATE_PREDICATE = {
  "=": "≠", "≠": "=",
  "<": "≥", "≤": ">", ">": "≤", "≥": "<",
  "true": "false", "false": "true",
};

// Pass 1: de Bruijn (§8.1)
export function applyDeBruijn(formula, stack = []) {
  switch (formula.kind) {
    case "forall":
    case "exists": {
      const inner = applyDeBruijn(formula.predicate.body, [formula.predicate.varName, ...stack]);
      return { kind: formula.kind, sort: formula.sort, body: inner };
    }
    case "and":
      return { kind: "and", operands: formula.conjuncts.map(c => applyDeBruijn(c, stack)) };
    case "or":
      return { kind: "or", operands: formula.disjuncts.map(c => applyDeBruijn(c, stack)) };
    case "not":
      return { kind: "not", body: applyDeBruijn(formula.body, stack) };
    case "implies":
      return { kind: "implies", antecedent: applyDeBruijn(formula.antecedent, stack), consequent: applyDeBruijn(formula.consequent, stack) };
    case "atomic":
      return { kind: "atomic", predicate: formula.predicate, args: formula.args.map(a => applyDeBruijnTerm(a, stack)) };
  }
  throw new Error(`pass1 unknown kind ${formula.kind}`);
}

function applyDeBruijnTerm(term, stack) {
  switch (term.kind) {
    case "var": {
      const idx = stack.indexOf(term.name);
      if (idx < 0) throw new Error(`unbound variable ${term.name}`);
      return { kind: "var", index: idx, sort: term.sort };
    }
    case "const":
      return { kind: "const", value: term.value, sort: term.sort };
    case "ctor":
      return { kind: "ctor", name: term.name, args: term.args.map(a => applyDeBruijnTerm(a, stack)), sort: term.sort };
  }
  throw new Error(`pass1 unknown term kind ${term.kind}`);
}

// Pass 2: predicate canonicalization (§8.2)
export function canonicalizePredicates(node) {
  if (node.kind === "atomic") {
    let predicate = ALIAS_TABLE[node.predicate] ?? node.predicate;
    let args = node.args;
    if (["<", "≤", ">", "≥"].includes(predicate) && args.length === 2 && args[0].kind === "const" && args[1].kind !== "const") {
      predicate = FLIP_PREDICATE[predicate];
      args = [args[1], args[0]];
    }
    if (["=", "≠"].includes(predicate) && args.length === 2) {
      const a = jcsTermBytes(args[0]);
      const b = jcsTermBytes(args[1]);
      if (lexCompare(a, b) > 0) args = [args[1], args[0]];
    }
    return { kind: "atomic", predicate, args };
  }
  return mapChildren(node, canonicalizePredicates);
}

// Pass 3: sort canonicalization is identity (§8.3) for the prototype's input domain.

// Pass 4: implies removal (§8.4)
export function removeImplies(node) {
  if (node.kind === "implies") {
    return { kind: "or", operands: [{ kind: "not", body: removeImplies(node.antecedent) }, removeImplies(node.consequent)] };
  }
  return mapChildren(node, removeImplies);
}

// Pass 5: NNF (§8.5)
export function toNnf(node) {
  if (node.kind === "not") return pushNot(node.body);
  return mapChildren(node, toNnf);
}
function pushNot(inner) {
  switch (inner.kind) {
    case "not": return toNnf(inner.body);
    case "and": return { kind: "or", operands: inner.operands.map(pushNot) };
    case "or": return { kind: "and", operands: inner.operands.map(pushNot) };
    case "forall": return { kind: "exists", sort: inner.sort, body: pushNot(inner.body) };
    case "exists": return { kind: "forall", sort: inner.sort, body: pushNot(inner.body) };
    case "atomic": {
      const neg = NEGATE_PREDICATE[inner.predicate];
      if (neg) return { kind: "atomic", predicate: neg, args: inner.args };
      return { kind: "not", body: inner };
    }
  }
  throw new Error(`pushNot unknown kind ${inner.kind}`);
}

// Pass 6: AC normalize (§8.6)
const TRUE_ATOMIC = { kind: "atomic", predicate: "true", args: [] };
const FALSE_ATOMIC = { kind: "atomic", predicate: "false", args: [] };
export function acNormalize(node) {
  switch (node.kind) {
    case "forall":
    case "exists":
      return { kind: node.kind, sort: node.sort, body: acNormalize(node.body) };
    case "not":
      return { kind: "not", body: acNormalize(node.body) };
    case "atomic":
      return node;
    case "and":
      return normalizeKind("and", node.operands.map(acNormalize));
    case "or":
      return normalizeKind("or", node.operands.map(acNormalize));
  }
  throw new Error(`ac unknown kind ${node.kind}`);
}
function normalizeKind(kind, operands) {
  const flat = [];
  for (const op of operands) {
    if (op.kind === kind) flat.push(...op.operands);
    else flat.push(op);
  }
  const isAnd = kind === "and";
  if (flat.some(o => isAnd ? eqAtomic(o, FALSE_ATOMIC) : eqAtomic(o, TRUE_ATOMIC))) {
    return isAnd ? FALSE_ATOMIC : TRUE_ATOMIC;
  }
  const filtered = flat.filter(o => isAnd ? !eqAtomic(o, TRUE_ATOMIC) : !eqAtomic(o, FALSE_ATOMIC));
  if (filtered.length === 0) return isAnd ? TRUE_ATOMIC : FALSE_ATOMIC;
  if (filtered.length === 1) return filtered[0];
  const sorted = [...filtered].sort((a, b) => lexCompare(jcsBytes(a), jcsBytes(b)));
  const deduped = [];
  for (const op of sorted) {
    if (deduped.length === 0 || lexCompare(jcsBytes(deduped[deduped.length - 1]), jcsBytes(op)) !== 0) {
      deduped.push(op);
    }
  }
  if (deduped.length === 1) return deduped[0];
  return { kind, operands: deduped };
}
function eqAtomic(a, b) {
  return a.kind === "atomic" && b.kind === "atomic" && a.predicate === b.predicate && a.args.length === 0 && b.args.length === 0;
}

function mapChildren(node, f) {
  switch (node.kind) {
    case "forall":
    case "exists":
      return { kind: node.kind, sort: node.sort, body: f(node.body) };
    case "and":
      return { kind: "and", operands: node.operands.map(f) };
    case "or":
      return { kind: "or", operands: node.operands.map(f) };
    case "not":
      return { kind: "not", body: f(node.body) };
    case "atomic":
      return node;
  }
  throw new Error(`mapChildren unknown kind ${node.kind}`);
}

// Pass 7 (JCS) per §7.3. Sorted keys (Unicode code-point), no whitespace,
// minimal RFC 8785 string escapes, integers without decimal, reals WITH
// decimal point. We honor §17 alignment item 3 (Real(N.0) -> "N.0").
export function jcsBytes(value) {
  return new TextEncoder().encode(jcsString(value));
}
export function jcsTermBytes(term) {
  return jcsBytes(term);
}
function jcsString(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return jcsNumber(value, /* sortRealHint */ undefined);
  if (typeof value === "string") return jcsStringLiteral(value);
  if (typeof value === "bigint") return value.toString(10);
  if (Array.isArray(value)) return "[" + value.map(jcsString).join(",") + "]";
  if (typeof value === "object") {
    // Special-case: when serializing a Const node for Real, force decimal-point form.
    if (value.kind === "const" && value.sort?.kind === "primitive" && value.sort?.name === "Real" && typeof value.value === "number") {
      const keys = Object.keys(value).sort();
      const parts = keys.map(k => {
        if (k === "value") return JSON.stringify(k) + ":" + jcsRealLiteral(value.value);
        return JSON.stringify(k) + ":" + jcsString(value[k]);
      });
      return "{" + parts.join(",") + "}";
    }
    const keys = Object.keys(value).filter(k => value[k] !== undefined).sort();
    return "{" + keys.map(k => JSON.stringify(k) + ":" + jcsString(value[k])).join(",") + "}";
  }
  throw new Error(`jcs cannot encode ${typeof value}`);
}
function jcsNumber(n) {
  if (!Number.isFinite(n)) throw new Error("jcs rejects non-finite numbers");
  if (Number.isInteger(n)) return n.toString(10);
  return n.toString();
}
function jcsRealLiteral(n) {
  if (!Number.isFinite(n)) throw new Error("jcs rejects non-finite numbers");
  const s = n.toString();
  if (s.includes(".") || s.includes("e") || s.includes("E")) return s;
  return s + ".0";
}
function jcsStringLiteral(s) {
  // RFC 8785 §3.2.2.2 minimal escaping: only " and \ and the 0x00..0x1F range.
  let out = '"';
  for (const ch of s) {
    const code = ch.codePointAt(0);
    if (ch === '"') out += '\\"';
    else if (ch === "\\") out += "\\\\";
    else if (code < 0x20) {
      switch (ch) {
        case "\b": out += "\\b"; break;
        case "\t": out += "\\t"; break;
        case "\n": out += "\\n"; break;
        case "\f": out += "\\f"; break;
        case "\r": out += "\\r"; break;
        default: out += "\\u" + code.toString(16).padStart(4, "0").toLowerCase();
      }
    } else {
      out += ch;
    }
  }
  return out + '"';
}

function lexCompare(a, b) {
  const len = Math.min(a.length, b.length);
  for (let i = 0; i < len; i++) {
    if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1;
  }
  return a.length - b.length;
}

// Pipeline (§3) + pass 8 (§9)
export function canonicalize(formula) {
  const s1 = applyDeBruijn(formula);
  const s2 = canonicalizePredicates(s1);
  const s3 = s2; // pass 3 identity for our domain
  const s4 = removeImplies(s3);
  const s5 = toNnf(s4);
  const s6 = acNormalize(s5);
  return jcsBytes(s6);
}

export function propertyHash(formula) {
  const bytes = canonicalize(formula);
  const h = createHash("sha256").update(bytes).digest("hex");
  return "sha256:" + h.slice(0, 16);
}
