import { describe, it, expect } from "vitest";
import { parseZ3Model } from "./modelParser.js";

describe("parseZ3Model", () => {
  it("parses a simple Real model", () => {
    const input = `
(
  (define-fun den () Real
    0.0)
  (define-fun result () Real
    (/ 1.0 0.0))
)
    `.trim();

    const parsed = parseZ3Model(input);
    expect(parsed.get("den")).toEqual({ sort: "Real", value: 0 });
    expect(parsed.get("result")).toEqual({ sort: "Real", value: "div_by_zero" });
  });

  it("parses Int values", () => {
    const input = `
(
  (define-fun count () Int
    42)
  (define-fun neg () Int
    (- 5))
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("count")).toEqual({ sort: "Int", value: 42n });
    expect(parsed.get("neg")).toEqual({ sort: "Int", value: -5n });
  });

  it("parses Bool values", () => {
    const input = `
(
  (define-fun ok () Bool
    true)
  (define-fun bad () Bool
    false)
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("ok")).toEqual({ sort: "Bool", value: true });
    expect(parsed.get("bad")).toEqual({ sort: "Bool", value: false });
  });

  it("parses String values", () => {
    const input = `
(
  (define-fun s () String
    "hello")
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("s")).toEqual({ sort: "String", value: "hello" });
  });

  it("returns an empty map on empty model", () => {
    expect(parseZ3Model("()").size).toBe(0);
  });
});
