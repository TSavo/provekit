/**
 * provekit-lift adapter: class-validator.
 *
 * Walks `class Foo { @IsNotEmpty() @MinLength(2) field: string }` style
 * DTO declarations (NestJS standard) and lifts each class to a contract
 * memento. Each property's decorator chain contributes conjuncts to the
 * class-level precondition.
 *
 * Strategic positioning: we do NOT replace class-validator. Developers
 * keep their existing decorated DTOs. This adapter reads the decorators
 * and produces a signed `<cid>.proof` so the validation contract becomes
 * content-addressed and shippable across kit boundaries.
 *
 * SHAPE (v0):
 *
 *   class CreateUserDto {
 *     @IsNotEmpty() @MinLength(2)        username: string;
 *     @IsEmail()                         email:    string;
 *     @Min(0) @Max(120)                  age:      number;
 *   }
 *
 *   lifts to a ContractDecl named CreateUserDto with:
 *     pre: forall u: Ref. and(
 *       length(field(u, "username")) > 0,
 *       length(field(u, "username")) >= 2,
 *       matches_email_regex(field(u, "email")),
 *       field(u, "age") >= 0,
 *       field(u, "age") <= 120,
 *     )
 *
 * Each decorator becomes a (set of) atomic constraint(s) over `field(u,
 * "<name>")`, encoded as kit-defined Ctors when no canonical predicate
 * exists. Unknown decorators (custom validators, @Validate(MyConstraint),
 * etc.) cause the WHOLE CLASS to skip with a warning, mirroring the
 * fail-loud-on-unknown discipline of the zod adapter's .refine() handling.
 */

import ts from "typescript";
import type { IrFormula, IrTerm, AtomicFormula } from "../../ir/formulas.js";
import { Int, Real, String as StringSort, Bool, Ref } from "../../ir/sorts.js";
import type { Sort } from "../../ir/formulas.js";
import type { AdapterOutput, ContractDecl, AdapterWarning } from "../types.js";

const ADAPTER = "class-validator";

/** Public adapter entry point — mirrors the Rust adapter's `lift_file`. */
export function liftFile(sourceFile: ts.SourceFile, sourcePath: string): AdapterOutput {
  const decls: ContractDecl[] = [];
  const warnings: AdapterWarning[] = [];
  let seen = 0;

  ts.forEachChild(sourceFile, (node) => {
    const cand = extractClassCandidate(node);
    if (!cand) return;
    seen += 1;
    const r = liftClass(cand, sourcePath);
    if (r.kind === "ok") {
      decls.push(r.decl);
    } else {
      warnings.push({
        adapter: ADAPTER,
        sourcePath,
        itemName: cand.name,
        reason: r.reason,
      });
    }
  });

  return { decls, seen, lifted: decls.length, warnings };
}

interface ClassCandidate {
  name: string;
  cls: ts.ClassDeclaration;
}

/**
 * A "candidate" is any top-level named class declaration that has at
 * least one property carrying at least one decorator. Classes with zero
 * decorated properties are not class-validator schemas and are not
 * touched (the adapter sees only decorated DTOs).
 */
function extractClassCandidate(node: ts.Node): ClassCandidate | null {
  if (!ts.isClassDeclaration(node)) return null;
  if (!node.name) return null;
  let hasDecoratedProp = false;
  for (const m of node.members) {
    if (!ts.isPropertyDeclaration(m)) continue;
    const decs = ts.canHaveDecorators(m) ? ts.getDecorators(m) : undefined;
    if (decs && decs.length > 0) {
      hasDecoratedProp = true;
      break;
    }
  }
  if (!hasDecoratedProp) return null;
  return { name: node.name.text, cls: node };
}

type LiftResult =
  | { kind: "ok"; decl: ContractDecl }
  | { kind: "skip"; reason: string };

function liftClass(c: ClassCandidate, sourcePath: string): LiftResult {
  const conjuncts: IrFormula[] = [];

  for (const member of c.cls.members) {
    if (!ts.isPropertyDeclaration(member)) continue;
    const propName = propertyName(member);
    if (!propName) continue;
    const decs = ts.canHaveDecorators(member) ? ts.getDecorators(member) : undefined;
    if (!decs || decs.length === 0) continue;

    const propSort = sortFromTypeNode(member.type);
    const fieldTerm: IrTerm = {
      kind: "ctor",
      name: "field",
      args: [
        { kind: "var", name: "u" },
        { kind: "const", value: propName, sort: StringSort },
      ],
    };

    // Add a kind-of constraint when we know the static type. This mirrors
    // the zod adapter, which emits kind-of for z.string()/z.number()/etc.
    if (propSort.kind === "primitive" && propSort.name !== "Ref") {
      conjuncts.push(kindOf(fieldTerm, propSort.name));
    }

    for (const dec of decs) {
      const r = liftDecorator(dec, fieldTerm, propSort, propName);
      if (r.kind === "skip") {
        return { kind: "skip", reason: `property "${propName}": ${r.reason}` };
      }
      conjuncts.push(...r.constraints);
    }
  }

  if (conjuncts.length === 0) {
    return { kind: "skip", reason: "class had no liftable decorated properties" };
  }

  const body =
    conjuncts.length === 1
      ? conjuncts[0]!
      : { kind: "and" as const, operands: conjuncts };

  const pre: IrFormula = {
    kind: "forall",
    name: "u",
    sort: Ref,
    body,
  };

  return {
    kind: "ok",
    decl: { name: c.name, outBinding: "out", sourcePath, adapter: ADAPTER, pre },
  };
}

