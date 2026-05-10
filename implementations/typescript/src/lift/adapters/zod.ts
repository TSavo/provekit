/**
 * provekit-lift adapter: zod.
 *
 * Walks `z.object({...})` / `z.string()...` / `z.number()...` schema
 * declarations and lifts each top-level schema to a contract memento.
 *
 * Strategic positioning: we do NOT replace zod. Developers keep their
 * existing zod schemas. This adapter reads them and produces a signed
 * `<cid>.proof` so the schema becomes content-addressed and shippable
 * across kit boundaries.
 *
 * SHAPE (v0):
 *   const X = z.object({ field: z.<chain>... });
 *
 *   lifts to a ContractDecl named X with:
 *     pre: forall x: Ref. and(<per-field constraint>, ...)
 *
 * Each field's chain becomes a list of atomic terms:
 *   z.string()                       -> kind-of(x.field, "String")
 *   z.string().min(N)                -> > (length(x.field), N - 1)
 *   z.string().max(N)                -> < (length(x.field), N + 1)
 *   z.string().email()               -> matches(x.field, "^.+@.+$")  (Ctor)
 *   z.string().uuid()                -> is_uuid(x.field)              (Ctor)
 *   z.string().regex(r)              -> matches(x.field, <r-source>)  (Ctor)
 *   z.number()                       -> kind-of(x.field, "Real")
 *   z.number().int()                 -> is_int(x.field)               (Ctor)
 *   z.number().nonnegative()         -> >= (x.field, 0)
 *   z.number().positive()            -> >  (x.field, 0)
 *   z.number().nonpositive()         -> <= (x.field, 0)
 *   z.number().negative()            -> <  (x.field, 0)
 *   z.number().min(N)                -> >= (x.field, N)
 *   z.number().max(N)                -> <= (x.field, N)
 *   z.boolean()                      -> kind-of(x.field, "Bool")
 *
 * Top-level schemas that are not z.object(...) (e.g., a bare
 * z.string().email()) are also lifted: the contract's pre is the
 * conjunction of constraints applied to a single binding `x`.
 *
 * Refinements with arbitrary callbacks (z.string().refine(fn)) are
 * SKIPPED with a warning: we cannot lift unknown predicate semantics
 * to canonical IR without polluting the lattice.
 */

import ts from "typescript";
import type {
  IrFormula,
  IrTerm,
  AtomicFormula,
} from "../../ir/formulas.js";
import { Int, Real, String as StringSort, Bool, Ref } from "../../ir/sorts.js";
import type { Sort } from "../../ir/formulas.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "zod";

/** Public adapter entry point: mirrors the Rust adapter's `lift_file`. */
export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  let seen = 0;

  ts.forEachChild(sourceFile, (node) => {
    const candidate = extractZodCandidate(node);
    if (!candidate) return;
    seen += 1;

    const liftResult = liftSchema(candidate.name, candidate.expr, sourcePath);
    if (liftResult.kind === "ok") {
      decls.push(liftResult.decl);
    } else {
      warnings.push({
        adapter: ADAPTER,
        sourcePath,
        itemName: candidate.name,
        reason: liftResult.reason,
      });
    }
  });

  return { decls, seen, lifted: decls.length, warnings };
}

interface ZodCandidate {
  name: string;
  expr: ts.Expression;
}

/**
 * A "candidate" for the zod adapter is any top-level
 * `const|let|var <Name> = <expr>;` whose RHS root call is z.<something>.
 * We pre-screen by syntactic shape only: the lift step does the actual
 * chain decoding.
 */
function extractZodCandidate(node: ts.Node): ZodCandidate | null {
  if (!ts.isVariableStatement(node)) return null;
  for (const decl of node.declarationList.declarations) {
    if (!ts.isIdentifier(decl.name)) continue;
    if (!decl.initializer) continue;
    if (!isZodRoot(decl.initializer)) continue;
    return { name: decl.name.text, expr: decl.initializer };
  }
  return null;
}

/** Walk to the leftmost call target and check if it's `z.<method>`. */
function isZodRoot(expr: ts.Expression): boolean {
  let cur: ts.Expression = expr;
  while (true) {
    if (ts.isCallExpression(cur)) {
      cur = cur.expression;
      continue;
    }
    if (ts.isPropertyAccessExpression(cur)) {
      cur = cur.expression;
      continue;
    }
    break;
  }
  return ts.isIdentifier(cur) && cur.text === "z";
}

type LiftResult =
  | { kind: "ok"; decl: ContractDecl }
  | { kind: "skip"; reason: string };

