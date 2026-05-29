"use strict";

const { blake3 } = require("@noble/hashes/blake3.js");

function emit(plan) {
  const normalized = normalizePlan(plan);
  const emittedPredicates = [];
  const unsupportedPredicates = [];
  const tests = [];

  normalized.predicates.forEach((predicate, index) => {
    const head = headOf(predicate);
    const assertion = renderAssertion(head, predicate);
    if (assertion === null) {
      unsupportedPredicates.push(head || "<malformed>");
      return;
    }
    emittedPredicates.push(canonicalHead(head));
    tests.push(renderTest(head, index, declarationsFor(head, freeVars(predicate)), assertion));
  });

  const source = renderModule(normalized.functionName, tests);
  return {
    kind: "typescript-vitest-test-emission",
    source,
    path: modulePath(normalized.functionName),
    extension: "ts",
    emitted_artifact_cid: blake3Cid(source),
    emitted_predicates: emittedPredicates,
    unsupported_predicates: unsupportedPredicates,
    is_complete: unsupportedPredicates.length === 0 && emittedPredicates.length > 0,
  };
}

function normalizePlan(plan) {
  const params = isObject(plan) ? plan : {};
  return {
    contractId: firstString(params.contract_id, params.concept_name),
    functionName: firstString(params.function, params.function_name, params.functionName) || "contract",
    predicates: array(params.predicates).filter(isObject),
  };
}

function renderModule(functionName, tests) {
  const suite = stringLiteral(`provekit contract ${functionName || "contract"}`);
  const body = tests.length > 0 ? tests.join("\n\n") : "  it(\"has no emitted predicates\", () => {});";
  return `describe(${suite}, () => {\n${body}\n});\n`;
}

function renderTest(head, index, declarations, assertion) {
  const testName = stringLiteral(`verifies ${canonicalHead(head)} ${index}`);
  const lines = [`  it(${testName}, () => {`];
  declarations.forEach((decl) => lines.push(`    ${decl}`));
  assertion.split("\n").forEach((line) => lines.push(`    ${line}`));
  lines.push("  });");
  return lines.join("\n");
}

function renderAssertion(head, predicate) {
  const args = array(predicate.args).filter(isObject);
  switch (canonicalHead(head)) {
    case "eq":
      return binaryAssertion(args, "toEqual");
    case "ne":
      return binaryAssertion(args, "not.toEqual");
    case "lt":
      return binaryComparator(args, "<");
    case "gt":
      return binaryComparator(args, ">");
    case "le":
      return binaryComparator(args, "<=");
    case "ge":
      return binaryComparator(args, ">=");
    case "option-is-some":
    case "not-null":
      return unaryNotNull(args);
    case "option-is-none":
      return unaryNull(args);
    default:
      return null;
  }
}

function binaryAssertion(args, matcher) {
  if (args.length !== 2) return null;
  const left = renderTerm(args[0]);
  const right = renderTerm(args[1]);
  if (left === null || right === null) return null;
  return `expect(${left}).${matcher}(${right});`;
}

function binaryComparator(args, op) {
  if (args.length !== 2) return null;
  const left = renderTerm(args[0]);
  const right = renderTerm(args[1]);
  if (left === null || right === null) return null;
  return `expect(${left} ${op} ${right}).toBe(true);`;
}

function unaryNotNull(args) {
  if (args.length !== 1) return null;
  const value = renderTerm(args[0]);
  if (value === null) return null;
  return `expect(${value}).not.toBeNull();`;
}

function unaryNull(args) {
  if (args.length !== 1) return null;
  const value = renderTerm(args[0]);
  if (value === null) return null;
  return `expect(${value}).toBeNull();`;
}

function declarationsFor(head, vars) {
  return vars.map((name, index) => {
    const ident = sanitizeIdentifier(name, `v${index}`);
    return `const ${ident} = ${placeholderValue(head, index)};`;
  });
}

function placeholderValue(head, index) {
  switch (canonicalHead(head)) {
    case "lt":
      return index === 0 ? "0" : "1";
    case "gt":
      return index === 0 ? "1" : "0";
    case "le":
    case "ge":
    case "eq":
      return "0";
    case "ne":
      return index === 0 ? "0" : "1";
    case "option-is-none":
      return "null";
    case "option-is-some":
    case "not-null":
      return "1";
    default:
      return "0";
  }
}

function renderTerm(term) {
  if (!isObject(term)) return null;
  const kind = firstString(term.kind);
  if (kind === "var") {
    return sanitizeIdentifier(firstString(term.name), "");
  }
  if (kind === "const") {
    return renderConst(term.value);
  }
  if (kind === "op" || kind === "ctor") {
    return renderApplication(term);
  }
  return null;
}

function renderConst(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (typeof value === "string") return JSON.stringify(value);
  return null;
}

function renderApplication(term) {
  const name = firstString(term.name, term.conceptName, term.concept_name);
  const args = array(term.args).filter(isObject);
  const rendered = args.map(renderTerm);
  if (rendered.some((value) => value === null)) return null;
  if (["+", "-", "*", "/", "%"].includes(name) && rendered.length === 2) {
    return `(${rendered[0]} ${name} ${rendered[1]})`;
  }
  if (name === "array") return `[${rendered.join(", ")}]`;
  if (name === "tuple") return `[${rendered.join(", ")}]`;
  const callee = sanitizeIdentifier(name.replace(/^concept:/, ""), "");
  if (callee === "") return null;
  return `${callee}(${rendered.join(", ")})`;
}

function freeVars(predicate) {
  const out = [];
  collectFreeVars(predicate, out);
  return out;
}

function collectFreeVars(value, out) {
  if (Array.isArray(value)) {
    value.forEach((item) => collectFreeVars(item, out));
    return;
  }
  if (!isObject(value)) return;
  if (value.kind === "var") {
    const name = firstString(value.name);
    if (name !== "" && !out.includes(name)) out.push(name);
  }
  collectFreeVars(value.args, out);
}

function headOf(predicate) {
  if (!isObject(predicate)) return "";
  return firstString(predicate.name, predicate.concept_name, predicate.conceptName);
}

function canonicalHead(head) {
  const normalized = firstString(head).replace(/^concept:/, "").replace(/_/g, "-");
  switch (normalized) {
    case "=":
      return "eq";
    case "!=":
    case "\u2260":
      return "ne";
    case "<":
      return "lt";
    case ">":
      return "gt";
    case "<=":
    case "\u2264":
      return "le";
    case ">=":
    case "\u2265":
      return "ge";
    default:
      return normalized;
  }
}

function modulePath(functionName) {
  const safe = firstString(functionName).replace(/[^0-9A-Za-z_]+/g, "_").replace(/^_+|_+$/g, "").toLowerCase();
  return `provekit_${safe || "contract"}.test.ts`;
}

function sanitizeIdentifier(value, fallback) {
  const raw = firstString(value).replace(/-/g, "_").replace(/[^0-9A-Za-z_$]/g, "_");
  const candidate = raw.replace(/^[^A-Za-z_$]+/, "");
  return /^[A-Za-z_$][0-9A-Za-z_$]*$/.test(candidate) ? candidate : fallback;
}

function stringLiteral(value) {
  return JSON.stringify(String(value));
}

function blake3Cid(source) {
  const digest = blake3(Buffer.from(source, "utf8"), { dkLen: 64 });
  return `blake3-512:${Buffer.from(digest).toString("hex")}`;
}

function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim() !== "") return value.trim();
  }
  return "";
}

function array(value) {
  return Array.isArray(value) ? value : [];
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

module.exports = {
  emit,
  normalizePlan,
};