function propertyName(prop: ts.PropertyDeclaration): string | null {
  if (ts.isIdentifier(prop.name)) return prop.name.text;
  if (ts.isStringLiteral(prop.name)) return prop.name.text;
  return null;
}

function sortFromTypeNode(t: ts.TypeNode | undefined): Sort {
  if (!t) return Ref;
  switch (t.kind) {
    case ts.SyntaxKind.StringKeyword:
      return StringSort;
    case ts.SyntaxKind.NumberKeyword:
      return Real;
    case ts.SyntaxKind.BooleanKeyword:
      return Bool;
    case ts.SyntaxKind.BigIntKeyword:
      return Int;
    default:
      return Ref;
  }
}

type DecoratorResult =
  | { kind: "ok"; constraints: IrFormula[] }
  | { kind: "skip"; reason: string };

function liftDecorator(
  dec: ts.Decorator,
  subject: IrTerm,
  sort: Sort,
  propName: string,
): DecoratorResult {
  const expr = dec.expression;
  let name: string;
  let argNodes: readonly ts.Expression[];

  if (ts.isCallExpression(expr)) {
    if (!ts.isIdentifier(expr.expression)) {
      return { kind: "skip", reason: "non-identifier decorator callee" };
    }
    name = expr.expression.text;
    argNodes = expr.arguments;
  } else if (ts.isIdentifier(expr)) {
    // Bare `@IsInt` — legal class-validator usage (rare but supported).
    name = expr.text;
    argNodes = [];
  } else {
    return { kind: "skip", reason: "unsupported decorator expression shape" };
  }

  const args: Array<number | string | boolean> = [];
  for (const a of argNodes) {
    const v = literalValue(a);
    if (v === undefined) {
      // Some decorators (e.g., @IsEnum, @ValidateNested) take object/class
      // arguments we can't lift. Skip the whole class on first such case.
      return {
        kind: "skip",
        reason: `@${name}(...) has non-literal argument`,
      };
    }
    args.push(v);
  }

  const isStringSort = sort.kind === "primitive" && sort.name === "String";
  const isNumericSort =
    sort.kind === "primitive" && (sort.name === "Real" || sort.name === "Int");

  switch (name) {
    // Length / non-empty on strings.
    case "IsNotEmpty": {
      if (isStringSort) {
        return ok([atom(">", lengthOf(subject), intConst(0))]);
      }
      // For non-string sorts we conservatively encode as kit-Ctor.
      return ok([ctorPred("is_not_empty", [subject])]);
    }
    case "IsEmpty": {
      if (isStringSort) {
        return ok([atom("=", lengthOf(subject), intConst(0))]);
      }
      return ok([ctorPred("is_empty", [subject])]);
    }
    case "MinLength": {
      if (args.length !== 1 || typeof args[0] !== "number") {
        return { kind: "skip", reason: "@MinLength requires a numeric literal arg" };
      }
      return ok([atom(">=", lengthOf(subject), intConst(args[0]))]);
    }
    case "MaxLength": {
      if (args.length !== 1 || typeof args[0] !== "number") {
        return { kind: "skip", reason: "@MaxLength requires a numeric literal arg" };
      }
      return ok([atom("<=", lengthOf(subject), intConst(args[0]))]);
    }
    case "Length": {
      if (
        args.length < 1 ||
        args.length > 2 ||
        typeof args[0] !== "number" ||
        (args.length === 2 && typeof args[1] !== "number")
      ) {
        return { kind: "skip", reason: "@Length requires 1 or 2 numeric literal args" };
      }
      const out: IrFormula[] = [atom(">=", lengthOf(subject), intConst(args[0]))];
      if (args.length === 2) {
        out.push(atom("<=", lengthOf(subject), intConst(args[1] as number)));
      }
      return ok(out);
    }

    // Numeric bounds.
    case "Min": {
      if (args.length !== 1 || typeof args[0] !== "number") {
        return { kind: "skip", reason: "@Min requires a numeric literal arg" };
      }
      return ok([atom(">=", subject, intConst(args[0]))]);
    }
    case "Max": {
      if (args.length !== 1 || typeof args[0] !== "number") {
        return { kind: "skip", reason: "@Max requires a numeric literal arg" };
      }
      return ok([atom("<=", subject, intConst(args[0]))]);
    }
    case "IsPositive": {
      return ok([atom(">", subject, intConst(0))]);
    }
    case "IsNegative": {
      return ok([atom("<", subject, intConst(0))]);
    }

    // Type predicates — encode as kit-Ctors. The kind-of from the static
    // type is already emitted at the property level; these decorators
    // tighten it (e.g., @IsInt on a `number` field).
    case "IsInt": {
      return ok([ctorPred("is_int", [subject])]);
    }
    case "IsNumber": {
      return ok([ctorPred("is_number", [subject])]);
    }
    case "IsBoolean": {
      return ok([ctorPred("is_boolean", [subject])]);
    }
    case "IsString": {
      return ok([ctorPred("is_string", [subject])]);
    }
    case "IsDate": {
      return ok([ctorPred("is_date", [subject])]);
    }
    case "IsArray": {
      return ok([ctorPred("is_array", [subject])]);
    }
    case "IsObject": {
      return ok([ctorPred("is_object", [subject])]);
    }

    // String formats — kit-defined Ctors. The verifier reports them as
    // undecidable when discharged via Z3 with no native semantics; that's
    // the honest v0 outcome and matches the zod adapter's encoding.
    case "IsEmail": {
      return ok([ctorPred("matches_email_regex", [subject])]);
    }
    case "IsUrl":
    case "IsURL": {
      return ok([ctorPred("matches_url_regex", [subject])]);
    }
    case "IsUUID": {
      return ok([ctorPred("is_uuid", [subject])]);
    }
    case "IsAlpha": {
      return ok([ctorPred("is_alpha", [subject])]);
    }
    case "IsAlphanumeric": {
      return ok([ctorPred("is_alphanumeric", [subject])]);
    }
    case "IsAscii": {
      return ok([ctorPred("is_ascii", [subject])]);
    }
    case "IsBase64": {
      return ok([ctorPred("is_base64", [subject])]);
    }
    case "IsHexadecimal": {
      return ok([ctorPred("is_hex", [subject])]);
    }
    case "IsJSON": {
      return ok([ctorPred("is_json", [subject])]);
    }
    case "IsIP": {
      return ok([ctorPred("is_ip", [subject])]);
    }
    case "IsPhoneNumber": {
      return ok([ctorPred("is_phone_number", [subject])]);
    }
    case "Matches": {
      // @Matches(/^foo$/) or @Matches("^foo$").
      const expr0 = argNodes[0];
      let pat: string | undefined;
      if (expr0 && ts.isRegularExpressionLiteral(expr0)) {
        pat = expr0.text;
      } else if (typeof args[0] === "string") {
        pat = args[0];
      }
      if (!pat) return { kind: "skip", reason: "@Matches requires a regex or string literal" };
      return ok([
        {
          kind: "atomic",
          name: "matches",
          args: [subject, { kind: "const", value: pat, sort: StringSort }],
        } satisfies AtomicFormula,
      ]);
    }

    // Decorators we accept but don't constrain on (yet).
    case "IsOptional":
    case "IsDefined":
    case "Allow":
    case "Expose":
    case "Exclude":
    case "Type":
    case "Transform":
    case "ValidateIf":
      // Suppress unused-var lints in some configs.
      void propName;
      return ok([]);

    // Unknown / custom validators — fail loudly per the zod adapter pattern.
    default:
      return {
        kind: "skip",
        reason: `unsupported class-validator decorator @${name}(...) in v0`,
      };
  }
}

function ok(constraints: IrFormula[]): DecoratorResult {
  return { kind: "ok", constraints };
}

function literalValue(node: ts.Node): number | string | boolean | undefined {
  if (ts.isNumericLiteral(node)) return Number(node.text);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node))
    return node.text;
  if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
  // Negative numeric literal: -N parses as PrefixUnaryExpression(MinusToken, NumericLiteral).
  if (
    ts.isPrefixUnaryExpression(node) &&
    node.operator === ts.SyntaxKind.MinusToken &&
    ts.isNumericLiteral(node.operand)
  ) {
    return -Number(node.operand.text);
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// IR-builder helpers (kept local; same pattern as zod.ts).
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

function ctorPred(name: string, args: IrTerm[]): AtomicFormula {
  return { kind: "atomic", name, args };
}

function atom(op: string, lhs: IrTerm, rhs: IrTerm): AtomicFormula {
  return { kind: "atomic", name: op, args: [lhs, rhs] };
}
