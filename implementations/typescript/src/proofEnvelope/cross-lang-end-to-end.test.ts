// THE END-TO-END DEMO.
//
// A TS function calls into the C++ kit (via the kit's bridged TS
// primitive), authors a property memento, and `provekit verify`
// (bridge enforcement) catches violations of the C++-published
// precondition.
//
// Architecture, end-to-end:
//
//   1. C++ kit author writes parseInt_kit_proof.cpp using kit
//      primitives (must, forall, gt, num).  `forall n: Int. n > 0`
//      becomes the parseInt precondition.
//   2. C++ binary mints + signs + bundles into a .proof file.
//      (We rely on the prior commit's output at /tmp/cpp-kit-out/.)
//   3. The .proof file is installed in node_modules at the kit's
//      package root (mimics `npm install @example/cpp-kit`).
//   4. The TS consumer authors invariants by CALLING the bridged
//      kit primitive `parseInt(...)` — that call emits a
//      Ctor("parseInt", [arg]) IrTerm.
//   5. The consumer's collected declarations get minted + bundled
//      into the consumer's own .proof.
//   6. runBridgeEnforcement walks both .proofs:
//        - load-all-proofs builds a unified CID pool.
//        - enumerate-bridge-callsites walks every property memento's
//          formula, finds Ctor("parseInt", ...), emits a callsite.
//        - resolve-bridge-target hash-lookups the bridge → property
//          memento → IR formula (the C++-authored precondition).
//        - instantiate-obligation substitutes the call's arg into
//          the precondition.
//        - solve-obligation passes to Z3.
//        - report-bridge-violations aggregates.
//
//   parseInt(num(5))  → instantiate `5 > 0` → unsat(¬(5 > 0)) → DISCHARGED
//   parseInt(num(0))  → instantiate `0 > 0` → sat(¬(0 > 0))   → UNSATISFIED
//
// The TS code never imports a single line of C++. The connection is
// the protocol: the bridge in the C++-published .proof points at the
// C++-authored property memento. The TS verifier walks bytes the
// C++ kit produced, discharges obligations against them.

