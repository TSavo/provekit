import { describe, expect, it } from "vitest";

import { liftTypeScriptSourceText } from "./index.js";
import { normalizeTypeScriptSourceVerifyDocument } from "./verify.js";

describe("typescript-source verify projection", () => {
  it("projects TypeScript body contracts into solver-facing ProofIR without source-unit noise", () => {
    const lifted = liftTypeScriptSourceText(
      "export function double(x: number): number { return x * 2; }\n",
      "src/double.ts",
    );

    const doc = normalizeTypeScriptSourceVerifyDocument(lifted);

    expect(doc.kind).toBe("ir-document");
    expect(doc.ir).toHaveLength(1);
    const contract = doc.ir[0] as any;
    expect(contract.kind).toBe("function-contract");
    expect(contract.fnName).toBe("src/double.ts:double");
    expect(contract.bridgeSourceSymbol).toBe("double");
    expect(contract.formals).toEqual(["x"]);
    expect(contract.post.args[0]).toEqual({ kind: "var", name: "result" });
    expect(contract.post.args[1]).toMatchObject({ kind: "ctor", name: "*" });
    expect(contract.post.args[1].args[1]).toEqual({
      kind: "const",
      value: 2,
      sort: { kind: "primitive", name: "Int" },
    });
    expect(JSON.stringify(doc.ir)).not.toContain("<source-unit>");
  });
});
