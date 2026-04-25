/**
 * A7b: Parser tests.
 */

import { describe, it, expect } from "vitest";
import { parseDSL, ParseError } from "./parser.js";

// ---------------------------------------------------------------------------
// Division-by-zero DSL source used across tests.
// ---------------------------------------------------------------------------

const DIV_BY_ZERO_SRC = `
predicate zero_guard($var: node) {
  match $g: node where narrows.target_node == $var and narrows.narrowing_kind == "literal_eq"
}

principle division-by-zero {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) before $div
  report violation {
    at $div
    captures { division: $div }
    message "division denominator may be zero"
  }
}
`.trim();

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("parseDSL", () => {
  it("round-trip: parses division-by-zero and produces correct top-level structure", () => {
    const program = parseDSL(DIV_BY_ZERO_SRC);
    expect(program.nodes).toHaveLength(2);

    const predicate = program.nodes[0];
    expect(predicate.kind).toBe("predicate");
    if (predicate.kind !== "predicate") throw new Error("expected predicate");
    expect(predicate.name).toBe("zero_guard");
    expect(predicate.paramVar).toBe("var");
    expect(predicate.matchClauses).toHaveLength(1);

    const guardClause = predicate.matchClauses[0];
    expect(guardClause.varName).toBe("g");
    expect(guardClause.where.kind).toBe("andPred");
    expect(guardClause.where.operands).toHaveLength(2);

    // First atom: narrows.target_node == $var
    const atom0 = guardClause.where.operands[0];
    expect(atom0.lhs.capability).toBe("narrows");
    expect(atom0.lhs.column).toBe("target_node");
    expect(atom0.rhs.kind).toBe("varRef");
    if (atom0.rhs.kind === "varRef") {
      expect(atom0.rhs.name).toBe("var");
    }

    // Second atom: narrows.narrowing_kind == "literal_eq"
    const atom1 = guardClause.where.operands[1];
    expect(atom1.lhs.capability).toBe("narrows");
    expect(atom1.lhs.column).toBe("narrowing_kind");
    expect(atom1.rhs.kind).toBe("string");
    if (atom1.rhs.kind === "string") {
      expect(atom1.rhs.value).toBe("literal_eq");
    }

    const principle = program.nodes[1];
    expect(principle.kind).toBe("principle");
    if (principle.kind !== "principle") throw new Error("expected principle");
    expect(principle.name).toBe("division-by-zero");
    expect(principle.matchClauses).toHaveLength(1);

    const divClause = principle.matchClauses[0];
    expect(divClause.varName).toBe("div");
    expect(divClause.where.operands).toHaveLength(1);
    expect(divClause.where.operands[0].lhs.capability).toBe("arithmetic");
    expect(divClause.where.operands[0].lhs.column).toBe("op");
    expect(divClause.where.operands[0].rhs.kind).toBe("string");
    if (divClause.where.operands[0].rhs.kind === "string") {
      expect(divClause.where.operands[0].rhs.value).toBe("/");
    }

    // Require clause
    const req = principle.requireClause;
    expect(req).not.toBeNull();
    if (!req) throw new Error("expected requireClause");
    expect(req.guardVar).toBe("guard");
    expect(req.predName).toBe("zero_guard");
    expect(req.predArgVarName).toBeNull();
    expect(req.predArgDeref).not.toBeNull();
    if (req.predArgDeref) {
      expect(req.predArgDeref.varName).toBe("div");
      expect(req.predArgDeref.capability).toBe("arithmetic");
      expect(req.predArgDeref.column).toBe("rhs_node");
    }
    expect(req.relation).toBe("before");
    expect(req.targetVarName).toBe("div");
    expect(req.targetVarDeref).toBeNull();

    // Report block
    const rb = principle.reportBlock;
    expect(rb.severity).toBe("violation");
    expect(rb.atVar).toBe("div");
    expect(rb.captures).toHaveLength(1);
    expect(rb.captures[0].name).toBe("division");
    expect(rb.captures[0].varName).toBe("div");
    expect(rb.message).toBe("division denominator may be zero");
  });

  it("parse error: missing closing brace produces error with location", () => {
    const src = `principle foo { match $x: node where arithmetic.op == "/"`;
    expect(() => parseDSL(src)).toThrow(ParseError);
    try {
      parseDSL(src);
    } catch (e) {
      expect(e).toBeInstanceOf(ParseError);
      const pe = e as ParseError;
      expect(pe.message).toContain("Parse error");
      expect(pe.line).toBeGreaterThan(0);
    }
  });

  it("parse error: 'require yes' instead of 'require no' produces error", () => {
    const src = `
principle foo {
  match $x: node where arithmetic.op == "/"
  require yes $guard: zero_guard($x) before $x
  report violation { at $x captures { div: $x } message "oops" }
}
    `.trim();
    expect(() => parseDSL(src)).toThrow(ParseError);
    try {
      parseDSL(src);
    } catch (e) {
      expect(e).toBeInstanceOf(ParseError);
      expect((e as ParseError).message).toContain("'no'");
    }
  });

  it("parses a principle with no require clause", () => {
    const src = `
principle simple {
  match $x: node where arithmetic.op == "+"
  report info {
    at $x
    captures { expr: $x }
    message "addition found"
  }
}
    `.trim();
    const program = parseDSL(src);
    expect(program.nodes).toHaveLength(1);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind === "principle") {
      expect(p.requireClause).toBeNull();
      expect(p.reportBlock.severity).toBe("info");
    }
  });

  it("parses arbitrary relation name in require clause (same_value)", () => {
    const src = `
principle test-same-value {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) same_value $div
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(program.nodes).toHaveLength(1);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind !== "principle") throw new Error("expected principle");
    const req = p.requireClause;
    expect(req).not.toBeNull();
    if (!req) throw new Error("expected requireClause");
    expect(req.relation).toBe("same_value");
    expect(req.predName).toBe("zero_guard");
    expect(req.targetVarName).toBe("div");
    expect(req.targetVarDeref).toBeNull();
  });

  it("parses varDeref RHS in match clause", () => {
    const src = `
principle cross-ref {
  match
    $div: node where arithmetic.op == "/"
    $den: node where arithmetic.node_id == $div.arithmetic.rhs_node
  report violation {
    at $div
    captures { division: $div, denominator: $den }
    message "test"
  }
}
    `.trim();
    const program = parseDSL(src);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind === "principle") {
      expect(p.matchClauses).toHaveLength(2);
      const den = p.matchClauses[1];
      const atom = den.where.operands[0];
      expect(atom.rhs.kind).toBe("varDeref");
      if (atom.rhs.kind === "varDeref") {
        expect(atom.rhs.varName).toBe("div");
        expect(atom.rhs.capability).toBe("arithmetic");
        expect(atom.rhs.column).toBe("rhs_node");
      }
    }
  });

  it("parses varDeref target in require clause OLD form (same_value $y.cap.col)", () => {
    const src = `
principle test-deref-target {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) same_value $div.arithmetic.rhs_node
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(program.nodes).toHaveLength(1);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind !== "principle") throw new Error("expected principle");
    const req = p.requireClause;
    expect(req).not.toBeNull();
    if (!req) throw new Error("expected requireClause");
    expect(req.relation).toBe("same_value");
    // OLD form: relationArgs is null, targetVarDeref is populated.
    expect(req.relationArgs).toBeNull();
    expect(req.targetVarName).toBeNull();
    expect(req.targetVarDeref).not.toBeNull();
    if (req.targetVarDeref) {
      expect(req.targetVarDeref.varName).toBe("div");
      expect(req.targetVarDeref.capability).toBe("arithmetic");
      expect(req.targetVarDeref.column).toBe("rhs_node");
    }
  });

  it("parses NEW where RELATION(LHS, RHS) form with two varDeref args", () => {
    const src = `
principle test-explicit-rel {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div)
    where same_value($guard.narrows.target_node, $div.arithmetic.rhs_node)
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(program.nodes).toHaveLength(1);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind !== "principle") throw new Error("expected principle");
    const req = p.requireClause;
    expect(req).not.toBeNull();
    if (!req) throw new Error("expected requireClause");
    expect(req.relation).toBe("same_value");
    // NEW form: relationArgs is populated, targetVarName/targetVarDeref are null.
    expect(req.relationArgs).not.toBeNull();
    expect(req.targetVarName).toBeNull();
    expect(req.targetVarDeref).toBeNull();
    if (req.relationArgs) {
      expect(req.relationArgs).toHaveLength(2);
      const [lhs, rhs] = req.relationArgs;
      // LHS: $guard.narrows.target_node
      expect(lhs.name).toBe("guard");
      expect(lhs.deref).not.toBeNull();
      if (lhs.deref) {
        expect(lhs.deref.varName).toBe("guard");
        expect(lhs.deref.capability).toBe("narrows");
        expect(lhs.deref.column).toBe("target_node");
      }
      // RHS: $div.arithmetic.rhs_node
      expect(rhs.name).toBe("div");
      expect(rhs.deref).not.toBeNull();
      if (rhs.deref) {
        expect(rhs.deref.varName).toBe("div");
        expect(rhs.deref.capability).toBe("arithmetic");
        expect(rhs.deref.column).toBe("rhs_node");
      }
    }
  });

  it("OLD form still parses: require no $g: pred($x) before $y", () => {
    const src = `
principle test-old-compat {
  match $div: node where arithmetic.op == "/"
  require no $guard: zero_guard($div.arithmetic.rhs_node) before $div
  report violation { at $div captures { div: $div } message "test" }
}
    `.trim();
    const program = parseDSL(src);
    expect(program.nodes).toHaveLength(1);
    const p = program.nodes[0];
    expect(p.kind).toBe("principle");
    if (p.kind !== "principle") throw new Error("expected principle");
    const req = p.requireClause;
    expect(req).not.toBeNull();
    if (!req) throw new Error("expected requireClause");
    expect(req.relation).toBe("before");
    // OLD form: relationArgs is null.
    expect(req.relationArgs).toBeNull();
    expect(req.targetVarName).toBe("div");
    expect(req.targetVarDeref).toBeNull();
  });
});
