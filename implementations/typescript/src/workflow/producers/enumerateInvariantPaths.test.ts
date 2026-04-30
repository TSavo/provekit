/**
 * enumerateInvariantPaths Stage tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeEnumerateInvariantPathsStage,
  runEnumerateInvariantPaths,
  ENUMERATE_INVARIANT_PATHS_CAPABILITY,
} from "./enumerateInvariantPaths.js";

describe("enumerateInvariantPaths", () => {
  it("exposes the canonical capability name", () => {
    expect(ENUMERATE_INVARIANT_PATHS_CAPABILITY).toBe(
      "enumerate-invariant-paths",
    );
  });

  it("throws when the requested invariant is not found", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "paths-stage-"));
    await expect(
      runEnumerateInvariantPaths({
        projectRoot,
        invariantId: "missing",
        maxPaths: 50,
      }),
    ).rejects.toThrow(/invariant missing not found/);
  });

  it("Stage shape: serializeInput is the cache key", () => {
    const stage = makeEnumerateInvariantPathsStage();
    expect(
      stage.serializeInput({
        projectRoot: "/p",
        invariantId: "inv-1",
        maxPaths: 50,
      }),
    ).toEqual({
      projectRoot: "/p",
      invariantId: "inv-1",
      maxPaths: 50,
    });
  });

  it("Stage shape: round-trips output through serialize/deserialize", () => {
    const stage = makeEnumerateInvariantPathsStage();
    const out = {
      invariantId: "inv-1",
      filePath: "src/a.ts",
      startLine: 10,
      paths: [
        {
          steps: [{ slot: "callsite", nodeId: "abcd1234abcd1234" }],
        },
      ],
    };
    expect(stage.deserializeOutput(stage.serializeOutput(out))).toEqual(out);
  });
});
