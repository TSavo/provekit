/**
 * Bridge enforcement workflow — end-to-end integration test.
 *
 * Architecture (per the protocol):
 *   - Kit emits IR when its invariant code runs
 *   - Mint bundles the kit's IR into .proof files
 *   - Bridge enforcement loads ALL .proof files into a unified
 *     CID-keyed pool, then everything is hash lookup
 *
 * Setup:
 *   - C++-style "kit" mints precondition (property memento) + bridge
 *     declaration (parseInt → property memento CID), bundles into a
 *     .proof at its package root
 *   - Consumer "kit" mints property mementos referencing parseInt
 *     ctors with both a satisfying arg (5) and a violating arg (-3),
 *     bundles into its own .proof at the project root
 *
 * Test:
 *   - Stage 1: load-all-proofs walks all .proof files into one pool
 *   - Stage 2: enumerate every property memento in the pool, find
 *     parseInt ctors → callsites
 *   - Stage 3: for each callsite, hash-lookup the bridge target in
 *     the pool → resolved precondition
 *   - Stage 4: instantiate + solve
 *   - Stage 5: aggregate
 *   - Assert: parseInt(5) discharged; parseInt(-3) unsatisfied
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
  type BridgeReportRow,
} from "./reportBridgeViolations.js";
import type { Solver } from "./checkImplication.js";
import type { IrFormula } from "../../ir/formulas.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";

const IntSort = { kind: "primitive" as const, name: "Int" };

// Test helper retained for any future hash-of-string needs; switches to
// the v1.1.0 self-identifying BLAKE3-512 form.
import { computeCid as computeCidImpl } from "../../canonicalizer/hash.js";
function hashString(s: string): string {
  return computeCidImpl(Buffer.from(s, "utf8"));
}
void hashString;

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
  const signerCid = computeCidImpl(pubDer);

  const built = buildProofEnvelope({
    name: packageName,
    version: "1.0.0",
    members,
    signerCid,
    signerPrivateKey: catalogKey,
  });
  mkdirSync(packageRoot, { recursive: true });
  writeFileSync(join(packageRoot, `${built.cid}.proof`), Buffer.from(built.bytes));
  writeFileSync(
    join(packageRoot, "package.json"),
    JSON.stringify(
      { name: packageName, version: "1.0.0", provekit: { proofHash: built.cid } },
      null,
      2,
    ),
  );
  return { proofCid: built.cid };
}

describe("bridge enforcement end-to-end", () => {
  it("discharges parseInt(positive) and reports parseInt(negative) as unsatisfied", async () => {
    _resetBridges();
    const projectRoot = mkdtempSync(join(tmpdir(), "bridge-e2e-"));
    try {
      const { privateKey } = generateKeypair({ seed: randomBytes(32) });

      // ----- C++-style kit: precondition + bridge -----
      const precondition: IrFormula = {
        kind: "forall", name: "n", sort: IntSort, body: {
            kind: "atomic",
            name: ">",
            args: [
              { kind: "var", name: "n"},
              { kind: "const", value: 0, sort: IntSort },
            ],
          },
      };

      const propertyMemento = mintContract({
        producedBy: "cpp-kit@1",
        privateKey,
        contractName: "parseInt",
        pre: precondition,
        authoring: { producerKind: "kit-author", author: "cpp-kit@1" },
      });

      const bridgeMemento = mintBridge({
        producedBy: "ts-kit@1",
        privateKey,
        sourceSymbol: "parseInt",
        sourceLayer: "ts",
        targetContractCid: propertyMemento.cid,
        targetLayer: "cpp-kit",
        irArgSorts: ["String"],
        irReturnSort: "Int",
      });

      bundleAndWrite(
        join(projectRoot, "node_modules", "@example", "cpp-kit"),
        "@example/cpp-kit",
        new Map([
          [propertyMemento.cid, propertyMemento],
          [bridgeMemento.cid, bridgeMemento],
        ]),
      );

      // ----- Consumer's invariant code emitted these via running -----
      // Contract memento whose pre carries a parseInt(5) Ctor.
      const callsitePos = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "calls-parseInt-with-5",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "parseInt",
              args: [{ kind: "const", value: 5, sort: IntSort }],
            },
            { kind: "const", value: 5, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      // Contract memento whose pre carries a parseInt(-3) Ctor.
      const callsiteNeg = mintContract({
        producedBy: "consumer-app@1",
        privateKey,
        contractName: "calls-parseInt-with-neg-3",
        pre: {
          kind: "atomic",
          name: "=",
          args: [
            {
              kind: "ctor",
              name: "parseInt",
              args: [{ kind: "const", value: -3, sort: IntSort }],
            },
            { kind: "const", value: -3, sort: IntSort },
          ],
        },
        authoring: { producerKind: "kit-author", author: "consumer-app@1" },
      });

      // Consumer's .proof at the project root.
      bundleAndWrite(
        projectRoot,
        "consumer-app",
        new Map([
          [callsitePos.cid, callsitePos],
          [callsiteNeg.cid, callsiteNeg],
        ]),
      );

      // ----- Stage 1: load all proofs into a unified pool -----
      const loadStage = makeLoadAllProofsStage();
      const pool = await loadStage.run({ projectRoot });
      expect(pool.errors).toEqual([]);
      expect(Object.keys(pool.mementoPool.mementos).length).toBe(4); // 2 from kit + 2 from consumer
      expect(pool.bridgesBySymbol.parseInt).toBeDefined();

      // ----- Stage 2: enumerate bridge callsites in the pool -----
      const enumStage = makeEnumerateBridgeCallsitesStage();
      const enumResult = await enumStage.run({
        mementoPool: pool.mementoPool,
        bridgesBySymbol: pool.bridgesBySymbol,
      });
      expect(enumResult.callsites).toHaveLength(2);

      // ----- Stages 3, 4 per callsite -----
      const resolveStage = makeResolveBridgeTargetStage();
      const instantiateStage = makeInstantiateObligationStage();
      const solveStage = makeSolveObligationStage();

      const rows: BridgeReportRow[] = [];
      for (const cs of enumResult.callsites) {
        const resolved = await resolveStage.run({
          bridgeTargetContractCid: cs.bridgeTargetContractCid,
          mementoPool: pool.mementoPool,
        });
        if (!resolved.resolved) {
          rows.push({
            callsite: cs as unknown as BridgeReportRow["callsite"],
            status: "unresolved-target",
            ...(resolved.failureReason ? { reason: resolved.failureReason } : {}),
          });
          continue;
        }
        const arg = cs.argTerms[0];
        if (!arg) {
          rows.push({
            callsite: cs as unknown as BridgeReportRow["callsite"],
            status: "unliftable-argument",
            reason: "no first arg",
          });
          continue;
        }
        const obligation = await instantiateStage.run({
          formula: resolved.resolved.irFormula,
          argTerm: arg,
        });
        if (!obligation.obligation) {
          rows.push({
            callsite: cs as unknown as BridgeReportRow["callsite"],
            status: obligation.failureReason === "formula-not-forall" ? "non-precondition" : "lift-error",
            ...(obligation.failureMessage ? { reason: obligation.failureMessage } : {}),
          });
          continue;
        }
        const solveResult = await solveStage.run({
          obligation: obligation.obligation,
          solver: Z3_SOLVER,
        });
        rows.push({
          callsite: cs as unknown as BridgeReportRow["callsite"],
          status: statusFromObligationVerdict(solveResult.verdict),
          solverProbes: solveResult.perEntry.map((e) => ({ solverType: e.solverType, probe: e.probe })),
        });
      }

      // ----- Stage 5: aggregate -----
      const reportStage = makeReportBridgeViolationsStage();
      const final = await reportStage.run({ rows });
      expect(final.totalCallsites).toBe(2);
      expect(final.discharged).toBe(1);
      expect(final.violations).toBe(1);

      const positiveRow = final.rows.find((r) =>
        (r.callsite as { propertyName: string }).propertyName.includes("with-5"),
      )!;
      const negativeRow = final.rows.find((r) =>
        (r.callsite as { propertyName: string }).propertyName.includes("neg-3"),
      )!;
      expect(positiveRow.status).toBe("discharged");
      expect(negativeRow.status).toBe("unsatisfied");
    } finally {
      rmSync(projectRoot, { recursive: true, force: true });
      _resetBridges();
    }
  }, 30000);
});
