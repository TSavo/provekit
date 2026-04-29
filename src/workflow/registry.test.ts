import { describe, it, expect } from "vitest";
import { InMemoryRegistry } from "./registry.js";
import type { Stage } from "./types.js";

function trivialStage<I, O>(
  name: string,
  fn: (input: I) => O,
): Stage<I, O> {
  return {
    name,
    producedBy: `${name}@1.0`,
    serializeInput: (input) => input,
    serializeOutput: (output) => JSON.stringify(output),
    deserializeOutput: (witness) => JSON.parse(witness) as O,
    async run(input) {
      return fn(input);
    },
  };
}

describe("InMemoryRegistry", () => {
  it("registers and resolves a capability", () => {
    const reg = new InMemoryRegistry();
    const stage = trivialStage("intake", (s: string) => s.length);
    reg.register("intake", stage);
    const resolved = reg.resolve<string, number>("intake");
    expect(resolved).toBe(stage);
  });

  it("returns null for unregistered capabilities", () => {
    const reg = new InMemoryRegistry();
    expect(reg.resolve("missing")).toBeNull();
  });

  it("throws on double-registration", () => {
    const reg = new InMemoryRegistry();
    const a = trivialStage("intake", (s: string) => s.length);
    const b = trivialStage("intake-v2", (s: string) => s.length + 1);
    reg.register("intake", a);
    expect(() => reg.register("intake", b)).toThrow(/already registered/);
  });

  it("replace() overwrites without throwing", () => {
    const reg = new InMemoryRegistry();
    const a = trivialStage("intake", (s: string) => s.length);
    const b = trivialStage("intake-v2", (s: string) => s.length + 1);
    reg.register("intake", a);
    reg.replace("intake", b);
    expect(reg.resolve("intake")).toBe(b);
  });

  it("capabilities() returns sorted list of registered names", () => {
    const reg = new InMemoryRegistry();
    reg.register("verify", trivialStage("v", (n: number) => n));
    reg.register("intake", trivialStage("i", (n: number) => n));
    reg.register("patch", trivialStage("p", (n: number) => n));
    expect(reg.capabilities()).toEqual(["intake", "patch", "verify"]);
  });
});
