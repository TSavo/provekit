/**
 * Phase 3: Contract Derivation
 *
 * Input:  .neurallog/contexts/bundles.json (from Phase 2)
 * Output: .neurallog/contracts/*.json (immutable per-run)
 *
 * For each call site in each context bundle, sends the assembled prompt
 * to the LLM, gets back SMT-LIB blocks, verifies with Z3, writes contracts.
 * Contracts accumulate sequentially — each derivation sees all prior contracts.
 */

import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname } from "path";
import { createHash } from "crypto";
import { query } from "@anthropic-ai/claude-agent-sdk";
import Handlebars from "handlebars";
import { ContextBundle, CallSiteContext } from "./phase2-context";
import { verifyAll, VerificationResult } from "../verifier";

export interface DerivedContract {
  file: string;
  function: string;
  line: number;
  proven: { principle: string | null; claim: string; smt2: string }[];
  violations: { principle: string | null; claim: string; smt2: string }[];
  depends_on: string[];
  clause_history: any[];
}

export interface DerivationOutput {
  contracts: DerivedContract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

const compiledTemplate = loadTemplate();

function loadTemplate(): HandlebarsTemplateDelegate {
  const candidates = [
    join(__dirname, "..", "..", "prompts", "invariant_derivation.md"),
    join(process.cwd(), "prompts", "invariant_derivation.md"),
  ];

  for (const path of candidates) {
    try {
      const raw = readFileSync(path, "utf-8");
      const promptStart = raw.indexOf("## Prompt\n");
      const template = promptStart !== -1
        ? raw.slice(promptStart + "## Prompt\n\n".length)
        : raw;
      return Handlebars.compile(template, { noEscape: true });
    } catch {
      continue;
    }
  }

  throw new Error("Could not find prompts/invariant_derivation.md");
}

export async function deriveContracts(
  bundles: ContextBundle[],
  projectRoot: string,
  model: string,
  verbose: boolean
): Promise<DerivationOutput> {
  console.log("Phase 3: Deriving contracts...");

  const allContracts: DerivedContract[] = [];
  const allNewViolations: { violation: VerificationResult; context: string }[] = [];
  let accumulated = "(no existing contracts yet — first pass)";

  for (const bundle of bundles) {
    console.log(`  ${bundle.relativePath}:`);

    for (const callSite of bundle.callSites) {
      process.stdout.write(
        `    ${callSite.functionName}:${callSite.line} (${allContracts.length} in context) ... `
      );

      const prompt = buildPrompt(callSite, accumulated);

      let rawResponse = "";
      for await (const message of query({
        prompt,
        options: {
          model,
          includePartialMessages: true,
          systemPrompt: `You are a formal verification engine. Produce SMT-LIB 2 formulas. Every block MUST use \`\`\`smt2 fences and include (check-sat). Tag every block with ; PRINCIPLE: P1-P7 or [NEW].`,
        },
      })) {
        if (verbose && message.type === "stream_event") {
          const event = (message as any).event;
          if (event?.type === "content_block_delta" && event.delta?.type === "text_delta") {
            process.stdout.write(event.delta.text);
          }
        }
        if (message.type === "assistant") {
          const content = (message as any).message?.content;
          if (Array.isArray(content)) {
            rawResponse += content
              .filter((b: any) => b.type === "text")
              .map((b: any) => b.text)
              .join("");
          }
        }
        if (message.type === "result" && message.subtype === "success") {
          rawResponse = message.result;
        }
      }

      const verifications = verifyAll(rawResponse);

      const contract = buildContract(
        bundle.filePath,
        callSite.functionName,
        callSite.line,
        verifications,
        allContracts
      );

      allContracts.push(contract);

      // Detect [NEW] violations
      const newViolations = verifications
        .filter((v) => v.z3Result === "sat" && v.principle?.toUpperCase().includes("NEW"))
        .map((v) => ({ violation: v, context: `${bundle.filePath}:${callSite.functionName}:${callSite.line}` }));
      allNewViolations.push(...newViolations);

      // Update accumulated contracts for next derivation
      accumulated = formatAccumulated(allContracts);

      const proven = verifications.filter((v) => v.z3Result === "unsat").length;
      const violations = verifications.filter((v) => v.z3Result === "sat").length;
      const newCount = newViolations.length;

      console.log(
        `${verifications.length} blocks: ${proven} proven, ${violations} violations` +
          (newCount > 0 ? ` (${newCount} [NEW])` : "")
      );
    }

    // Write contracts for this file to disk
    writeContractsForFile(bundle.filePath, allContracts, projectRoot);
  }

  const output: DerivationOutput = {
    contracts: allContracts,
    newViolations: allNewViolations,
    derivedAt: new Date().toISOString(),
  };

  // Write full derivation output
  const outPath = join(projectRoot, ".neurallog", "derivation.json");
  writeFileSync(outPath, JSON.stringify(output, null, 2));

  console.log(`  ${allContracts.length} contracts derived, ${allNewViolations.length} [NEW] violations`);
  console.log();

  return output;
}

function buildPrompt(callSite: CallSiteContext, accumulated: string): string {
  const importSources = callSite.importSources.length > 0
    ? callSite.importSources
        .map((imp) => `#### ${imp.path}\n\`\`\`typescript\n${imp.source}\n\`\`\``)
        .join("\n\n")
    : "(no imports)";

  return compiledTemplate({
    TARGET_FILE: callSite.functionName,
    TARGET_FUNCTION: callSite.functionName,
    TARGET_LINE: String(callSite.line),
    TARGET_STATEMENT: callSite.logText,
    TARGET_FILE_SOURCE: callSite.fileSource,
    IMPORT_SOURCES: importSources,
    EXISTING_CONTRACTS: accumulated,
    CALLING_CONTEXT: callSite.callingContext,
  });
}

function buildContract(
  file: string,
  functionName: string,
  line: number,
  verifications: VerificationResult[],
  priorContracts: DerivedContract[]
): DerivedContract {
  const proven: DerivedContract["proven"] = [];
  const violations: DerivedContract["violations"] = [];

  for (const v of verifications) {
    const commentLines = v.smt2
      .split("\n")
      .filter((l) => l.trim().startsWith(";"))
      .map((l) => l.trim().replace(/^;\s*/, ""));

    const claim = commentLines.find((l) => !l.startsWith("PRINCIPLE:") && l.length > 10)
      || "(no claim extracted)";

    if (v.z3Result === "unsat") {
      proven.push({ principle: v.principle, claim, smt2: v.smt2 });
    } else if (v.z3Result === "sat") {
      violations.push({ principle: v.principle, claim, smt2: v.smt2 });
    }
  }

  const depends_on = priorContracts.map((c) => {
    const content = c.proven.map((p) => p.smt2).join("\n") + c.violations.map((v) => v.smt2).join("\n");
    return createHash("md5").update(content).digest("hex").slice(0, 12);
  });

  return {
    file,
    function: functionName,
    line,
    proven,
    violations,
    depends_on,
    clause_history: [
      ...proven.map((p) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
      ...violations.map((v) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
    ],
  };
}

function formatAccumulated(contracts: DerivedContract[]): string {
  if (contracts.length === 0) return "(no existing contracts yet — first pass)";

  return contracts.map((c) => {
    const lines: string[] = [];
    lines.push(`### ${c.file}:${c.function} (line ${c.line})`);
    if (c.proven.length > 0) {
      lines.push("\nProven (Z3 unsat):");
      for (const p of c.proven) lines.push(`  [${p.principle || "?"}] ${p.claim}`);
    }
    if (c.violations.length > 0) {
      lines.push("\nViolations (Z3 sat):");
      for (const v of c.violations) lines.push(`  [${v.principle || "?"}] ${v.claim}`);
    }
    return lines.join("\n");
  }).join("\n\n");
}

function writeContractsForFile(
  filePath: string,
  allContracts: DerivedContract[],
  projectRoot: string
): void {
  const { relative } = require("path");
  const relPath = relative(projectRoot, filePath);
  const contractPath = join(projectRoot, ".neurallog", "contracts", relPath + ".json");
  const dir = dirname(contractPath);
  mkdirSync(dir, { recursive: true });

  const fileSource = readFileSync(filePath, "utf-8");
  const fileHash = createHash("md5").update(fileSource).digest("hex");
  const contractsForFile = allContracts.filter((c) => c.file === filePath);

  writeFileSync(contractPath, JSON.stringify({
    file_hash: fileHash,
    contracts: contractsForFile,
  }, null, 2));
}
