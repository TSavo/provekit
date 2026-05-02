/**
 * Reference parser + emitter for the IR's external JSON encoding.
 *
 * Spec: protocol/specs/2026-04-30-ir-formal-grammar.md
 *
 * The parser ingests JSON conforming to the maximal-uniformity grammar
 * and produces typed IR values (IrFormula, IrTerm, Sort, Declaration).
 * The emitter takes typed IR values and produces canonical JSON that
 * matches the spec's locked key-order rules. The pair satisfies a
 * fixed-point property:
 *
 *   emit(parseDocument(text)) === text   (when text is grammar-conformant)
 *   parseDocument(emit(value))           is structurally equal to value
 *
 * Strict mode additionally enforces emit-order key sequencing during parse;
 * non-strict mode accepts any key order at parse time.
 */

import type {
  AtomicPredicate,
  EvidenceTerm,
  IrFormula,
  IrTerm,
  PrimitiveSortName,
  Sort,
} from "../formulas.js";
import type { Declaration } from "../symbolic/property.js";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class GrammarParseError extends Error {
  readonly path: string;
  readonly expected: string;
  readonly actual: unknown;

  constructor(opts: { path: string; expected: string; actual: unknown }) {
    super(
      `at ${opts.path}: expected ${opts.expected}, got ${stringifyForError(opts.actual)}`,
    );
    this.name = "GrammarParseError";
    this.path = opts.path;
    this.expected = opts.expected;
    this.actual = opts.actual;
  }
}

function stringifyForError(value: unknown): string {
  try {
    const s = JSON.stringify(value);
    if (s === undefined) return String(value);
    return s.length > 120 ? `${s.slice(0, 117)}...` : s;
  } catch {
    return String(value);
  }
}

// ---------------------------------------------------------------------------
// Locked key orders (must match the spec doc and every kit's emit order)
// ---------------------------------------------------------------------------

const CONTRACT_DECL_REQUIRED_KEYS = ["kind", "name", "outBinding"] as const;
const CONTRACT_DECL_OPTIONAL_KEYS = ["pre", "post", "inv", "evidence"] as const;
const BRIDGE_DECL_REQUIRED_KEYS = [
  "kind",
  "name",
  "sourceSymbol",
  "sourceLayer",
  "targetContractCid",
  "targetLayer",
] as const;
const BRIDGE_DECL_OPTIONAL_KEYS = ["notes"] as const;

const QUANTIFIER_KEYS = ["kind", "name", "sort", "body"] as const;
const CONNECTIVE_KEYS = ["kind", "operands"] as const;
const ATOMIC_KEYS = ["kind", "name", "args"] as const;

const VAR_TERM_KEYS = ["kind", "name"] as const;
const CONST_TERM_KEYS = ["kind", "value", "sort"] as const;
const CTOR_TERM_KEYS = ["kind", "name", "args"] as const;
const LAMBDA_TERM_KEYS = ["kind", "paramName", "paramSort", "body"] as const;
const LET_TERM_KEYS = ["kind", "bindings", "body"] as const;
const LET_BINDING_KEYS = ["name", "boundTerm"] as const;

const CHOICE_KEYS = ["kind", "varName", "sort", "body"] as const;

const PRIMITIVE_SORT_KEYS = ["kind", "name"] as const;
const BITVEC_SORT_KEYS = ["kind", "width"] as const;
const SET_SORT_KEYS = ["kind", "element"] as const;
const TUPLE_SORT_KEYS = ["kind", "elements"] as const;
const FUNCTION_SORT_KEYS = ["kind", "domain", "range"] as const;

const CANONICAL_PRIMITIVE_SORTS: ReadonlySet<string> = new Set([
  "Bool",
  "Int",
  "Real",
  "String",
  "Ref",
  "Node",
  "Edge",
  "Region",
  "Time",
]);

const CANONICAL_PREDICATES: ReadonlySet<string> = new Set([
  "=",
  "≠",
  "<",
  "≤",
  ">",
  "≥",
  "true",
  "false",
  "subset",
  "member",
  "kind-of",
  "data-flows-to",
  "dominates",
  "post-dominates",
  "transition-from-to",
  "on-path",
  "bvult",
  "bvule",
  "bvugt",
  "bvuge",
  "bvslt",
  "bvsle",
  "bvsgt",
  "bvsge",
]);

