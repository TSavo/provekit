import { describe, it, expect, afterEach, vi } from "vitest";
import { getModelTier, MODEL_TIERS } from "./modelTiers";

const RESET_KEYS = [
  "PROVEKIT_MODEL_OVERRIDE",
  "PROVEKIT_MODEL_C1",
  "PROVEKIT_MODEL_INTAKEREPORT",
  "PROVEKIT_MODEL_NEWSTAGE",
];

afterEach(() => {
  for (const k of RESET_KEYS) delete process.env[k];
  vi.restoreAllMocks();
});

describe("getModelTier", () => {
  it("returns the mapped tier when set in MODEL_TIERS", () => {
    expect(getModelTier("C1")).toBe("opus");
    expect(getModelTier("classify")).toBe("sonnet");
    expect(getModelTier("intake-report")).toBe("sonnet");
  });

  it("falls back to sonnet for unmapped stage names", () => {
    expect(getModelTier("nonexistent-stage-xyz")).toBe("sonnet");
  });

  it("global PROVEKIT_MODEL_OVERRIDE wins over MODEL_TIERS", () => {
    process.env.PROVEKIT_MODEL_OVERRIDE = "haiku";
    expect(getModelTier("C1")).toBe("haiku");
    expect(getModelTier("classify")).toBe("haiku");
  });

  it("normalizes hyphenated stage names to PROVEKIT_MODEL_<UPPERNOPUNCT>", () => {
    process.env.PROVEKIT_MODEL_INTAKEREPORT = "opus";
    expect(getModelTier("intake-report")).toBe("opus");
  });

  it("per-stage override takes precedence over MODEL_TIERS but not global", () => {
    process.env.PROVEKIT_MODEL_C1 = "sonnet";
    expect(getModelTier("C1")).toBe("sonnet");

    process.env.PROVEKIT_MODEL_OVERRIDE = "opus";
    expect(getModelTier("C1")).toBe("opus");
  });

  it("ignores invalid env values (case-insensitive validation) and warns", () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    process.env.PROVEKIT_MODEL_NEWSTAGE = "gigachat";
    expect(getModelTier("newstage")).toBe("sonnet");
    expect(warnSpy).toHaveBeenCalledTimes(1);
  });

  it("accepts mixed-case env values", () => {
    process.env.PROVEKIT_MODEL_OVERRIDE = "OPUS";
    expect(getModelTier("any")).toBe("opus");
  });
});

describe("MODEL_TIERS", () => {
  it("does not assign haiku to any default stage (Bug-1 v5 calibration)", () => {
    for (const [stage, tier] of Object.entries(MODEL_TIERS)) {
      expect(tier, `stage ${stage} must not default to haiku`).not.toBe("haiku");
    }
  });
});
