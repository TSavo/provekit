/**
 * listInvariants Stage tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeListInvariantsStage,
  runListInvariants,
  LIST_INVARIANTS_CAPABILITY,
} from "./listInvariants.js";

describe("listInvariants", () => {
  it("exposes the canonical capability name", () => {
    expect(LIST_INVARIANTS_CAPABILITY).toBe("list-invariants");
  });

  it("returns storeExists=false when .provekit/invariants/ does not exist", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "list-inv-stage-"));
    const out = await runListInvariants({
      projectRoot,
      includeRetired: false,
    });
    expect(out.storeExists).toBe(false);
    expect(out.invariants).toEqual([]);
  });

  it("Stage shape: serializeInput is the cache key", () => {
    const stage = makeListInvariantsStage();
    expect(
      stage.serializeInput({
        projectRoot: "/p",
        includeRetired: true,
      }),
    ).toEqual({ projectRoot: "/p", includeRetired: true });
  });

  it("Stage shape: round-trips output through serialize/deserialize", () => {
    const stage = makeListInvariantsStage();
    const out = {
      storeExists: true,
      invariants: [
        {
          id: "inv-1",
          kind: "comparison",
          filePath: "src/a.ts",
          startLine: 10,
          originatingBug: "div by zero",
          retired: false,
        },
      ],
    };
    expect(stage.deserializeOutput(stage.serializeOutput(out))).toEqual(out);
  });
});
