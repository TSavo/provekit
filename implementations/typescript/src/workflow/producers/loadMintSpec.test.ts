/**
 * loadMintSpec Stage tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeLoadMintSpecStage,
  runLoadMintSpec,
  LOAD_MINT_SPEC_CAPABILITY,
} from "./loadMintSpec.js";

describe("loadMintSpec", () => {
  it("exposes the canonical capability name", () => {
    expect(LOAD_MINT_SPEC_CAPABILITY).toBe("load-mint-spec");
  });

  it("returns the inline spec verbatim when supplied", async () => {
    const out = await runLoadMintSpec({
      kind: "property",
      spec: {
        bindingHash: "aaaa",
        propertyHash: "bbbb",
        producedBy: "p@v1",
      },
    });
    expect(out.kind).toBe("property");
    expect(out.spec).toEqual({
      bindingHash: "aaaa",
      propertyHash: "bbbb",
      producedBy: "p@v1",
    });
  });

  it("loads the spec from a JSON file when specPath is supplied", async () => {
    const tmp = mkdtempSync(join(tmpdir(), "load-spec-"));
    const specPath = join(tmp, "spec.json");
    writeFileSync(
      specPath,
      JSON.stringify({ bindingHash: "x", propertyHash: "y", producedBy: "z" }),
    );
    const out = await runLoadMintSpec({ kind: "generic", specPath });
    expect(out.kind).toBe("generic");
    expect(out.spec).toEqual({
      bindingHash: "x",
      propertyHash: "y",
      producedBy: "z",
    });
  });

  it("throws when neither spec nor specPath is supplied", async () => {
    await expect(
      runLoadMintSpec({ kind: "property" }),
    ).rejects.toThrow(/requires either spec or specPath/);
  });

  it("Stage shape: serializeInput preserves the input shape", () => {
    const stage = makeLoadMintSpecStage();
    expect(
      stage.serializeInput({
        kind: "bridge",
        specPath: "/p/spec.json",
      }),
    ).toEqual({ kind: "bridge", specPath: "/p/spec.json" });
  });

  it("Stage shape: round-trips output through serialize/deserialize", () => {
    const stage = makeLoadMintSpecStage();
    const out = {
      kind: "property" as const,
      spec: {
        bindingHash: "x",
        propertyHash: "y",
        producedBy: "z",
      },
    };
    expect(stage.deserializeOutput(stage.serializeOutput(out))).toEqual(out);
  });
});
