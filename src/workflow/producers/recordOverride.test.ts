import { describe, it, expect } from "vitest";
import { makeRecordOverrideStage } from "./recordOverride.js";

describe("recordOverride stage", () => {
  it("produces a structured override record", async () => {
    const stage = makeRecordOverrideStage();
    const out = await stage.run({ reason: "intentional fallthrough for migration" });
    expect(out.reason).toBe("intentional fallthrough for migration");
    expect(out.followupCommand).toBe("git commit --no-verify");
    expect(out.message).toContain("Override recorded");
  });

  it("rejects empty / whitespace reason", async () => {
    const stage = makeRecordOverrideStage();
    await expect(stage.run({ reason: "" })).rejects.toThrow(/non-empty reason/);
    await expect(stage.run({ reason: "   " })).rejects.toThrow(/non-empty reason/);
  });

  it("round-trips via serialize / deserialize", async () => {
    const stage = makeRecordOverrideStage();
    const out = await stage.run({ reason: "x" });
    const witness = stage.serializeOutput(out);
    const restored = stage.deserializeOutput(witness);
    expect(restored).toEqual(out);
  });

  it("respects producerVersion override", () => {
    const stage = makeRecordOverrideStage({ producerVersion: "test@9.9" });
    expect(stage.producedBy).toBe("test@9.9");
  });
});
