/**
 * format-roots-output Stage tests. Pure formatter; no DB dep. Asserts
 * text + JSON renderings of an external-CID list.
 */

import { describe, it, expect } from "vitest";
import {
  makeFormatRootsOutputStage,
  FORMAT_ROOTS_OUTPUT_CAPABILITY,
} from "./formatRootsOutput.js";

const SAMPLE_ROOTS = ["cid-alpha", "cid-mid", "cid-omega"];

describe("format-roots-output Stage", () => {
  it("declares its capability constant", () => {
    expect(FORMAT_ROOTS_OUTPUT_CAPABILITY).toBe("format-roots-output");
  });

  it("renders empty input as a clear text message", async () => {
    const stage = makeFormatRootsOutputStage();
    const result = await stage.run({ roots: [], format: "text" });
    expect(result.format).toBe("text");
    expect(result.body).toBe(
      "No external roots — every referenced CID was minted locally.",
    );
  });

  it("defaults to text format when none is supplied", async () => {
    const stage = makeFormatRootsOutputStage();
    const result = await stage.run({ roots: SAMPLE_ROOTS });
    expect(result.format).toBe("text");
    expect(result.body).toContain("External roots: 3");
    expect(result.body).toContain("cid-alpha");
    expect(result.body).toContain("cid-mid");
    expect(result.body).toContain("cid-omega");
  });

  it("emits valid JSON in json format with the same projection", async () => {
    const stage = makeFormatRootsOutputStage();
    const result = await stage.run({ roots: SAMPLE_ROOTS, format: "json" });
    expect(result.format).toBe("json");
    const parsed = JSON.parse(result.body) as { roots: string[] };
    expect(parsed.roots).toEqual(SAMPLE_ROOTS);
  });

  it("falls back to text for unknown format strings", async () => {
    const stage = makeFormatRootsOutputStage();
    const result = await stage.run({ roots: SAMPLE_ROOTS, format: "yaml" });
    expect(result.format).toBe("text");
  });

  it("round-trips through serializeOutput / deserializeOutput", async () => {
    const stage = makeFormatRootsOutputStage();
    const result = await stage.run({ roots: SAMPLE_ROOTS, format: "json" });
    const witness = stage.serializeOutput(result);
    expect(stage.deserializeOutput(witness)).toEqual(result);
  });
});