function liftSchema(name: string, expr: ts.Expression, sourcePath: string): LiftResult {
  // z.object({...}): lift each field as a clause over u.<field>.
  const objectFields = extractZObjectFields(expr);
  if (objectFields !== null) {
    if (objectFields === "skip-refine") {
      return { kind: "skip", reason: "uses .refine(callback) which is not liftable in v0" };
    }
    const conjuncts: IrFormula[] = [];
    for (const field of objectFields) {
      const fieldTerm: IrTerm = {
        kind: "ctor",
        name: "field",
        args: [{ kind: "var", name: "u" }, { kind: "const", value: field.name, sort: StringSort }],
      };
      const r = chainToConstraints(fieldTerm, field.chain);
      if (r.kind === "skip") {
        return { kind: "skip", reason: `field "${field.name}": ${r.reason}` };
      }
      conjuncts.push(...r.constraints);
    }
    if (conjuncts.length === 0) {
      return { kind: "skip", reason: "z.object had no liftable fields" };
    }
    const body = conjuncts.length === 1 ? conjuncts[0]! : { kind: "and" as const, operands: conjuncts };
    const pre: IrFormula = {
      kind: "forall",
      name: "u",
      sort: Ref,
      body,
    };
    return {
      kind: "ok",
      decl: { name, outBinding: "out", sourcePath, adapter: ADAPTER, pre },
    };
  }

  // Bare top-level chain like `const Name = z.string().min(1);`
  const chain = decodeChain(expr);
  if (chain.kind === "skip") {
    return { kind: "skip", reason: chain.reason };
  }
  const xTerm: IrTerm = { kind: "var", name: "x" };
  const r = chainToConstraints(xTerm, chain.steps);
  if (r.kind === "skip") return { kind: "skip", reason: r.reason };
  if (r.constraints.length === 0) {
    return { kind: "skip", reason: "no liftable constraints in chain" };
  }
  const body = r.constraints.length === 1 ? r.constraints[0]! : { kind: "and" as const, operands: r.constraints };
  // Sort hint based on chain root so the verifier sees x as the right sort.
  const rootSort = sortForChainRoot(chain.steps);
  const pre: IrFormula = {
    kind: "forall",
    name: "x",
    sort: rootSort,
    body,
  };
  return {
    kind: "ok",
    decl: { name, outBinding: "out", sourcePath, adapter: ADAPTER, pre },
  };
}

interface ChainStep {
  /** Method name applied (e.g., "min", "email", "string"). The root is
   * always present as the first step (e.g., "string", "number"). */
  method: string;
  /** Argument literals, if any, decoded to JS values. Non-literal arguments
   * cause the whole chain to skip. */
  args: Array<number | string | boolean>;
  /** Set when an argument was unsupported (regex literal source, etc.): we still
   * lift the call, encoding via a Ctor whose payload is the raw source. */
  rawArgs?: string[];
}

type ChainResult =
  | { kind: "ok"; steps: ChainStep[] }
  | { kind: "skip"; reason: string };

/**
 * Decode a chain like `z.string().min(1).email()` into ordered ChainSteps.
 * Returns `skip` if the chain hits an unsupported call (refine, transform,
 * pipe, etc.) so the whole schema is skipped uniformly.
 */
function decodeChain(expr: ts.Expression): ChainResult {
  const steps: ChainStep[] = [];
  let cur: ts.Expression = expr;
  while (true) {
    if (ts.isCallExpression(cur)) {
      const target = cur.expression;
      if (!ts.isPropertyAccessExpression(target)) {
        // e.g., z()
        return { kind: "skip", reason: "non-method-call at chain root" };
      }
      const methodName = target.name.text;
      if (UNSUPPORTED_METHODS.has(methodName)) {
        return { kind: "skip", reason: `uses .${methodName}(...) which is not liftable in v0` };
      }
      const args: Array<number | string | boolean> = [];
      const rawArgs: string[] = [];
      for (const a of cur.arguments) {
        const v = literalValue(a);
        if (v !== undefined) {
          args.push(v);
        } else if (ts.isRegularExpressionLiteral(a)) {
          rawArgs.push(a.text);
        } else {
          return {
            kind: "skip",
            reason: `.${methodName}(...) has non-literal argument`,
          };
        }
      }
      const step: ChainStep = { method: methodName, args };
      if (rawArgs.length > 0) step.rawArgs = rawArgs;
      steps.unshift(step);
      cur = target.expression;
      continue;
    }
    if (ts.isPropertyAccessExpression(cur)) {
      // e.g., `z.string` accessed without call: uncommon, skip.
      cur = cur.expression;
      continue;
    }
    break;
  }
  if (!ts.isIdentifier(cur) || cur.text !== "z") {
    return { kind: "skip", reason: "chain root is not the `z` namespace" };
  }
  return { kind: "ok", steps };
}