import { describe, it, expect } from "vitest";
import { existsSync, readdirSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { randomBytes } from "node:crypto";

// Kit primitive: parseInt is a bridged Ctor; calling it emits IR.
import { parseInt as parseIntPrim, num, eq } from "../ir/symbolic/primitives.js";
import { must, beginCollecting } from "../ir/symbolic/property.js";

// TS-side mint + bundle (the consumer's own .proof).
import { mintContract } from "../claimEnvelope/index.js";
import { generateKeypair } from "../producerKeys/index.js";
import { buildProofEnvelope } from "./index.js";
import { computeCid } from "../canonicalizer/hash.js";
import { runBridgeEnforcement } from "../verifier/bridgeEnforcement.js";
import { _resetBridges } from "../ir/extensions/bridges.js";

const CPP_OUT_DIR = "/tmp/cpp-kit-out-v11";
const PROOF_FILENAME_RE = /^([a-z0-9]+-[0-9]+:[0-9a-f]+)\.proof$/;

function findV11CppProof(): { filename: string; path: string; cid: string } | null {
  if (!existsSync(CPP_OUT_DIR)) return null;
  for (const entry of readdirSync(CPP_OUT_DIR)) {
    const m = entry.match(PROOF_FILENAME_RE);
    if (m) return { filename: entry, path: `${CPP_OUT_DIR}/${entry}`, cid: m[1]! };
  }
  return null;
}

describe("END-TO-END: TS function calls C++ kit primitive; verify catches violations", () => {
  const cppProof = findV11CppProof();
  it.runIf(cppProof !== null)(
    "parseInt(num(5)) discharges; parseInt(num(0)) is caught by the C++ precondition",
    async () => {
      _resetBridges();
      const { filename: CPP_PROOF_FILENAME, path: CPP_PROOF_PATH, cid: CPP_PROOF_CID } =
        cppProof!;
      const projectRoot = mkdtempSync(join(tmpdir(), "cross-lang-e2e-"));
      try {
        // ---- 1. Install the C++-produced .proof in node_modules ----
        const cppKitDir = join(projectRoot, "node_modules", "@example", "cpp-kit");
        mkdirSync(cppKitDir, { recursive: true });
        const cppProofBytes = readFileSync(CPP_PROOF_PATH);
        writeFileSync(join(cppKitDir, CPP_PROOF_FILENAME), cppProofBytes);
        writeFileSync(
          join(cppKitDir, "package.json"),
          JSON.stringify({
            name: "@example/cpp-kit",
            version: "1.0.0",
            provekit: { proofHash: CPP_PROOF_CID },
          }, null, 2),
        );

        // ---- 2. The TS consumer's invariants (the "TS function that calls C++") ----
        // These calls into the bridged kit primitive `parseIntPrim` produce
        // Ctor("parseInt", [arg]) IrTerms. That's what "calling C++" means at
        // the protocol level: the kit primitive emits the IR ctor that hashes
        // back into the C++-published bridge.
        const finishCollect = beginCollecting();
        must(
          "consumer-calls-parseInt-with-positive-5",
          eq(parseIntPrim(num(5)), num(5)),
        );
        must(
          "consumer-calls-parseInt-with-zero",
          eq(parseIntPrim(num(0)), num(0)),
        );
        const decls = finishCollect();
        expect(decls).toHaveLength(2);

        // ---- 3. Mint each declaration into a contract memento ----
        const { privateKey } = generateKeypair({ seed: randomBytes(32) });
        const consumerMembers = new Map();
        for (const decl of decls) {
          if (decl.kind !== "contract") continue;
          if (!decl.pre) continue;
          const env = mintContract({
            producedBy: "consumer-app@1",
            privateKey,
            contractName: decl.name,
            pre: decl.pre,
            authoring: { producerKind: "kit-author", author: "consumer-app@1" },
          });
          consumerMembers.set(env.cid, env);
        }

        // ---- 4. Bundle the consumer's mementos into its own .proof ----
        const { privateKey: catalogKey, publicKey: catalogPub } = generateKeypair({ seed: randomBytes(32) });
        const pubDer = catalogPub.export({ type: "spki", format: "der" });
        const signerCid = computeCid(pubDer);
        const built = buildProofEnvelope({
          name: "consumer-app",
          version: "1.0.0",
          members: consumerMembers,
          signerCid,
          signerPrivateKey: catalogKey,
        });
        writeFileSync(join(projectRoot, `${built.cid}.proof`), Buffer.from(built.bytes));
        writeFileSync(
          join(projectRoot, "package.json"),
          JSON.stringify({
            name: "consumer-app",
            version: "1.0.0",
            provekit: { proofHash: built.cid },
          }, null, 2),
        );

        // ---- 5. Run the protocol-first verifier ----
        const report = await runBridgeEnforcement(projectRoot);

        // The C++-produced bridge for parseInt was discovered.
        expect(report.totalCallsites).toBe(2);

        const passingRow = report.rows.find((r) =>
          (r.callsite as { propertyName: string }).propertyName.includes("with-positive-5"),
        )!;
        const failingRow = report.rows.find((r) =>
          (r.callsite as { propertyName: string }).propertyName.includes("with-zero"),
        )!;

        // 5 > 0 → discharged via Z3.
        expect(passingRow.status).toBe("discharged");

        // 0 > 0 → UNSAT failed → the C++-declared precondition catches the
        // TS code that calls parseInt with 0.
        expect(failingRow.status).toBe("unsatisfied");

        // The protocol's complete claim:
        //   - C++ author wrote the precondition.
        //   - C++ kit signed + sealed it in a .proof file.
        //   - TS consumer called parseInt(num(0)).
        //   - TS verifier discovered the C++-published bridge, instantiated
        //     the C++-authored precondition at the call site, and proved
        //     UNSAT — entirely via the protocol; no language-specific glue.
        console.log(
          `\n  ✓ DEMO: TS verifier caught parseInt(num(0)) using the C++-authored precondition.\n` +
            `    Discharged calls:  ${report.discharged}\n` +
            `    Caught violations: ${report.violations}\n`,
        );
      } finally {
        rmSync(projectRoot, { recursive: true, force: true });
        _resetBridges();
      }
    },
    30000,
  );
});