const KIT_EXTENSION_PREDICATE_RE = /^[a-zA-Z_][a-zA-Z0-9_-]*$/;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export interface ParseOptions {
  /** When true, enforce locked key order and locked predicate/sort name vocab. */
  strict?: boolean;
}

export function parseDocument(json: string, opts: ParseOptions = {}): Declaration[] {
  const value = jsonParse(json);
  return parseDocumentValue(value, "", opts);
}

export function parseFormula(json: string, opts: ParseOptions = {}): IrFormula {
  const value = jsonParse(json);
  return parseFormulaValue(value, "", opts);
}

export function parseTerm(json: string, opts: ParseOptions = {}): IrTerm {
  const value = jsonParse(json);
  return parseTermValue(value, "", opts);
}

export function parseSort(json: string, opts: ParseOptions = {}): Sort {
  const value = jsonParse(json);
  return parseSortValue(value, "", opts);
}

/**
 * Emit a Declaration[] back to canonical JSON matching the locked key order.
 * The output has no extraneous whitespace, matching the kit-emit form.
 */
export function emitDocument(decls: Declaration[]): string {
  const buf: string[] = ["["];
  for (let i = 0; i < decls.length; i++) {
    if (i > 0) buf.push(",");
    buf.push(emitDeclaration(decls[i]!));
  }
  buf.push("]");
  return buf.join("");
}

// ---------------------------------------------------------------------------
// Internal: low-level JSON parse with NaN/Infinity guard
// ---------------------------------------------------------------------------

