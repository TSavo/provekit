// Driver test for the TypeScript self-contracts orchestrator.
//
// Vitest's Vite ESM loader handles `@ipld/dag-cbor` (ESM-only) cleanly,
// where the repo's tsx-driven CJS launchers currently can't on Node 25.
// This test IS the working invocation; the bluepaper Appendix A.1
// documents it as such.

import { describe, expect, it } from "vitest";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { runMintSelfContracts } from "./mint-ts-self-contracts.mjs";

describe("ts-self-contracts: mint orchestrator", () => {
  it("mints the catalog deterministically and prints the CID", () => {
    const dirA = mkdtempSync(join(tmpdir(), "ts-self-A-"));
    const dirB = mkdtempSync(join(tmpdir(), "ts-self-B-"));
    try {
      const a = runMintSelfContracts(dirA);
      const b = runMintSelfContracts(dirB);

      // The bluepaper-documented banner — vitest captures stdout with
      // its default reporter, so this output IS the deliverable.
      console.log("");
      console.log("== ProvekIt TypeScript self-contracts orchestrator ==");
      console.log("authored:");
      for (const { label, count } of b.perSourceCounts) {
        console.log(
          `  ${label.padStart(22)}  ${String(count).padStart(2)} contracts`,
        );
      }
      console.log(
        `  ${"[ALL]".padStart(22)}  ${String(b.totalContracts).padStart(2)} contracts (TOTAL)`,
      );
      console.log("");
      console.log(`  bytes:              ${b.bytesLen}`);
      console.log(`  members:            ${b.memberCount}`);
      console.log(`  total contracts:    ${b.totalContracts}`);
      console.log(`  catalog CID:        ${b.cid}`);
      console.log(`  contractSetCid:     ${b.contractSetCid}`);
      console.log(
        `  determinism check:  ${a.cid === b.cid && a.contractSetCid === b.contractSetCid ? "OK" : "FAILED"} (two runs produced ${a.cid === b.cid ? "identical" : "different"} CIDs)`,
      );
      console.log("");

      // Determinism check (assertion form).
      expect(a.cid).toEqual(b.cid);
      expect(a.contractSetCid).toEqual(b.contractSetCid);

      // contractSetCid has the standard v1.1.0 self-identifying shape.
      expect(b.contractSetCid).toMatch(/^blake3-512:[0-9a-f]{128}$/);

      // Sanity: catalog CID has the standard v1.1.0 self-identifying shape.
      expect(b.cid).toMatch(/^blake3-512:[0-9a-f]{128}$/);

      // .proof file written, non-empty.
      const stat = statSync(b.path);
      expect(stat.size).toBe(b.bytesLen);
      expect(stat.size).toBeGreaterThan(0);

      // Each slab authored at least one contract — no zero-contract files.
      for (const { label, count } of b.perSourceCounts) {
        expect(count, `slab ${label}`).toBeGreaterThan(0);
      }
    } finally {
      rmSync(dirA, { recursive: true, force: true });
      rmSync(dirB, { recursive: true, force: true });
    }
  });
});
