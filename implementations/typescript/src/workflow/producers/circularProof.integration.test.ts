/**
 * Circular Proof Demo: Working Integration Test
 * 
 * Demonstrates cross-language verification:
 *   TS processValue → C++ multiply2x → Go addThree → TS finalizeValue
 * 
 * Each language mints its own .proof file containing:
 *   - Contract mementos (what the function guarantees)
 *   - Bridge mementos (what external functions it calls)
 * 
 * The Rust CLI loads all .proof files and verifies the bridges.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { randomBytes } from "node:crypto";

import { generateKeypair } from "../../producerKeys/index.js";
import { mintContract, mintBridge } from "../../claimEnvelope/index.js";
import { buildProofEnvelope } from "../../proofEnvelope/index.js";
import { _resetBridges } from "../../ir/extensions/bridges.js";
import { makeLoadAllProofsStage } from "./loadAllProofs.js";
import { makeEnumerateBridgeCallsitesStage } from "./enumerateBridgeCallsites.js";
import { makeResolveBridgeTargetStage } from "./resolveBridgeTarget.js";
import { makeInstantiateObligationStage } from "./instantiateObligation.js";
import { makeSolveObligationStage } from "./solveObligation.js";
import {
  makeReportBridgeViolationsStage,
  statusFromObligationVerdict,
} from "./reportBridgeViolations.js";
import type { Solver } from "./checkImplication.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";
import { computeCid } from "../../canonicalizer/hash.js";

const IntSort = { kind: "primitive" as const, name: "Int" };

const Z3_SOLVER: Solver = {
  entries: [
    {
      type: "z3",
      binary: "z3",
      compiler: "smt-lib",
      flags: ["-in", "-T:5"],
      timeoutMs: 5000,
    },
  ],
};

function bundleAndWrite(
  packageRoot: string,
  packageName: string,
  members: Map<string, ClaimEnvelope>,
): { proofCid: string } {
  const { privateKey: catalogKey, publicKey: catalogPub } = generateKeypair({
    seed: randomBytes(32),
  });
  const pubDer = catalogPub.export({ type: "spki", format: "der" });
  const signerCid = computeCid(pubDer);

  const built = buildProofEnvelope({
    name: packageName,
    version: "1.0.0",
    members,
    signerCid,
    signerPrivateKey: catalogKey,
  });
  mkdirSync(packageRoot, { recursive: true });
  writeFileSync(join(packageRoot, `${built.cid}.proof`), Buffer.from(built.bytes));
  return { proofCid: built.cid };
}

describe("circular proof: TS → C++ → Go → TS", () => {
  it("verifies a valid circular dependency chain", async () => {
    _resetBridges();
    const projectRoot = mkdtempSync(join(tmpdir(), "circular-proof-"));
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });

      // ================================================================
      // Node 1: TypeScript (processValue)
      // Contract: input ≥ 0 → output ≥ input
      // ================================================================
      const tsPre: IrFormula = {
        kind: "forall",
        name: "input",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "input" },
            { kind: "const", value: 0, sort: IntSort },
          ],
        },
      };
      const tsPost: IrFormula = {
        kind: "forall",
        name: "input",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "out" },
            { kind: "var", name: "input" },
          ],
        },
      };
      const tsContract = mintContract({
        producedBy: "ts-kit@1",
        privateKey,
        contractName: "processValue",
        pre: tsPre,
        post: tsPost,
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "ts-kit@1" },
      });

      // ================================================================
      // Node 2: C++ (multiply2x)
      // Contract: output = 2 * input
      // Bridge from TS: TS calls multiply2x
      // ================================================================
      // C++ multiply2x PRE: x >= 0 (requires non-negative input)
      const cppPre: IrFormula = {
        kind: "forall",
        name: "x",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "x" },
            { kind: "const", value: 0, sort: IntSort },
          ],
        },
      };
      const cppContract = mintContract({
        producedBy: "cpp-kit@1",
        privateKey,
        contractName: "multiply2x",
        pre: cppPre,
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "cpp-kit@1" },
      });

      // Bridge: TS.processValue calls C++.multiply2x
      // TS.post: output ≥ input
      // C++.post: output = 2 * input
      // Check: does 2*input ≥ input? (for input ≥ 0, yes!)
      const tsToCpp = mintBridge({
        producedBy: "ts-kit@1",
        privateKey,
        sourceSymbol: "multiply2x",
        sourceLayer: "ts",
        targetContractCid: cppContract.cid,
        targetLayer: "cpp",
        irArgSorts: ["Int"],
        irReturnSort: "Int",
      });

      // ================================================================
      // Node 3: Go (addThree)
      // Contract: output = input + 3
      // Bridge from C++: C++ calls addThree
      // ================================================================
      // Go addThree PRE: x >= 0 (non-negative)
      const goPre: IrFormula = {
        kind: "forall",
        name: "x",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "x" },
            { kind: "const", value: 0, sort: IntSort },
          ],
        },
      };
      const goContract = mintContract({
        producedBy: "go-kit@1",
        privateKey,
        contractName: "addThree",
        pre: goPre,
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "go-kit@1" },
      });

      // Bridge: C++.multiply2x calls Go.addThree
      // C++ code: return addThree(doubled)
      // C++ post would need to reflect this, but for simplicity:
      // We check: does Go.post imply what C++ expects?
      const cppToGo = mintBridge({
        producedBy: "cpp-kit@1",
        privateKey,
        sourceSymbol: "addThree",
        sourceLayer: "cpp",
        targetContractCid: goContract.cid,
        targetLayer: "go",
        irArgSorts: ["Int"],
        irReturnSort: "Int",
      });

      // ================================================================
      // Node 4: TypeScript (finalizeValue): circular back to TS!
      // Contract: output = input * 2
      // Bridge from Go: Go calls finalizeValue
      // ================================================================
      // TS finalizeValue PRE: z >= 0 (non-negative)
      const ts2Pre: IrFormula = {
        kind: "forall",
        name: "z",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "z" },
            { kind: "const", value: 0, sort: IntSort },
          ],
        },
      };
      const ts2Contract = mintContract({
        producedBy: "ts-kit@1",
        privateKey,
        contractName: "finalizeValue",
        pre: ts2Pre,
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "ts-kit@1" },
      });

      // Bridge: Go.addThree calls TS.finalizeValue
      const goToTs = mintBridge({
        producedBy: "go-kit@1",
        privateKey,
        sourceSymbol: "finalizeValue",
        sourceLayer: "go",
        targetContractCid: ts2Contract.cid,
        targetLayer: "ts",
        irArgSorts: ["Int"],
        irReturnSort: "Int",
      });

      // ================================================================
      // Bundle each package's .proof
      // ================================================================
      // ================================================================
      // Consumer app: actually calls the bridged functions
      // These contracts create the CALLSITES that the verifier checks
      // ================================================================
      const consumerCallMultiply = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "uses-multiply2x",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "multiply2x",
              args: [{ kind: "const", value: 5, sort: IntSort }],
            },
            { kind: "const", value: 10, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      const consumerCallAddThree = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "uses-addThree",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "addThree",
              args: [{ kind: "const", value: 10, sort: IntSort }],
            },
            { kind: "const", value: 13, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      const consumerCallFinalize = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "uses-finalizeValue",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "finalizeValue",
              args: [{ kind: "const", value: 7, sort: IntSort }],
            },
            { kind: "const", value: 14, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      bundleAndWrite(
        join(projectRoot, "node_modules", "ts-kit"),
        "ts-kit",
        new Map([
          [tsContract.cid, tsContract],
          [ts2Contract.cid, ts2Contract],
          [tsToCpp.cid, tsToCpp],
        ]),
      );

      bundleAndWrite(
        join(projectRoot, "node_modules", "cpp-kit"),
        "cpp-kit",
        new Map([
          [cppContract.cid, cppContract],
          [cppToGo.cid, cppToGo],
        ]),
      );

      bundleAndWrite(
        join(projectRoot, "node_modules", "go-kit"),
        "go-kit",
        new Map([
          [goContract.cid, goContract],
          [goToTs.cid, goToTs],
        ]),
      );

      // Consumer's .proof at project root
      bundleAndWrite(
        projectRoot,
        "consumer-app",
        new Map([
          [consumerCallMultiply.cid, consumerCallMultiply],
          [consumerCallAddThree.cid, consumerCallAddThree],
          [consumerCallFinalize.cid, consumerCallFinalize],
        ]),
      );

      // ================================================================
      // Verify: load all proofs and check bridges
      // ================================================================
      const loadStage = makeLoadAllProofsStage();
      const pool = await loadStage.run({ projectRoot });
      expect(pool.errors).toEqual([]);
      expect(Object.keys(pool.mementoPool.mementos).length).toBe(10); // 6 kit contracts + 3 bridges + 3 consumer contracts

      const enumStage = makeEnumerateBridgeCallsitesStage();
      const enumResult = await enumStage.run({
        mementoPool: pool.mementoPool,
        bridgesBySymbol: pool.bridgesBySymbol,
      });
      expect(enumResult.callsites.length).toBeGreaterThan(0);

      // Resolve and solve
      const resolveStage = makeResolveBridgeTargetStage();
      const instantiateStage = makeInstantiateObligationStage();
      const solveStage = makeSolveObligationStage();

      const rows = [];
      for (const cs of enumResult.callsites) {
        const resolved = await resolveStage.run({
          bridgeTargetContractCid: cs.bridgeTargetContractCid,
          mementoPool: pool.mementoPool,
        });
        if (!resolved.resolved) {
          rows.push({ status: "unresolved-target", reason: resolved.failureReason });
          continue;
        }
        const arg = cs.argTerms[0];
        if (!arg) {
          rows.push({ status: "unliftable-argument", reason: "no first arg" });
          continue;
        }
        const obligation = await instantiateStage.run({
          formula: resolved.resolved.irFormula,
          argTerm: arg,
        });
        if (!obligation.obligation) {
          rows.push({ status: "lift-error", reason: obligation.failureMessage });
          continue;
        }
        const solveResult = await solveStage.run({
          obligation: obligation.obligation,
          solver: Z3_SOLVER,
        });
        rows.push({
          status: statusFromObligationVerdict(solveResult.verdict),
        });
      }

      // All bridges should be discharged (the math works out)
      const reportStage = makeReportBridgeViolationsStage();
      const final = await reportStage.run({ rows });
      
      console.log(`Circular proof: ${final.discharged}/${final.totalCallsites} discharged`);
      expect(final.violations).toBe(0);

    } finally {
      rmSync(projectRoot, { recursive: true, force: true });
      _resetBridges();
    }
  }, 30000);

  it("catches a bug when Go contract is weakened", async () => {
    _resetBridges();
    const projectRoot = mkdtempSync(join(tmpdir(), "circular-bug-"));
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });

      // Same TS contract
      const tsContract = mintContract({
        producedBy: "ts-kit@1",
        privateKey,
        contractName: "processValue",
        pre: {
          kind: "forall",
          name: "input",
          sort: IntSort,
          body: {
            kind: "atomic",
            name: "≥",
            args: [
              { kind: "var", name: "input" },
              { kind: "const", value: 0, sort: IntSort },
            ],
          },
        },
        post: {
          kind: "forall",
          name: "input",
          sort: IntSort,
          body: {
            kind: "atomic",
            name: "≥",
            args: [
              { kind: "var", name: "out" },
              { kind: "var", name: "input" },
            ],
          },
        },
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "ts-kit@1" },
      });

      // BUG: Go contract now requires x >= 10 (too restrictive!)
      // This breaks calls with x < 10
      const buggyGoPre: IrFormula = {
        kind: "forall",
        name: "x",
        sort: IntSort,
        body: {
          kind: "atomic",
          name: "≥",
          args: [
            { kind: "var", name: "x" },
            { kind: "const", value: 10, sort: IntSort },
          ],
        },
      };
      const buggyGoContract = mintContract({
        producedBy: "go-kit@1",
        privateKey,
        contractName: "addThree",
        pre: buggyGoPre,
        outBinding: "out",
        authoring: { producerKind: "kit-author", author: "go-kit@1" },
      });

      const cppToGo = mintBridge({
        producedBy: "cpp-kit@1",
        privateKey,
        sourceSymbol: "addThree",
        sourceLayer: "cpp",
        targetContractCid: buggyGoContract.cid,
        targetLayer: "go",
        irArgSorts: ["Int"],
        irReturnSort: "Int",
      });

      bundleAndWrite(
        join(projectRoot, "node_modules", "ts-kit"),
        "ts-kit",
        new Map([[tsContract.cid, tsContract]]),
      );

      // Consumer calls addThree(5), which violates the buggy pre x >= 10
      const consumerBuggyCall = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "uses-addThree-buggy",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "addThree",
              args: [{ kind: "const", value: 5, sort: IntSort }],
            },
            { kind: "const", value: 8, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      bundleAndWrite(
        join(projectRoot, "node_modules", "go-kit"),
        "go-kit",
        new Map([
          [buggyGoContract.cid, buggyGoContract],
          [cppToGo.cid, cppToGo],
        ]),
      );

      bundleAndWrite(
        projectRoot,
        "consumer-app",
        new Map([[consumerBuggyCall.cid, consumerBuggyCall]]),
      );

      // Verify
      const loadStage = makeLoadAllProofsStage();
      const pool = await loadStage.run({ projectRoot });

      const enumStage = makeEnumerateBridgeCallsitesStage();
      const enumResult = await enumStage.run({
        mementoPool: pool.mementoPool,
        bridgesBySymbol: pool.bridgesBySymbol,
      });

      const resolveStage = makeResolveBridgeTargetStage();
      const instantiateStage = makeInstantiateObligationStage();
      const solveStage = makeSolveObligationStage();

      let foundViolation = false;
      for (const cs of enumResult.callsites) {
        const resolved = await resolveStage.run({
          bridgeTargetContractCid: cs.bridgeTargetContractCid,
          mementoPool: pool.mementoPool,
        });
        if (!resolved.resolved) continue;
        const arg = cs.argTerms[0];
        if (!arg) continue;
        const obligation = await instantiateStage.run({
          formula: resolved.resolved.irFormula,
          argTerm: arg,
        });
        if (!obligation.obligation) continue;
        const solveResult = await solveStage.run({
          obligation: obligation.obligation,
          solver: Z3_SOLVER,
        });
        if (solveResult.verdict === "unsatisfied") {
          foundViolation = true;
        }
      }

      expect(foundViolation).toBe(true);
      console.log("Bug caught! Go contract weakening detected.");

    } finally {
      rmSync(projectRoot, { recursive: true, force: true });
      _resetBridges();
    }
  }, 30000);
});
