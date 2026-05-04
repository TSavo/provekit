// SPDX-License-Identifier: Apache-2.0
//
// Mint orchestrator. Drives one full author + mint + bundle pass:
//
//   1. Author every contract slab in JavaKitInvariants.
//   2. Mint each as a signed memento under the foundation key
//      (Ed25519, seed = [0x42; 32]).
//   3. Bundle every memento into a `<cid>.proof` whose filename IS the
//      catalog CID; the catalog is built via ProofEnvelope.build().
//   4. Compute contractSetCid (signer-independent, sort-then-hash).
//
// Mirrors implementations/csharp/Provekit.SelfContracts/Program.cs and
// implementations/rust/provekit-self-contracts/src/lib.rs MintOneRun.

package com.provekit.selfcontracts;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import com.provekit.claimenvelope.Blake3;
import com.provekit.claimenvelope.ClaimEnvelope;
import com.provekit.claimenvelope.Ed25519;
import com.provekit.claimenvelope.ProofEnvelope;
import com.provekit.selfcontracts.Slab.AuthoredSlab;
import com.provekit.selfcontracts.Slab.ContractDecl;

public final class Orchestrator {

    public static final byte[] FOUNDATION_SEED = Ed25519.FOUNDATION_V0_SEED;
    public static final String DECLARED_AT = "2026-04-30T18:00:00.000Z";
    public static final String PRODUCED_BY = "provekit-java-self-contracts@1.0";
    public static final String CATALOG_NAME = "@provekit/java-self-contracts";
    public static final String CATALOG_VERSION = "1.0.0";

    private Orchestrator() {}

    /** One mint run; returns (cid, contractSetCid, contracts, slabs, proof bytes). */
    public static final class MintResult {
        public final String cid;
        public final String contractSetCid;
        public final int contractCount;
        public final int slabCount;
        public final byte[] bytes;

        MintResult(String cid, String contractSetCid, int contractCount, int slabCount, byte[] bytes) {
            this.cid = cid;
            this.contractSetCid = contractSetCid;
            this.contractCount = contractCount;
            this.slabCount = slabCount;
            this.bytes = bytes;
        }
    }

    /**
     * Author every slab, mint every contract, build the catalog, write
     * {@code <cid>.proof} into {@code outDir}. Returns the mint result.
     */
    public static MintResult mintOneRun(Path outDir) throws IOException {
        Files.createDirectories(outDir);

        List<AuthoredSlab> slabs = JavaKitInvariants.authorAll();

        Map<String, byte[]> members = new LinkedHashMap<>();
        List<String> contentCids = new ArrayList<>();
        int contractCount = 0;

        for (AuthoredSlab slab : slabs) {
            for (ContractDecl d : slab.contracts) {
                ClaimEnvelope.MintContractArgs args = new ClaimEnvelope.MintContractArgs();
                args.contractName = d.name;
                args.pre = d.pre;
                args.post = d.post;
                args.inv = d.inv;
                args.outBinding = d.outBinding;
                args.producedBy = PRODUCED_BY;
                args.producedAt = DECLARED_AT;
                args.inputCids = List.of();
                args.authoring = new ClaimEnvelope.Authoring.KitAuthor(
                    PRODUCED_BY,
                    "self-contract from " + slab.path);
                args.signerSeed = FOUNDATION_SEED;

                // Compute signer-independent content CID BEFORE minting (spec #94).
                String contentCid = ClaimEnvelope.contractCid(args);
                contentCids.add(contentCid);

                ClaimEnvelope.MintedEnvelope minted = ClaimEnvelope.mintContract(args);
                if (members.containsKey(minted.cid)) {
                    throw new IllegalStateException(
                        "duplicate attestation CID across slabs (contract `" + d.name + "`)");
                }
                members.put(minted.cid, minted.canonicalBytes);
                contractCount++;
            }
        }

        // Catalog signer CID = BLAKE3-512 of the self-identifying pubkey string.
        // Mirrors the rust peer (`signer_cid = blake3_512_of(signer_pubkey.as_bytes())`).
        String pubkeyString = Ed25519.pubkeyString(FOUNDATION_SEED);
        String signerCid = Blake3.blake3_512(pubkeyString.getBytes(java.nio.charset.StandardCharsets.UTF_8));

        ProofEnvelope.Input input = new ProofEnvelope.Input(
            CATALOG_NAME,
            CATALOG_VERSION,
            members,
            signerCid,
            FOUNDATION_SEED,
            DECLARED_AT);
        ProofEnvelope.Output built = ProofEnvelope.build(input);

        Path proofPath = outDir.resolve(built.cid + ".proof");
        Files.write(proofPath, built.bytes);

        String contractSetCid = ClaimEnvelope.computeContractSetCid(contentCids);
        return new MintResult(built.cid, contractSetCid, contractCount, slabs.size(), built.bytes);
    }

    /**
     * CLI entry point. Mints once into {@code outDir}, asserts byte-determinism
     * by minting again into a sibling _determinism_check directory, and
     * prints a human-readable report. Exits 0 on success, 1 on
     * determinism failure.
     */
    public static void runCli(String outDirArg) throws IOException {
        Path outDir = Paths.get(outDirArg);

        System.out.println("== ProvekIt Java self-contracts orchestrator ==");
        System.out.println();
        System.out.println("output dir: " + outDir.toAbsolutePath());
        System.out.println();

        System.out.println("== mint #1 ==");
        MintResult run1 = mintOneRun(outDir);
        printReport(run1, outDir);

        Path detDir = outDir.resolve("_determinism_check");
        System.out.println();
        System.out.println("== mint #2 (determinism check) ==");
        MintResult run2 = mintOneRun(detDir);

        if (!run1.cid.equals(run2.cid) || !run1.contractSetCid.equals(run2.contractSetCid)) {
            System.err.println("DETERMINISM FAILURE:");
            System.err.println("  run 1 cid:              " + run1.cid);
            System.err.println("  run 2 cid:              " + run2.cid);
            System.err.println("  run 1 contractSetCid:   " + run1.contractSetCid);
            System.err.println("  run 2 contractSetCid:   " + run2.contractSetCid);
            System.exit(1);
        }
        System.out.println("  determinism check:  OK (two runs produced identical CIDs)");
        System.out.println();
        System.out.printf("== done. Java self-application: live (%d contracts across %d slabs). ==%n",
            run1.contractCount, run1.slabCount);
    }

    private static void printReport(MintResult r, Path outDir) {
        System.out.println("  contracts:        " + r.contractCount + " across " + r.slabCount + " slabs");
        System.out.println("  catalog CID:      " + r.cid);
        System.out.println("  contractSetCid:   " + r.contractSetCid);
        System.out.println("  proof bytes:      " + r.bytes.length);
        System.out.println("  .proof file:      " + outDir.resolve(r.cid + ".proof"));
    }
}
