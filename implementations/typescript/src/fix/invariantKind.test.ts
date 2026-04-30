/**
 * Calibration tests for the invariant-kind classifier.
 *
 * Each case is anchored to either a real bug we've staged (division-by-zero,
 * empty-catch, shell-injection, Bug-1 Express duplicate methods) or a
 * representative synthetic invariant for a class we expect to encounter.
 *
 * The classifier's correctness is what determines whether oracle #1.5 routes
 * through SMT-equivalence or prose-overlap. Wrong routing = false negatives
 * that abort the loop on perfectly valid invariants.
 */

import { describe, it, expect } from "vitest";
import { classifyInvariantKind } from "./invariantKind.js";
import type { InvariantClaim } from "./types.js";

function inv(opts: {
  description: string;
  formal: string;
  bindings?: InvariantClaim["bindings"];
}): InvariantClaim {
  return {
    principleId: null,
    description: opts.description,
    formalExpression: opts.formal,
    bindings: opts.bindings ?? [],
    complexity: 1,
    witness: null,
  };
}

describe("classifyInvariantKind — concrete (arithmetic)", () => {
  it("division-by-zero: Int bindings, no set/order keywords -> concrete", () => {
    const i = inv({
      description: "denominator must not be zero",
      formal: "(declare-const b Int)\n(assert (= b 0))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });

  it("off-by-one: Int bindings, no abstract keywords -> concrete", () => {
    const i = inv({
      description: "loop index i must be less than array length n at access time",
      formal: "(declare-const i Int)\n(declare-const n Int)\n(assert (>= i n))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });

  it("integer overflow: Int bindings, additive concern, concrete", () => {
    const i = inv({
      description: "the sum a+b must fit within signed 32-bit range",
      formal: "(declare-const a Int)\n(declare-const b Int)\n(assert (> (+ a b) 2147483647))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });
});

describe("classifyInvariantKind — abstract (set/order/cardinality)", () => {
  it("Bug-1 Express: 'duplicate methods' prose flips Int bindings to abstract", () => {
    const i = inv({
      description:
        "An HTTP method appears more than once in the OPTIONS Allow response header (the options array contains duplicate entries after route._options() are appended).",
      formal:
        "(declare-const options_length Int)\n(declare-const distinct_count Int)\n(assert (> options_length distinct_count))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("uniqueness invariant: 'no two' prose -> abstract even with Int", () => {
    const i = inv({
      description: "no two cache keys may map to the same backing slot",
      formal: "(declare-const key1 Int)\n(declare-const key2 Int)\n(assert (= (slot key1) (slot key2)))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("sort-order invariant: 'sorted' prose -> abstract", () => {
    const i = inv({
      description: "the result array must be sorted ascending",
      formal: "(declare-const i Int)\n(declare-const j Int)\n(assert (and (< i j) (> (a i) (a j))))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("cardinality invariant: 'exactly once' prose -> abstract", () => {
    const i = inv({
      description: "every event must be processed exactly once",
      formal: "(declare-const event_id Int)\n(declare-const process_count Int)\n(assert (not (= process_count 1)))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("SMT distinct clause -> abstract regardless of bindings", () => {
    const i = inv({
      description: "ids generated for new sessions remain unique across the request",
      formal: "(declare-const id1 Int)\n(declare-const id2 Int)\n(declare-const id3 Int)\n(assert (not (distinct id1 id2 id3)))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("SMT forall quantifier -> abstract", () => {
    const i = inv({
      description: "for every key in the cache, its value must be non-null",
      formal: "(declare-const cache Int)\n(assert (not (forall ((k Int)) (>= (lookup cache k) 0))))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });
});

describe("classifyInvariantKind — abstract (taint / Bool-only, prior behavior)", () => {
  it("shell-injection: Bool-only, no numeric -> abstract", () => {
    const i = inv({
      description:
        "user-supplied input passed to execSync must not contain unescaped shell metacharacters",
      formal:
        "(declare-const input_contains_shell_metachar Bool)\n(assert (= input_contains_shell_metachar true))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("empty-catch: Bool-only, no numeric -> abstract", () => {
    const i = inv({
      description: "a try-block exception must not be silently swallowed",
      formal:
        "(declare-const catch_block_is_empty Bool)\n(assert (= catch_block_is_empty true))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("no declarations at all -> abstract", () => {
    const i = inv({
      description: "the function must terminate within 100ms",
      formal: "(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });
});

describe("classifyInvariantKind — edge cases", () => {
  it("concrete prose with Int bindings stays concrete (not over-eager abstract)", () => {
    const i = inv({
      description: "the divisor must be greater than zero",
      formal: "(declare-const divisor Int)\n(assert (<= divisor 0))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });

  it("Real (floating-point) bindings still concrete", () => {
    const i = inv({
      description: "the computed ratio must be within the inclusive range zero through one",
      formal: "(declare-const ratio Real)\n(assert (or (< ratio 0.0) (> ratio 1.0)))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });

  it("ascending in description matches 'ascending' (substring match)", () => {
    const i = inv({
      description: "result is ascending order",
      formal: "(declare-const i Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });
});

describe("classifyInvariantKind — cardinality-phrase coverage (Bug-1 v2 calibration)", () => {
  it("Bug-1 v2 actual phrasing: 'at most once' + 'appears more than once'", () => {
    const i = inv({
      description:
        "The Allow header in the OPTIONS response must contain each HTTP method at most once, but when multiple handlers are registered for the same method on the same path, a method appears more than once in the options array.",
      formal:
        "(declare-const allow_count Int)\n(declare-const distinct_count Int)\n(assert (> allow_count distinct_count))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("'at most one' (singular) -> abstract", () => {
    const i = inv({
      description: "the cache may hold at most one entry per key",
      formal: "(declare-const entry_count Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("'at least once' + Int bindings -> abstract", () => {
    const i = inv({
      description: "every queued event must be processed at least once",
      formal: "(declare-const process_count Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("'occurs N times' regex catches custom phrasing", () => {
    const i = inv({
      description: "the validator must run before each request occurs 3 times in the trace",
      formal: "(declare-const trace_count Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("'no more than' -> abstract (cardinality upper bound)", () => {
    const i = inv({
      description: "no more than five connections may be open simultaneously",
      formal: "(declare-const conn_count Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("'fires twice' regex catches event-cardinality language", () => {
    const i = inv({
      description: "the onChange handler fires twice per state update",
      formal: "(declare-const fire_count Int)\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("abstract");
  });

  it("plain 'must equal 5' (numeric comparison, not cardinality) stays concrete", () => {
    const i = inv({
      description: "the result must equal 5",
      formal: "(declare-const result Int)\n(assert (not (= result 5)))\n(check-sat)",
    });
    expect(classifyInvariantKind(i)).toBe("concrete");
  });
});
