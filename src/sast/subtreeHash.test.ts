import { describe, it, expect } from "vitest";
import { createHash } from "crypto";
import { subtreeHash } from "./subtreeHash.js";

describe("subtreeHash", () => {
  it("returns sha256 hex of the input string", () => {
    const expected = createHash("sha256").update("abc").digest("hex");
    expect(subtreeHash("abc")).toBe(expected);
  });

  it("is stable across calls", () => {
    expect(subtreeHash("hello")).toBe(subtreeHash("hello"));
  });

  it("produces different hashes for different inputs", () => {
    expect(subtreeHash("foo")).not.toBe(subtreeHash("bar"));
  });
});
