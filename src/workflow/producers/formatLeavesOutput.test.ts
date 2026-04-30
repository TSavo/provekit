/**
 * format-leaves-output Stage tests. Pure formatter; no DB dep. Asserts
 * the text and JSON renderings of a small leaf projection.
 */

import { describe, it, expect } from "vitest";
import {
  makeFormatLeavesOutputStage,
  FORMAT_LEAVES_OUTPUT_CAPABILITY,
} from "./formatLeavesOutput.js";
import type { LocalLeaf } from "./enumerateLocalLeaves.js";

const SAMPLE_LEAVES: LocalLeaf[] = [
  {
    cid: "cid-aaa",
    bindingHash: "bh-1",
    propertyHash: "ph-1",
    verdict: "holds",
    producedBy: "ts-kit@1.0",
    evidenceKind: "bridge",
    inputCids: [],
  },
  {
    cid: "cid-bbb",
    bindingHash: "bh-2",
    propertyHash: "ph-2",
    verdict: "violated",
    producedBy: "z3@4.12",
    evidenceKind: null,
    inputCids: ["cid-aaa"],
  },
];

describe("format-leaves-output Stage", () => {
  it("declares its capability constant", () => {
    expect(FORMAT_LEAVES_OUTPUT_CAPABILITY).toBe("format-leaves-output");
  });

  it("renders empty input as a clear text message", async () => {
    const stage = makeFormatLeavesOutputStage();
    const result = await stage.run({ leaves: [], format: "text" });
    expect(result.format).toBe("text");
    expect(result.body).toBe("No locally-minted mementos.");
  });

  it("defaults to text format when none is supplied", async () => {
    const stage = makeFormatLeavesOutputStage();
    const result = await stage.run({ leaves: SAMPLE_LEAVES });
    expect(result.format).toBe("text");
    expect(result.body).toContain("Locally-minted leaves: 2");
    expect(result.body).toContain("cid-aaa");
    expect(result.body).toContain("kind=bridge");
    expect(result.body).toContain("kind=untyped");
  });

  it("emits valid JSON in json format with the same projection", async () => {
    const stage = makeFormatLeavesOutputStage();
    const result = await stage.run({ leaves: SAMPLE_LEAVES, format: "json" });
    expect(result.format).toBe("json");
    const parsed = JSON.parse(result.body) as { leaves: LocalLeaf[] };
    expect(parsed.leaves).toEqual(SAMPLE_LEAVES);
  });

  it("falls back to text for unknown format strings", async () => {
    const stage = makeFormatLeavesOutputStage();
    const result = await stage.run({ leaves: SAMPLE_LEAVES, format: "yaml" });
    expect(result.format).toBe("text");
  });

  it("round-trips through serializeOutput / deserializeOutput", async () => {
    const stage = makeFormatLeavesOutputStage();
    const result = await stage.run({ leaves: SAMPLE_LEAVES, format: "json" });
    const witness = stage.serializeOutput(result);
    expect(stage.deserializeOutput(witness)).toEqual(result);
  });
});
