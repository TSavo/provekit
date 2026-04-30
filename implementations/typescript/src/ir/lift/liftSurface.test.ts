/**
 * Unit tests for liftSurfaceText — the in-memory `string -> ts.Program ->
 * liftProject` helper used by the formulate-via-lifter producer.
 */
import { describe, it, expect } from "vitest";
import { liftSurfaceText } from "./liftSurface.js";

describe("liftSurfaceText", () => {
  it("lifts a single trivial property", () => {
    const surface = `
      import { property } from "provekit/ir";
      property("trivial", true);
    `;
    const result = liftSurfaceText(surface);
    expect(result.properties).toHaveLength(1);
    expect(result.properties[0]?.name).toBe("trivial");
  });

  it("lifts a forAll<Int> predicate", () => {
    const surface = `
      import { property, forAll } from "provekit/ir";
      import type { Int } from "provekit/sorts";
      property("nonNegative", forAll<Int>((x) => x >= 0));
    `;
    const result = liftSurfaceText(surface);
    expect(result.properties).toHaveLength(1);
    const prop = result.properties[0]!;
    expect(prop.name).toBe("nonNegative");
    expect(prop.formula.kind).toBe("forall");
  });

  it("lifts multiple property() calls in order", () => {
    const surface = `
      import { property } from "provekit/ir";
      property("a", true);
      property("b", false);
    `;
    const result = liftSurfaceText(surface);
    const names = result.properties.map((p) => p.name);
    expect(names).toEqual(["a", "b"]);
  });

  it("rejects virtual paths that don't end in .invariant.ts", () => {
    expect(() => liftSurfaceText("// empty", "/tmp/foo.ts")).toThrow(
      /must end in \.invariant\.ts/,
    );
  });

  it("emits a diagnostic when surface text contains an unliftable construct", () => {
    const surface = `
      import { property } from "provekit/ir";
      property("bad", somethingUnknown(42) === 0);
    `;
    const result = liftSurfaceText(surface);
    const messages = result.diagnostics.map((d) => String(d.messageText));
    expect(messages.some((m) => m.includes("not in pure-function registry"))).toBe(
      true,
    );
  });
});