const UNSUPPORTED_METHODS = new Set([
  "refine",
  "superRefine",
  "transform",
  "pipe",
  "brand",
  "catch",
  "preprocess",
]);

function literalValue(node: ts.Node): number | string | boolean | undefined {
  if (ts.isNumericLiteral(node)) return Number(node.text);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
  if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
  return undefined;
}

interface FieldSpec {
  name: string;
  chain: ChainStep[];
}

/**
 * Pull out fields from `z.object({...})` at the root of `expr` (allowing
 * trailing chain modifiers like `.strict()` etc., which are ignored).
 *
 * Returns:
 *   - array of fields when the root is z.object
 *   - "skip-refine" when an enclosing .refine(...) was found
 *   - null when not a z.object call
 */
function extractZObjectFields(expr: ts.Expression): FieldSpec[] | "skip-refine" | null {
  // Drill through trailing chain calls to find z.object(...) at the root.
  let cur: ts.Expression = expr;
  let sawRefine = false;
  while (ts.isCallExpression(cur)) {
    const target = cur.expression;
    if (ts.isPropertyAccessExpression(target)) {
      if (UNSUPPORTED_METHODS.has(target.name.text)) sawRefine = true;
      // If this call IS z.object, we've found it.
      if (
        ts.isIdentifier(target.expression) &&
        target.expression.text === "z" &&
        target.name.text === "object"
      ) {
        if (sawRefine) return "skip-refine";
        return readObjectLiteralFields(cur);
      }
      cur = target.expression;
      continue;
    }
    break;
  }
  return null;
}

function readObjectLiteralFields(call: ts.CallExpression): FieldSpec[] | "skip-refine" {
  if (call.arguments.length !== 1) return [];
  const arg = call.arguments[0]!;
  if (!ts.isObjectLiteralExpression(arg)) return [];
  const out: FieldSpec[] = [];
  for (const prop of arg.properties) {
    if (!ts.isPropertyAssignment(prop)) continue;
    let name: string | undefined;
    if (ts.isIdentifier(prop.name)) name = prop.name.text;
    else if (ts.isStringLiteral(prop.name)) name = prop.name.text;
    if (!name) continue;
    const decoded = decodeChain(prop.initializer);
    if (decoded.kind === "skip") {
      // The skip reason might be refine/transform: bubble up to the caller.
      if (decoded.reason.includes("not liftable")) return "skip-refine";
      // Otherwise drop this field silently: but keep the others.
      continue;
    }
    out.push({ name, chain: decoded.steps });
  }
  return out;
}

type ConstraintsResult =
  | { kind: "ok"; constraints: IrFormula[] }
  | { kind: "skip"; reason: string };

