// Inline IR-JSON parser, derived from protocol/specs/2026-04-30-ir-formal-grammar.md.
// No sugar dependency. The grammar is what we follow.
//
// This implements the EBNF productions in §"Top-level production" through
// §"Sorts" of the IR formal grammar spec. It is non-strict by default
// (per §"Strict mode"): unknown predicate names and unknown primitive sort
// names are accepted.
//
// Closed-object policy (§"Determinism rules" rule 6) IS enforced even in
// non-strict mode.

const KNOWN_KEYS = {
  property: ["kind", "name", "formula"],
  bridge: ["kind", "name", "sourceSymbol", "sourceLayer", "targetContractCid", "targetLayer", "notes"],
  forall: ["kind", "sort", "predicate"],
  exists: ["kind", "sort", "predicate"],
  lambda: ["kind", "varName", "sort", "body"],
  and: ["kind", "conjuncts"],
  or: ["kind", "disjuncts"],
  not: ["kind", "body"],
  implies: ["kind", "antecedent", "consequent"],
  atomic: ["kind", "predicate", "args"],
  var: ["kind", "name", "sort"],
  const: ["kind", "value", "sort"],
  ctor: ["kind", "name", "args", "sort"],
  primitive: ["kind", "name"],
  bitvec: ["kind", "width"],
  set: ["kind", "element"],
  tuple: ["kind", "elements"],
  function: ["kind", "domain", "range"],
};

export class GrammarParseError extends Error {
  constructor(message, path, expected, actual) {
    super(`${message} at ${path}: expected ${expected}, got ${JSON.stringify(actual).slice(0, 80)}`);
    this.path = path;
    this.expected = expected;
    this.actual = actual;
  }
}

function checkKeys(node, kindKey, path) {
  const allowed = KNOWN_KEYS[kindKey];
  if (!allowed) throw new GrammarParseError("unknown node kind", path, "a known kind", kindKey);
  for (const k of Object.keys(node)) {
    if (!allowed.includes(k)) {
      // For bridge.notes (optional), present-with-undefined is also rejected: only present-with-string is valid.
      throw new GrammarParseError("extra key forbidden by closed-object policy", `${path}.${k}`, `one of [${allowed.join(", ")}]`, k);
    }
  }
}

export function parseDocument(json) {
  const arr = JSON.parse(json);
  if (!Array.isArray(arr)) throw new GrammarParseError("document must be array", "/", "array", arr);
  return arr.map((d, i) => parseDeclaration(d, `/${i}`));
}

function parseDeclaration(node, path) {
  if (node === null || typeof node !== "object") throw new GrammarParseError("declaration must be object", path, "object", node);
  if (node.kind === "property") {
    checkKeys(node, "property", path);
    return {
      kind: "property",
      name: node.name,
      formula: parseFormula(node.formula, `${path}/formula`),
    };
  }
  if (node.kind === "bridge") {
    checkKeys(node, "bridge", path);
    return { kind: "bridge", ...node };
  }
  throw new GrammarParseError("unknown declaration kind", path, "property or bridge", node.kind);
}

export function parseFormula(node, path = "") {
  if (node === null || typeof node !== "object") throw new GrammarParseError("formula must be object", path, "object", node);
  switch (node.kind) {
    case "forall":
    case "exists": {
      checkKeys(node, node.kind, path);
      return { kind: node.kind, sort: parseSort(node.sort, `${path}/sort`), predicate: parseLambda(node.predicate, `${path}/predicate`) };
    }
    case "and": {
      checkKeys(node, "and", path);
      if (!Array.isArray(node.conjuncts)) throw new GrammarParseError("and.conjuncts must be array", path, "array", node.conjuncts);
      return { kind: "and", conjuncts: node.conjuncts.map((c, i) => parseFormula(c, `${path}/conjuncts/${i}`)) };
    }
    case "or": {
      checkKeys(node, "or", path);
      return { kind: "or", disjuncts: node.disjuncts.map((c, i) => parseFormula(c, `${path}/disjuncts/${i}`)) };
    }
    case "not": {
      checkKeys(node, "not", path);
      return { kind: "not", body: parseFormula(node.body, `${path}/body`) };
    }
    case "implies": {
      checkKeys(node, "implies", path);
      return { kind: "implies", antecedent: parseFormula(node.antecedent, `${path}/antecedent`), consequent: parseFormula(node.consequent, `${path}/consequent`) };
    }
    case "atomic": {
      checkKeys(node, "atomic", path);
      return { kind: "atomic", predicate: node.predicate, args: node.args.map((a, i) => parseTerm(a, `${path}/args/${i}`)) };
    }
    default:
      throw new GrammarParseError("unknown formula kind", path, "one of [forall,exists,and,or,not,implies,atomic]", node.kind);
  }
}

function parseLambda(node, path) {
  if (node?.kind !== "lambda") throw new GrammarParseError("expected lambda", path, "lambda", node?.kind);
  checkKeys(node, "lambda", path);
  return { kind: "lambda", varName: node.varName, sort: parseSort(node.sort, `${path}/sort`), body: parseFormula(node.body, `${path}/body`) };
}

export function parseTerm(node, path) {
  if (node === null || typeof node !== "object") throw new GrammarParseError("term must be object", path, "object", node);
  switch (node.kind) {
    case "var":
      checkKeys(node, "var", path);
      return { kind: "var", name: node.name, sort: parseSort(node.sort, `${path}/sort`) };
    case "const":
      checkKeys(node, "const", path);
      return { kind: "const", value: node.value, sort: parseSort(node.sort, `${path}/sort`) };
    case "ctor":
      checkKeys(node, "ctor", path);
      return { kind: "ctor", name: node.name, args: node.args.map((a, i) => parseTerm(a, `${path}/args/${i}`)), sort: parseSort(node.sort, `${path}/sort`) };
    default:
      throw new GrammarParseError("unknown term kind", path, "one of [var,const,ctor]", node.kind);
  }
}

export function parseSort(node, path) {
  if (node === null || typeof node !== "object") throw new GrammarParseError("sort must be object", path, "object", node);
  switch (node.kind) {
    case "primitive":
      checkKeys(node, "primitive", path);
      return { kind: "primitive", name: node.name };
    case "bitvec":
      checkKeys(node, "bitvec", path);
      return { kind: "bitvec", width: node.width };
    case "set":
      checkKeys(node, "set", path);
      return { kind: "set", element: parseSort(node.element, `${path}/element`) };
    case "tuple":
      checkKeys(node, "tuple", path);
      return { kind: "tuple", elements: node.elements.map((e, i) => parseSort(e, `${path}/elements/${i}`)) };
    case "function":
      checkKeys(node, "function", path);
      return { kind: "function", domain: node.domain.map((d, i) => parseSort(d, `${path}/domain/${i}`)), range: parseSort(node.range, `${path}/range`) };
    default:
      throw new GrammarParseError("unknown sort kind", path, "one of [primitive,bitvec,set,tuple,function]", node.kind);
  }
}
