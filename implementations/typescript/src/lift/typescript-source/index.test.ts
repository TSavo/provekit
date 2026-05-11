import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { canonicalJsonString } from "../../claimEnvelope/canonicalize.js";
import {
  compileTypeScriptSourceIr,
  functionContractCid,
  liftTypeScriptSourcePaths,
  liftTypeScriptSourceText,
} from "./index.js";

function tempDir(prefix = "provekit-ts-source-"): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

function rhs(contract: Record<string, any>): any {
  return contract.post.args[1];
}

describe("typescript-source lifter", () => {
  it("lifts function declarations into ts:-namespaced function-contracts wrapped by source-unit", () => {
    const result = liftTypeScriptSourceText(
      "export function add(x: number, y: number): number { return x + y; }\n",
      "src/math.ts",
    );

    expect(result.refusals).toEqual([]);
    expect(result.declarations).toHaveLength(2);

    const sourceUnit = result.declarations[0]!;
    expect(sourceUnit.kind).toBe("function-contract");
    expect(sourceUnit.fnName).toBe("src/math.ts:<source-unit>");
    expect(rhs(sourceUnit).name).toBe("ts:source-unit");
    expect(rhs(sourceUnit).args[1].name).toBe("ts:seq");

    const add = result.declarations[1]!;
    expect(add.fnName).toBe("src/math.ts:add");
    expect(add.formals).toEqual(["x", "y"]);
    expect(add.formalSorts).toEqual([
      { kind: "primitive", name: "Number" },
      { kind: "primitive", name: "Number" },
    ]);
    expect(add.returnSort).toEqual({ kind: "primitive", name: "Number" });
    expect(rhs(add)).toMatchObject({ kind: "ctor", name: "ts:add" });
    expect(functionContractCid(add)).toMatch(/^blake3-512:[0-9a-f]{128}$/);
  });

  it("qualifies class and namespace methods so distinct definitions do not collide", () => {
    const result = liftTypeScriptSourceText(
      `
      namespace N {
        export class A { m(x: number): number { return x + 1; } }
        export class B { m(x: number): number { return x + 2; } }
      }
      `,
      "src/classes.ts",
    );

    expect(result.refusals).toEqual([]);
    expect(result.declarations.map((d) => d.fnName)).toEqual([
      "src/classes.ts:<source-unit>",
      "src/classes.ts:N.A.m",
      "src/classes.ts:N.B.m",
    ]);
  });

  it("emits canonical Effect wire shapes sorted like the Rust Effect::sort_key", () => {
    const result = liftTypeScriptSourceText(
      `
      let counter = 0;
      function tick(x: number): number {
        counter = counter + x;
        console.log(counter);
        while (x > 0) {
          x = x - 1;
        }
        missing(counter);
        if (x < 0) { throw "negative"; }
        return counter;
      }
      `,
      "src/effects.ts",
    );

    expect(result.refusals).toEqual([]);
    const tick = result.declarations.find((d) => d.fnName === "src/effects.ts:tick")!;
    expect(tick.effects).toEqual([
      { kind: "reads", target: "src/effects.ts:counter" },
      { kind: "writes", target: "src/effects.ts:counter" },
      { kind: "io" },
      { kind: "panics" },
      { kind: "unresolved_call", name: "missing" },
      { kind: "opaque_loop", loopCid: expect.stringMatching(/^blake3-512:[0-9a-f]{128}$/) },
    ]);
  });

  it("refuses unsupported syntax instead of emitting unknown or skip fallbacks", () => {
    const result = liftTypeScriptSourceText(
      "const f = (x: number): number => x + 1;\n",
      "src/refuse.ts",
    );

    expect(result.declarations).toEqual([]);
    expect(result.refusals).toEqual([
      {
        kind: "ArrowFunction",
        function: "f",
        line: 1,
        reason: expect.stringContaining("arrow functions"),
      },
    ]);
    expect(canonicalJsonString(result)).not.toContain("ts:unknown");
    expect(canonicalJsonString(result)).not.toContain("ts:skip");
  });

  it("round-trips lift(compile(lift(src))) with byte-identical canonical IR", () => {
    const first = liftTypeScriptSourceText(
      "function add(x: number, y: number): number { return x + y; }\n",
      "src/roundtrip.ts",
    );
    expect(first.refusals).toEqual([]);

    const compiled = compileTypeScriptSourceIr(first.declarations);
    const second = liftTypeScriptSourceText(compiled, "src/roundtrip.ts");

    expect(second.refusals).toEqual([]);
    expect(canonicalJsonString(second.declarations)).toBe(
      canonicalJsonString(first.declarations),
    );
    expect(second.declarations.map(functionContractCid)).toEqual(
      first.declarations.map(functionContractCid),
    );
  });

  it("rejects source paths that escape the workspace root", () => {
    const td = tempDir();
    writeFileSync(join(td, "ok.ts"), "function ok(): number { return 1; }\n");

    const result = liftTypeScriptSourcePaths(td, ["../escape.ts"]);

    expect(result.declarations).toEqual([]);
    expect(result.refusals[0]).toMatchObject({
      kind: "path-traversal",
      function: null,
      line: null,
    });
  });
});
