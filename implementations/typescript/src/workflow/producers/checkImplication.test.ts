import { describe, it, expect } from "vitest";
import {
  makeCheckImplicationStage,
  type CheckImplicationInput,
  type Solver,
  type ImplicationVerdict,
} from "./checkImplication.js";

const Z3_SOLVER: Solver = {
  entries: [
    {
      type: "z3",
      binary: "z3",
      compiler: "smt-lib",
      flags: ["-in", "-T:5"],
      timeoutMs: 5000,
    },
  ],
};

describe("checkImplication", () => {
  describe("classifyVerdict (via stage)", () => {
    it("returns equivalent when both directions are unsat", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: `(declare-const x Int)
(assert (= x x))`,
        newSmt: `(declare-const y Int)
(assert (= y y))`,
        solver: Z3_SOLVER,
      };
      const output = await stage.run(input);
      expect(output.verdict).toBe("equivalent");
    });

    it("returns strengthened when new implies old but not vice versa", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: `(declare-const x Int)
(assert (>= x 0))`,
        newSmt: `(declare-const x Int)
(assert (>= x 5))`,
        solver: Z3_SOLVER,
      };
      const output = await stage.run(input);
      expect(output.verdict).toBe("strengthened");
    });

    it("returns weakened when old implies new but not vice versa", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: `(declare-const x Int)
(assert (>= x 5))`,
        newSmt: `(declare-const x Int)
(assert (>= x 0))`,
        solver: Z3_SOLVER,
      };
      const output = await stage.run(input);
      expect(output.verdict).toBe("weakened");
    });

    it("returns incomparable when neither implies the other", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: `(declare-const x Int)
(assert (= x 0))`,
        newSmt: `(declare-const x Int)
(assert (= x 1))`,
        solver: Z3_SOLVER,
      };
      const output = await stage.run(input);
      expect(output.verdict).toBe("incomparable");
    });
  });

  describe("serialize/deserialize", () => {
    it("produces cacheable JSON", () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: "(assert true)",
        newSmt: "(assert true)",
        solver: Z3_SOLVER,
      };
      const serialized = stage.serializeInput(input);
      expect(serialized).toBeDefined();
      expect(serialized.oldSmt).toBe("(assert true)");
      expect(serialized.solverEntries).toHaveLength(1);
    });

    it("round-trips output through JSON", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: "(assert true)",
        newSmt: "(assert true)",
        solver: Z3_SOLVER,
      };
      const output = await stage.run(input);
      const json = stage.serializeOutput(output);
      const restored = stage.deserializeOutput(json);
      expect(restored.verdict).toBe(output.verdict);
    });
  });

  describe("non-smt-lib compiler rejection", () => {
    it("throws when solver uses non-smt-lib compiler", async () => {
      const stage = makeCheckImplicationStage();
      const input: CheckImplicationInput = {
        oldSmt: "(assert true)",
        newSmt: "(assert true)",
        solver: {
          entries: [
            {
              type: "lean4",
              binary: "lean4",
              compiler: "lean",
              flags: [],
              timeoutMs: 5000,
            },
          ],
        },
      };
      await expect(stage.run(input)).rejects.toThrow("smt-lib");
    });
  });
});