/** Apply `chain` against the IrTerm `subject`, producing per-call atomics. */
function chainToConstraints(subject: IrTerm, chain: ChainStep[]): ConstraintsResult {
  const constraints: IrFormula[] = [];
  let rootKind: "string" | "number" | "boolean" | "unknown" = "unknown";

  for (const step of chain) {
    switch (step.method) {
      // Roots: set the kind constraint and the rootKind.
      case "string": {
        rootKind = "string";
        constraints.push(kindOf(subject, "String"));
        break;
      }
      case "number": {
        rootKind = "number";
        constraints.push(kindOf(subject, "Real"));
        break;
      }
      case "boolean": {
        rootKind = "boolean";
        constraints.push(kindOf(subject, "Bool"));
        break;
      }
      // Length-style on strings.
      case "min": {
        if (step.args.length !== 1 || typeof step.args[0] !== "number") {
          return { kind: "skip", reason: ".min requires a numeric literal arg" };
        }
        if (rootKind === "string") {
          constraints.push(atom(">=", lengthOf(subject), intConst(step.args[0])));
        } else if (rootKind === "number" || rootKind === "unknown") {
          constraints.push(atom(">=", subject, intConst(step.args[0])));
        }
        break;
      }
      case "max": {
        if (step.args.length !== 1 || typeof step.args[0] !== "number") {
          return { kind: "skip", reason: ".max requires a numeric literal arg" };
        }
        if (rootKind === "string") {
          constraints.push(atom("<=", lengthOf(subject), intConst(step.args[0])));
        } else if (rootKind === "number" || rootKind === "unknown") {
          constraints.push(atom("<=", subject, intConst(step.args[0])));
        }
        break;
      }
      case "length": {
        if (step.args.length !== 1 || typeof step.args[0] !== "number") {
          return { kind: "skip", reason: ".length requires a numeric literal arg" };
        }
        constraints.push(atom("=", lengthOf(subject), intConst(step.args[0])));
        break;
      }
      case "nonempty": {
        constraints.push(atom(">", lengthOf(subject), intConst(0)));
        break;
      }
      // Number-only.
      case "int": {
        constraints.push(ctorPred("is_int", [subject]));
        break;
      }
      case "nonnegative": {
        constraints.push(atom(">=", subject, intConst(0)));
        break;
      }
      case "positive": {
        constraints.push(atom(">", subject, intConst(0)));
        break;
      }
      case "nonpositive": {
        constraints.push(atom("<=", subject, intConst(0)));
        break;
      }
      case "negative": {
        constraints.push(atom("<", subject, intConst(0)));
        break;
      }
      case "finite": {
        constraints.push(ctorPred("is_finite", [subject]));
        break;
      }
      case "safe": {
        constraints.push(ctorPred("is_safe_int", [subject]));
        break;
      }
      // String-format checks: all encoded as Ctor predicates with kit-defined
      // names. The verifier will report them as undecidable when discharged
      // via Z3 with no native semantics; that's the honest v0 outcome.
      case "email": {
        constraints.push(matches(subject, "^.+@.+$"));
        break;
      }
      case "url": {
        constraints.push(matches(subject, "^https?://.+$"));
        break;
      }
      case "uuid": {
        constraints.push(ctorPred("is_uuid", [subject]));
        break;
      }
      case "cuid":
      case "cuid2":
      case "ulid": {
        constraints.push(ctorPred(`is_${step.method}`, [subject]));
        break;
      }
      case "regex": {
        const pat = step.args[0] ?? step.rawArgs?.[0];
        if (typeof pat !== "string") {
          return { kind: "skip", reason: ".regex(...) had no decodable pattern" };
        }
        constraints.push(matches(subject, pat));
        break;
      }
      case "startsWith": {
        if (typeof step.args[0] !== "string") {
          return { kind: "skip", reason: ".startsWith requires a string literal" };
        }
        constraints.push(ctorPred("starts_with", [subject, strConst(step.args[0])]));
        break;
      }
      case "endsWith": {
        if (typeof step.args[0] !== "string") {
          return { kind: "skip", reason: ".endsWith requires a string literal" };
        }
        constraints.push(ctorPred("ends_with", [subject, strConst(step.args[0])]));
        break;
      }
      // Modifiers we accept but don't constrain on (yet).
      case "optional":
      case "nullable":
      case "default":
      case "describe":
      case "trim":
      case "toLowerCase":
      case "toUpperCase":
      case "strict":
      case "passthrough":
      case "strip":
      case "object": // ignored at top because handled by extractZObjectFields
        break;
      default:
        return {
          kind: "skip",
          reason: `unsupported zod method .${step.method}(...) in v0 chain`,
        };
    }
  }

  return { kind: "ok", constraints };
}

function sortForChainRoot(steps: ChainStep[]): Sort {
  for (const s of steps) {
    if (s.method === "number") return Real;
    if (s.method === "string") return StringSort;
    if (s.method === "boolean") return Bool;
    if (s.method === "bigint") return Int;
  }
  return Ref;
}

// ---------------------------------------------------------------------------
// IR-builder helpers (kept local so adapters don't drag in the symbolic
// collector: it has a global counter that breaks CID determinism across
// multiple lift runs, see comment in ir/symbolic/property.ts).
// ---------------------------------------------------------------------------

function kindOf(t: IrTerm, kindName: string): AtomicFormula {
  return {
    kind: "atomic",
    name: "kind-of",
    args: [t, { kind: "const", value: kindName, sort: StringSort }],
  };
}

function lengthOf(t: IrTerm): IrTerm {
  return { kind: "ctor", name: "length", args: [t] };
}

function intConst(v: number): IrTerm {
  return { kind: "const", value: v, sort: Int };
}

function strConst(v: string): IrTerm {
  return { kind: "const", value: v, sort: StringSort };
}

function matches(t: IrTerm, pat: string): AtomicFormula {
  return {
    kind: "atomic",
    name: "matches",
    args: [t, { kind: "const", value: pat, sort: StringSort }],
  };
}

function ctorPred(name: string, args: IrTerm[]): AtomicFormula {
  return { kind: "atomic", name, args };
}

function atom(op: string, lhs: IrTerm, rhs: IrTerm): AtomicFormula {
  return { kind: "atomic", name: op, args: [lhs, rhs] };
}
