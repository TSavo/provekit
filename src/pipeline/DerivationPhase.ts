import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname, relative } from "path";
import { createHash } from "crypto";
import { query } from "@anthropic-ai/claude-agent-sdk";
import Handlebars from "handlebars";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContextBundle, CallSiteContext } from "./ContextPhase";
import { verifyAll, VerificationResult } from "../verifier";
import { PrincipleStore } from "../principles";
import { ClauseHistory } from "../contracts";

export interface DerivedContract {
  file: string;
  function: string;
  line: number;
  proven: { principle: string | null; claim: string; smt2: string }[];
  violations: { principle: string | null; claim: string; smt2: string }[];
  depends_on: string[];
  clause_history: ClauseHistory[];
}

export interface DerivationOutput {
  contracts: DerivedContract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

export interface DerivationInput {
  bundles: ContextBundle[];
  model: string;
}

export class DerivationPhase extends Phase<DerivationInput, DerivationOutput> {
  readonly name = "Contract Derivation";
  readonly phaseNumber = 3;

  private compiledTemplate: HandlebarsTemplateDelegate;

  constructor() {
    super();
    this.compiledTemplate = this.loadTemplate();
  }

  private loadTemplate(): HandlebarsTemplateDelegate {
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
      } catch { continue; }
    }

