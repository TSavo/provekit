import { describe, it, expect } from "vitest";
import { makePlanHookOperationStage } from "./planHookOperation.js";

describe("planHookOperation stage", () => {
  it("accepts install / uninstall / status and echoes inputs", async () => {
    const stage = makePlanHookOperationStage();
    for (const op of ["install", "uninstall", "status"] as const) {
      const out = await stage.run({ operation: op, projectRoot: "/tmp/p" });
      expect(out).toEqual({ operation: op, projectRoot: "/tmp/p" });
    }
  });

  it("rejects unknown operations", async () => {
    const stage = makePlanHookOperationStage();
    await expect(
      // @ts-expect-error — testing runtime guard
      stage.run({ operation: "burn", projectRoot: "/tmp/p" }),
    ).rejects.toThrow(/unknown operation/);
  });

  it("rejects empty project roots", async () => {
    const stage = makePlanHookOperationStage();
    await expect(
      stage.run({ operation: "install", projectRoot: "" }),
    ).rejects.toThrow(/non-empty projectRoot/);
  });

  it("round-trips via serialize / deserialize", async () => {
    const stage = makePlanHookOperationStage();
    const out = await stage.run({ operation: "install", projectRoot: "/x" });
    const witness = stage.serializeOutput(out);
    expect(stage.deserializeOutput(witness)).toEqual(out);
  });
});