function jsonParse(json: string): unknown {
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch (e) {
    throw new GrammarParseError({
      path: "",
      expected: "well-formed JSON",
      actual: (e as Error).message,
    });
  }
  return value;
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

function parseDocumentValue(value: unknown, path: string, opts: ParseOptions): Declaration[] {
  if (!Array.isArray(value)) {
    throw new GrammarParseError({ path, expected: "JSON array of declarations", actual: value });
  }
  return value.map((entry, i) => parseDeclarationValue(entry, `${path}/${i}`, opts));
}

function parseDeclarationValue(
  value: unknown,
  path: string,
  opts: ParseOptions,
): Declaration {
  const obj = expectObject(value, path);
  const kind = expectKindString(obj, path);
  switch (kind) {
    case "contract":
      return parseContractDeclaration(obj, path, opts);
    case "bridge":
      return parseBridgeDeclaration(obj, path, opts);
    default:
      throw new GrammarParseError({
        path: `${path}/kind`,
        expected: '"contract" | "bridge"',
        actual: kind,
      });
  }
}

function parseContractDeclaration(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Declaration {
  enforceClosedKeys(obj, path, CONTRACT_DECL_REQUIRED_KEYS, CONTRACT_DECL_OPTIONAL_KEYS);
  if (opts.strict) {
    const observed = Object.keys(obj);
    const expected = ["kind", "name", "outBinding"] as string[];
    if (observed.includes("pre")) expected.push("pre");
    if (observed.includes("post")) expected.push("post");
    if (observed.includes("inv")) expected.push("inv");
    if (observed.includes("evidence")) expected.push("evidence");
    if (observed.join(",") !== expected.join(",")) {
      throw new GrammarParseError({
        path,
        expected: `keys in order [${expected.join(", ")}]`,
        actual: observed,
      });
    }
  }
  const name = expectString(obj["name"], `${path}/name`, "string contract name");
  const outBinding = expectString(obj["outBinding"], `${path}/outBinding`, "string outBinding");
  const decl: Declaration = { kind: "contract", name, outBinding };
  if ("pre" in obj) {
    decl.pre = parseFormulaValue(obj["pre"], `${path}/pre`, opts);
  }
  if ("post" in obj) {
    decl.post = parseFormulaValue(obj["post"], `${path}/post`, opts);
  }
  if ("inv" in obj) {
    decl.inv = parseFormulaValue(obj["inv"], `${path}/inv`, opts);
  }
  if ("evidence" in obj) {
    decl.evidence = parseEvidenceValue(obj["evidence"], `${path}/evidence`, opts);
  }
  if (decl.pre === undefined && decl.post === undefined && decl.inv === undefined) {
    throw new GrammarParseError({
      path,
      expected: "at least one of pre/post/inv",
      actual: "none",
    });
  }
  return decl;
}

function parseBridgeDeclaration(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Declaration {
  enforceClosedKeys(obj, path, BRIDGE_DECL_REQUIRED_KEYS, BRIDGE_DECL_OPTIONAL_KEYS);
  if (opts.strict) {
    const observed = Object.keys(obj);
    const expected = [
      ...BRIDGE_DECL_REQUIRED_KEYS,
      ...(observed.includes("notes") ? (["notes"] as const) : ([] as const)),
    ];
    if (observed.join(",") !== expected.join(",")) {
      throw new GrammarParseError({
        path,
        expected: `keys in order [${expected.join(", ")}]`,
        actual: observed,
      });
    }
  }
  const name = expectString(obj["name"], `${path}/name`, "string bridge name");
  const sourceSymbol = expectString(
    obj["sourceSymbol"],
    `${path}/sourceSymbol`,
    "string sourceSymbol",
  );
  const sourceLayer = expectString(
    obj["sourceLayer"],
    `${path}/sourceLayer`,
    "string sourceLayer",
  );
  const targetContractCid = expectString(
    obj["targetContractCid"],
    `${path}/targetContractCid`,
    "string targetContractCid",
  );
  const targetLayer = expectString(
    obj["targetLayer"],
    `${path}/targetLayer`,
    "string targetLayer",
  );
  const decl: Declaration = {
    kind: "bridge",
    name,
    sourceSymbol,
    sourceLayer,
    targetContractCid,
    targetLayer,
  };
  if ("notes" in obj) {
    decl.notes = expectString(obj["notes"], `${path}/notes`, "string notes");
  }
  return decl;
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

function parseEvidenceValue(value: unknown, path: string, _opts: ParseOptions): EvidenceTerm {
  const obj = expectObject(value, path);
  enforceClosedKeys(obj, path, ["kind", "proofType", "certificate"], []);
  const kind = expectKindString(obj, path);
  if (kind !== "evidence") {
    throw new GrammarParseError({
      path: `${path}/kind`,
      expected: '"evidence"',
      actual: kind,
    });
  }
  const proofType = expectString(obj["proofType"], `${path}/proofType`, "proofType string");
  const certObj = expectObject(obj["certificate"], `${path}/certificate`);
  enforceClosedKeys(certObj, `${path}/certificate`, ["tool", "version", "formulaHash", "proofData"], []);
  const certificate = {
    tool: expectString(certObj["tool"], `${path}/certificate/tool`, "tool string"),
    version: expectString(certObj["version"], `${path}/certificate/version`, "version string"),
    formulaHash: expectString(certObj["formulaHash"], `${path}/certificate/formulaHash`, "formulaHash string"),
    proofData: expectString(certObj["proofData"], `${path}/certificate/proofData`, "proofData string"),
  };
  return { kind: "evidence", proofType: proofType as EvidenceTerm["proofType"], certificate };
}

// ---------------------------------------------------------------------------
// Formulas
// ---------------------------------------------------------------------------

function parseFormulaValue(value: unknown, path: string, opts: ParseOptions): IrFormula {
  const obj = expectObject(value, path);
  const kind = expectKindString(obj, path);
  switch (kind) {
    case "forall":
    case "exists":
      return parseQuantifiedFormula(obj, path, opts, kind);
    case "and":
    case "or":
    case "not":
    case "implies":
      return parseConnectiveFormula(obj, path, opts, kind);
    case "atomic":
      return parseAtomicFormula(obj, path, opts);
    case "choice":
      return parseChoiceFormula(obj, path, opts);
    default:
      throw new GrammarParseError({
        path: `${path}/kind`,
        expected:
          '"forall" | "exists" | "and" | "or" | "not" | "implies" | "atomic" | "choice"',
        actual: kind,
      });
  }
}

function parseQuantifiedFormula(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
  kind: "forall" | "exists",
): IrFormula {
  enforceClosedKeys(obj, path, QUANTIFIER_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, QUANTIFIER_KEYS);
  const name = expectString(obj["name"], `${path}/name`, "string quantifier var name");
  const sort = parseSortValue(obj["sort"], `${path}/sort`, opts);
  const body = parseFormulaValue(obj["body"], `${path}/body`, opts);
  return { kind, name, sort, body };
}

function parseConnectiveFormula(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
  kind: "and" | "or" | "not" | "implies",
): IrFormula {
  enforceClosedKeys(obj, path, CONNECTIVE_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, CONNECTIVE_KEYS);
  const operands = expectArray(obj["operands"], `${path}/operands`).map((o, i) =>
    parseFormulaValue(o, `${path}/operands/${i}`, opts),
  );
  if (kind === "not" && operands.length !== 1) {
    throw new GrammarParseError({
      path: `${path}/operands`,
      expected: "exactly 1 operand for not",
      actual: operands.length,
    });
  }
  if (kind === "implies" && operands.length !== 2) {
    throw new GrammarParseError({
      path: `${path}/operands`,
      expected: "exactly 2 operands for implies",
      actual: operands.length,
    });
  }
  return { kind, operands };
}

function parseAtomicFormula(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrFormula {
  enforceClosedKeys(obj, path, ATOMIC_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, ATOMIC_KEYS);
  const name = expectString(
    obj["name"],
    `${path}/name`,
    "string predicate name",
  );
  if (opts.strict && !isAcceptedPredicate(name)) {
    throw new GrammarParseError({
      path: `${path}/name`,
      expected:
        "canonical predicate name or kit-extension matching /^[a-zA-Z_][a-zA-Z0-9_-]*$/",
      actual: name,
    });
  }
  const args = expectArray(obj["args"], `${path}/args`).map((a, i) =>
    parseTermValue(a, `${path}/args/${i}`, opts),
  );
  return { kind: "atomic", name: name as AtomicPredicate, args };
}

function parseChoiceFormula(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrFormula {
  enforceClosedKeys(obj, path, CHOICE_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, CHOICE_KEYS);
  const varName = expectString(obj["varName"], `${path}/varName`, "string choice var name");
  const sort = parseSortValue(obj["sort"], `${path}/sort`, opts);
  const body = parseFormulaValue(obj["body"], `${path}/body`, opts);
  return { kind: "choice", varName, sort, body };
}

// ---------------------------------------------------------------------------
// Terms
// ---------------------------------------------------------------------------

function parseTermValue(value: unknown, path: string, opts: ParseOptions): IrTerm {
  const obj = expectObject(value, path);
  const kind = expectKindString(obj, path);
  switch (kind) {
    case "var":
      return parseVarTerm(obj, path, opts);
    case "const":
      return parseConstTerm(obj, path, opts);
    case "ctor":
      return parseCtorTerm(obj, path, opts);
    case "lambda":
      return parseLambdaTerm(obj, path, opts);
    case "let":
      return parseLetTerm(obj, path, opts);
    default:
      throw new GrammarParseError({
        path: `${path}/kind`,
        expected: '"var" | "const" | "ctor" | "lambda" | "let"',
        actual: kind,
      });
  }
}

function parseVarTerm(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrTerm {
  enforceClosedKeys(obj, path, VAR_TERM_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, VAR_TERM_KEYS);
  const name = expectString(obj["name"], `${path}/name`, "string var name");
  return { kind: "var", name };
}

function parseConstTerm(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrTerm {
  enforceClosedKeys(obj, path, CONST_TERM_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, CONST_TERM_KEYS);
  const value = obj["value"];
  if (!isAcceptedConstValue(value)) {
    throw new GrammarParseError({
      path: `${path}/value`,
      expected: "JSON Number, String, Boolean, or Null",
      actual: value,
    });
  }
  if (typeof value === "number" && !Number.isFinite(value)) {
    throw new GrammarParseError({
      path: `${path}/value`,
      expected: "finite Number (NaN/Infinity rejected)",
      actual: value,
    });
  }
  const sort = parseSortValue(obj["sort"], `${path}/sort`, opts);
  return { kind: "const", value, sort };
}

function parseCtorTerm(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrTerm {
  enforceClosedKeys(obj, path, CTOR_TERM_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, CTOR_TERM_KEYS);
  const name = expectString(obj["name"], `${path}/name`, "string ctor name");
  const args = expectArray(obj["args"], `${path}/args`).map((a, i) =>
    parseTermValue(a, `${path}/args/${i}`, opts),
  );
  return { kind: "ctor", name, args };
}

function parseLambdaTerm(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrTerm {
  enforceClosedKeys(obj, path, LAMBDA_TERM_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, LAMBDA_TERM_KEYS);
  const paramName = expectString(obj["paramName"], `${path}/paramName`, "string lambda param name");
  const paramSort = parseSortValue(obj["paramSort"], `${path}/paramSort`, opts);
  const body = parseTermValue(obj["body"], `${path}/body`, opts);
  return { kind: "lambda", paramName, paramSort, body };
}

function parseLetTerm(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): IrTerm {
  enforceClosedKeys(obj, path, LET_TERM_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, LET_TERM_KEYS);
  const bindings = expectArray(obj["bindings"], `${path}/bindings`).map((b, i) => {
    const bObj = expectObject(b, `${path}/bindings/${i}`);
    enforceClosedKeys(bObj, `${path}/bindings/${i}`, LET_BINDING_KEYS, []);
    const name = expectString(bObj["name"], `${path}/bindings/${i}/name`, "string binding name");
    const boundTerm = parseTermValue(bObj["boundTerm"], `${path}/bindings/${i}/boundTerm`, opts);
    return { name, boundTerm };
  });
  const body = parseTermValue(obj["body"], `${path}/body`, opts);
  return { kind: "let", bindings, body };
}

// ---------------------------------------------------------------------------
// Sorts
// ---------------------------------------------------------------------------

function parseSortValue(value: unknown, path: string, opts: ParseOptions): Sort {
  const obj = expectObject(value, path);
  const kind = expectKindString(obj, path);
  switch (kind) {
    case "primitive":
      return parsePrimitiveSort(obj, path, opts);
    case "bitvec":
      return parseBitvecSort(obj, path, opts);
    case "set":
      return parseSetSort(obj, path, opts);
    case "tuple":
      return parseTupleSort(obj, path, opts);
    case "function":
      return parseFunctionSort(obj, path, opts);
    default:
      throw new GrammarParseError({
        path: `${path}/kind`,
        expected: '"primitive" | "bitvec" | "set" | "tuple" | "function"',
        actual: kind,
      });
  }
}

function parsePrimitiveSort(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Sort {
  enforceClosedKeys(obj, path, PRIMITIVE_SORT_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, PRIMITIVE_SORT_KEYS);
  const name = expectString(obj["name"], `${path}/name`, "string primitive sort name");
  if (opts.strict && !CANONICAL_PRIMITIVE_SORTS.has(name)) {
    throw new GrammarParseError({
      path: `${path}/name`,
      expected: `one of [${[...CANONICAL_PRIMITIVE_SORTS].join(", ")}]`,
      actual: name,
    });
  }
  return { kind: "primitive", name: name as PrimitiveSortName };
}

function parseBitvecSort(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Sort {
  enforceClosedKeys(obj, path, BITVEC_SORT_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, BITVEC_SORT_KEYS);
  const width = obj["width"];
  if (
    typeof width !== "number" ||
    !Number.isFinite(width) ||
    !Number.isInteger(width) ||
    width <= 0
  ) {
    throw new GrammarParseError({
      path: `${path}/width`,
      expected: "positive integer",
      actual: width,
    });
  }
  return { kind: "bitvec", width };
}

function parseSetSort(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Sort {
  enforceClosedKeys(obj, path, SET_SORT_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, SET_SORT_KEYS);
  const element = parseSortValue(obj["element"], `${path}/element`, opts);
  return { kind: "set", element };
}

function parseTupleSort(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Sort {
  enforceClosedKeys(obj, path, TUPLE_SORT_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, TUPLE_SORT_KEYS);
  const elements = expectArray(obj["elements"], `${path}/elements`).map((e, i) =>
    parseSortValue(e, `${path}/elements/${i}`, opts),
  );
  return { kind: "tuple", elements };
}

function parseFunctionSort(
  obj: Record<string, unknown>,
  path: string,
  opts: ParseOptions,
): Sort {
  enforceClosedKeys(obj, path, FUNCTION_SORT_KEYS, []);
  if (opts.strict) enforceKeyOrder(obj, path, FUNCTION_SORT_KEYS);
  const domain = expectArray(obj["domain"], `${path}/domain`).map((d, i) =>
    parseSortValue(d, `${path}/domain/${i}`, opts),
  );
  const range = parseSortValue(obj["range"], `${path}/range`, opts);
  return { kind: "function", domain, range };
}

// ---------------------------------------------------------------------------
// Helpers — type-guard wrappers that throw GrammarParseError
// ---------------------------------------------------------------------------

function expectObject(value: unknown, path: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new GrammarParseError({ path, expected: "JSON object", actual: value });
  }
  return value as Record<string, unknown>;
}

function expectArray(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new GrammarParseError({ path, expected: "JSON array", actual: value });
  }
  return value;
}

function expectString(value: unknown, path: string, what: string): string {
  if (typeof value !== "string") {
    throw new GrammarParseError({ path, expected: what, actual: value });
  }
  return value;
}

function expectKindString(obj: Record<string, unknown>, path: string): string {
  if (!("kind" in obj)) {
    throw new GrammarParseError({ path, expected: 'object with "kind"', actual: obj });
  }
  const kind = obj["kind"];
  if (typeof kind !== "string") {
    throw new GrammarParseError({
      path: `${path}/kind`,
      expected: "string discriminator",
      actual: kind,
    });
  }
  return kind;
}

function enforceClosedKeys(
  obj: Record<string, unknown>,
  path: string,
  required: readonly string[],
  optional: readonly string[],
): void {
  const allowed = new Set([...required, ...optional]);
  for (const k of Object.keys(obj)) {
    if (!allowed.has(k)) {
      throw new GrammarParseError({
        path: `${path}/${k}`,
        expected: `one of [${[...allowed].join(", ")}]`,
        actual: `unexpected key "${k}"`,
      });
    }
  }
  for (const k of required) {
    if (!(k in obj)) {
      throw new GrammarParseError({
        path,
        expected: `required key "${k}"`,
        actual: `keys [${Object.keys(obj).join(", ")}]`,
      });
    }
  }
}

function enforceKeyOrder(
  obj: Record<string, unknown>,
  path: string,
  expected: readonly string[],
): void {
  const observed = Object.keys(obj);
  if (observed.length !== expected.length) {
    throw new GrammarParseError({
      path,
      expected: `keys in order [${expected.join(", ")}]`,
      actual: observed,
    });
  }
  for (let i = 0; i < expected.length; i++) {
    if (observed[i] !== expected[i]) {
      throw new GrammarParseError({
        path,
        expected: `keys in order [${expected.join(", ")}]`,
        actual: observed,
      });
    }
  }
}

function isAcceptedConstValue(v: unknown): v is number | string | boolean | null {
  return (
    v === null ||
    typeof v === "number" ||
    typeof v === "string" ||
    typeof v === "boolean"
  );
}

function isAcceptedPredicate(name: string): boolean {
  if (CANONICAL_PREDICATES.has(name)) return true;
  return KIT_EXTENSION_PREDICATE_RE.test(name);
}

// ---------------------------------------------------------------------------
// Emit
// ---------------------------------------------------------------------------

function emitDeclaration(decl: Declaration): string {
  if (decl.kind === "contract") {
    let out =
      "{" +
      `"kind":"contract",` +
      `"name":${JSON.stringify(decl.name)},` +
      `"outBinding":${JSON.stringify(decl.outBinding)}`;
    if (decl.pre !== undefined) {
      out += `,"pre":${emitFormula(decl.pre)}`;
    }
    if (decl.post !== undefined) {
      out += `,"post":${emitFormula(decl.post)}`;
    }
    if (decl.inv !== undefined) {
      out += `,"inv":${emitFormula(decl.inv)}`;
    }
    if (decl.evidence !== undefined) {
      out += `,"evidence":${emitEvidence(decl.evidence)}`;
    }
    out += "}";
    return out;
  }
  let out =
    "{" +
    `"kind":"bridge",` +
    `"name":${JSON.stringify(decl.name)},` +
    `"sourceSymbol":${JSON.stringify(decl.sourceSymbol)},` +
    `"sourceLayer":${JSON.stringify(decl.sourceLayer)},` +
    `"targetContractCid":${JSON.stringify(decl.targetContractCid)},` +
    `"targetLayer":${JSON.stringify(decl.targetLayer)}`;
  if (decl.notes !== undefined) {
    out += `,"notes":${JSON.stringify(decl.notes)}`;
  }
  out += "}";
  return out;
}

export function emitFormula(f: IrFormula): string {
  switch (f.kind) {
    case "forall":
    case "exists":
      return (
        "{" +
        `"kind":"${f.kind}",` +
        `"name":${JSON.stringify(f.name)},` +
        `"sort":${emitSort(f.sort)},` +
        `"body":${emitFormula(f.body)}` +
        "}"
      );
    case "and":
    case "or":
    case "not":
    case "implies":
      return (
        "{" +
        `"kind":"${f.kind}",` +
        `"operands":[${f.operands.map(emitFormula).join(",")}]` +
        "}"
      );
    case "atomic":
      return (
        "{" +
        `"kind":"atomic",` +
        `"name":${JSON.stringify(f.name)},` +
        `"args":[${f.args.map(emitTerm).join(",")}]` +
        "}"
      );
    case "choice":
      return (
        "{" +
        `"kind":"choice",` +
        `"varName":${JSON.stringify(f.varName)},` +
        `"sort":${emitSort(f.sort)},` +
        `"body":${emitFormula(f.body)}` +
        "}"
      );
  }
}

function emitEvidence(e: EvidenceTerm): string {
  return (
    "{" +
    `"kind":"evidence",` +
    `"proofType":${JSON.stringify(e.proofType)},` +
    `"certificate":{` +
    `"tool":${JSON.stringify(e.certificate.tool)},` +
    `"version":${JSON.stringify(e.certificate.version)},` +
    `"formulaHash":${JSON.stringify(e.certificate.formulaHash)},` +
    `"proofData":${JSON.stringify(e.certificate.proofData)}` +
    "}}"
  );
}

export function emitTerm(t: IrTerm): string {
  switch (t.kind) {
    case "var":
      return (
        "{" +
        `"kind":"var",` +
        `"name":${JSON.stringify(t.name)}` +
        "}"
      );
    case "const":
      return (
        "{" +
        `"kind":"const",` +
        `"value":${emitConstValue(t.value)},` +
        `"sort":${emitSort(t.sort)}` +
        "}"
      );
    case "ctor":
      return (
        "{" +
        `"kind":"ctor",` +
        `"name":${JSON.stringify(t.name)},` +
        `"args":[${t.args.map(emitTerm).join(",")}]` +
        "}"
      );
    case "lambda":
      return (
        "{" +
        `"kind":"lambda",` +
        `"paramName":${JSON.stringify(t.paramName)},` +
        `"sort":${emitSort(t.paramSort)},` +
        `"body":${emitTerm(t.body)}` +
        "}"
      );
    case "let":
      return (
        "{" +
        `"kind":"let",` +
        `"bindings":[${t.bindings.map(b => `{"name":${JSON.stringify(b.name)},"boundTerm":${emitTerm(b.boundTerm)}}`).join(",")}],` +
        `"body":${emitTerm(t.body)}` +
        "}"
      );
  }
}

function emitConstValue(v: unknown): string {
  if (v === null) return "null";
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "number") {
    if (!Number.isFinite(v)) {
      throw new GrammarParseError({
        path: "<const>",
        expected: "finite number",
        actual: v,
      });
    }
    return JSON.stringify(v);
  }
  if (typeof v === "string") return JSON.stringify(v);
  throw new GrammarParseError({
    path: "<const>",
    expected: "JSON Number, String, Boolean, or Null",
    actual: v,
  });
}

export function emitSort(s: Sort): string {
  switch (s.kind) {
    case "primitive":
      return "{" + `"kind":"primitive",` + `"name":${JSON.stringify(s.name)}` + "}";
    case "bitvec":
      return "{" + `"kind":"bitvec",` + `"width":${s.width}` + "}";
    case "set":
      return "{" + `"kind":"set",` + `"element":${emitSort(s.element)}` + "}";
    case "tuple":
      return (
        "{" +
        `"kind":"tuple",` +
        `"elements":[${s.elements.map(emitSort).join(",")}]` +
        "}"
      );
    case "function":
      return (
        "{" +
        `"kind":"function",` +
        `"domain":[${s.domain.map(emitSort).join(",")}],` +
        `"range":${emitSort(s.range)}` +
        "}"
      );
  }
}