    throw new Error("Could not find prompts/invariant_derivation.md");
  }

  async execute(input: DerivationInput, options: PhaseOptions): Promise<PhaseResult<DerivationOutput>> {
    const { bundles, model } = input;

    this.log("Deriving contracts...");

    const principleStore = new PrincipleStore(options.projectRoot);
    const discoveredPrinciples = principleStore.formatForPrompt();
    const principleHash = principleStore.computePrincipleHash();
    const discoveredCount = principleStore.getAll().length;

    this.detail(`Model: ${model}`);
    this.detail(`Principles: 7 seed${discoveredCount > 0 ? ` + ${discoveredCount} discovered` : ""}`);
    this.detail(`Principle hash: ${principleHash.slice(0, 12)}...`);
    this.detail(`Bundles: ${bundles.length} files, ${bundles.reduce((n, b) => n + b.callSites.length, 0)} call sites`);
    console.log();

    const allContracts: DerivedContract[] = [];
    const allNewViolations: { violation: VerificationResult; context: string }[] = [];
    let accumulated = "(no existing contracts yet -- first pass)";

    for (const bundle of bundles) {
      this.detail(`${bundle.relativePath}:`);

      for (const callSite of bundle.callSites) {
        process.stdout.write(
          `    ${callSite.functionName}:${callSite.line} (${allContracts.length} in context) ... `
        );

        const prompt = this.buildPrompt(callSite, bundle.filePath, accumulated, discoveredPrinciples);

        let rawResponse = "";
        for await (const message of query({
          prompt,
          options: {
            model,
            includePartialMessages: true,
            systemPrompt: `You are a formal verification engine. Produce SMT-LIB 2 formulas. Every block MUST use \`\`\`smt2 fences and include (check-sat). Tag every block with ; PRINCIPLE: P1-P7 or [NEW].`,
          },
        })) {
          if (options.verbose && message.type === "stream_event") {
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
        const contract = this.buildContract(bundle.filePath, callSite.functionName, callSite.line, verifications, allContracts);
        allContracts.push(contract);

        const newViolations = verifications
          .filter((v) => v.z3Result === "sat" && v.principle?.toUpperCase().includes("NEW"))
          .map((v) => ({ violation: v, context: `${bundle.filePath}:${callSite.functionName}:${callSite.line}` }));
        allNewViolations.push(...newViolations);

        accumulated = this.formatAccumulated(allContracts);

        const proven = verifications.filter((v) => v.z3Result === "unsat").length;
        const violations = verifications.filter((v) => v.z3Result === "sat").length;
        const newCount = newViolations.length;

        console.log(
          `${verifications.length} blocks: ${proven} proven, ${violations} violations` +
            (newCount > 0 ? ` (${newCount} [NEW])` : "")
        );
      }

      this.writeContractsForFile(bundle.filePath, allContracts, options.projectRoot, principleHash);
    }

    const output: DerivationOutput = {
      contracts: allContracts,
      newViolations: allNewViolations,
      derivedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "derivation.json");
    writeFileSync(outPath, JSON.stringify(output, null, 2));

    const totalProven = allContracts.reduce((n, c) => n + c.proven.length, 0);
    const totalViolations = allContracts.reduce((n, c) => n + c.violations.length, 0);
    this.detail(`Derivation complete:`);
    this.detail(`  ${allContracts.length} contracts across ${bundles.length} files`);
    this.detail(`  ${totalProven} proven (unsat) | ${totalViolations} violations (sat)`);
    this.detail(`  ${allNewViolations.length} [NEW] violations for Phase 4`);
    console.log();

    return { data: output, writtenTo: outPath };
  }

  private buildPrompt(
    callSite: CallSiteContext,
    filePath: string,
    accumulated: string,
    discoveredPrinciples: string
  ): string {
    const importSources = callSite.importSources.length > 0
      ? callSite.importSources
          .map((imp) => `#### ${imp.path}\n\`\`\`typescript\n${imp.source}\n\`\`\``)
          .join("\n\n")
      : "(no imports)";

    let enrichedContext = callSite.callingContext;

    if (callSite.typeContext && callSite.typeContext !== "(no type annotations found)") {
      enrichedContext += `\n\nType information (from TypeScript AST):\n${callSite.typeContext}`;
    }

    if (callSite.pathConditions && callSite.pathConditions.length > 0) {
      enrichedContext += `\n\nPath conditions (must be true for execution to reach this signal):\n`;
      enrichedContext += callSite.pathConditions.map((c, i) => `  ${i + 1}. ${c}`).join("\n");
      enrichedContext += `\nThese are KNOWN FACTS at this signal -- the code guarantees them.`;
    }

    let prompt = this.compiledTemplate({
      TARGET_FILE: filePath,
      TARGET_FUNCTION: callSite.functionName,
      TARGET_LINE: String(callSite.line),
      TARGET_STATEMENT: callSite.signalText,
      TARGET_FILE_SOURCE: callSite.fileSource,
      IMPORT_SOURCES: importSources,
      EXISTING_CONTRACTS: accumulated,
      CALLING_CONTEXT: enrichedContext,
    });

    if (discoveredPrinciples) {
      const insertPoint = prompt.indexOf("### SMT-LIB 2 Grammar");
      if (insertPoint !== -1) {
        prompt = prompt.slice(0, insertPoint) + discoveredPrinciples + "\n\n" + prompt.slice(insertPoint);
      }
    }

    return prompt;
  }

  private buildContract(
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

      const claim = commentLines.find((l) => !l.startsWith("PRINCIPLE:") && l.length > 10) || "(no claim extracted)";

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

  private formatAccumulated(contracts: DerivedContract[]): string {
    if (contracts.length === 0) return "(no existing contracts yet -- first pass)";

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

  private writeContractsForFile(
    filePath: string,
    allContracts: DerivedContract[],
    projectRoot: string,
    principleHash?: string
  ): void {
    const relPath = relative(projectRoot, filePath);
    const contractPath = join(projectRoot, ".neurallog", "contracts", relPath + ".json");
    const dir = dirname(contractPath);
    mkdirSync(dir, { recursive: true });

    const fileSource = readFileSync(filePath, "utf-8");
    const fileHash = createHash("md5").update(fileSource).digest("hex");
    const contractsForFile = allContracts.filter((c) => c.file === filePath);

    writeFileSync(contractPath, JSON.stringify({
      file_hash: fileHash,
      ...(principleHash ? { principle_hash: principleHash } : {}),
      contracts: contractsForFile,
    }, null, 2));
  }
}
