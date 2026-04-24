import Parser from "tree-sitter";
import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join, dirname, relative } from "path";
import { createHash } from "crypto";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { ContextBundle, CallSiteContext } from "./ContextPhase";
import { verifyAll, verifyBlock, VerificationResult, proofComplexity, extractReason } from "../verifier";
import { PrincipleStore } from "../principles";
import { Contract, ContractStore, signalKey, ClauseHistory, ProvenProperty, Violation } from "../contracts";
import { computeSignalHash } from "../signals";
import { parseFile } from "../parser";
import { LLMProvider, createProvider } from "../llm";
import { classifyAndGeneralize } from "../principles";
import { ObservationStore, ASTContext } from "../observations";
import { DagExecutor } from "./DagExecutor";
import { buildSignalFrame } from "./PromptStrategy";
import { assembleDossier, formatDossier } from "./Dossier";
import { TemplateEngine } from "../templates";
import { judgeTeachingExample } from "../judge";
import { refineErrorBlock } from "../refiner";

export interface DerivationOutput {
  contracts: Contract[];
  newViolations: { violation: VerificationResult; context: string }[];
  derivedAt: string;
}

interface FunctionNode {
  functionName: string;
  filePath: string;
  relativePath: string;
  signals: { callSite: CallSiteContext; key: string }[];
}

export interface DerivationInput {
  bundles: ContextBundle[];
  model: string;
  provider?: LLMProvider;
  maxConcurrency?: number;
}

export class DerivationPhase extends Phase<DerivationInput, DerivationOutput> {
  readonly name = "Contract Derivation";
  readonly phaseNumber = 3;

  async execute(input: DerivationInput, options: PhaseOptions): Promise<PhaseResult<DerivationOutput>> {
    const { bundles, model } = input;
    const provider = input.provider || createProvider();
    const maxConcurrency = input.maxConcurrency || 5;

    this.log("Deriving contracts...");
    this.detail(`Provider: ${provider.name}`);

    const principleStore = new PrincipleStore(options.projectRoot);
    let discoveredPrinciples = principleStore.formatForPrompt();
    const observationStore = new ObservationStore(options.projectRoot);

    this.detail(`Model: ${model}`);
    this.detail(`Principles: ${principleStore.getPrincipleCount()} (${principleStore.getAll().length} discovered), ${observationStore.getAll().length} observations`);

    const store = new ContractStore(options.projectRoot);

    const functionNodes = new Map<string, FunctionNode>();
    let totalSignals = 0;

    for (const bundle of bundles) {
      for (const callSite of bundle.callSites) {
        const fnKey = `${bundle.relativePath}/${callSite.functionName}`;
        if (!functionNodes.has(fnKey)) {
          functionNodes.set(fnKey, {
            functionName: callSite.functionName,
            filePath: bundle.filePath,
            relativePath: bundle.relativePath,
            signals: [],
          });
        }
        const key = signalKey(bundle.relativePath, callSite.functionName, callSite.line);
        functionNodes.get(fnKey)!.signals.push({ callSite, key });
        totalSignals++;
      }
    }

    this.detail(`${totalSignals} signals in ${functionNodes.size} functions`);

    const dag = new DagExecutor<FunctionNode, Contract[]>(maxConcurrency);
    let depEdges = 0;

    for (const [fnKey, node] of functionNodes) {
      const calleesSet = new Set<string>();
      for (const s of node.signals) {
        for (const c of s.callSite.callees || []) calleesSet.add(c);
      }
      const deps: string[] = [];
      for (const calleeName of calleesSet) {
        for (const [otherKey, otherNode] of functionNodes) {
          if (otherKey !== fnKey && otherNode.functionName === calleeName) {
            deps.push(otherKey);
          }
        }
      }
      depEdges += deps.length;
      dag.add({ key: fnKey, data: node, dependsOn: deps });
    }

    console.log(`  DAG: ${functionNodes.size} functions, ${totalSignals} signals, ${depEdges} call-graph edges, max ${maxConcurrency} concurrent`);
    console.log(`  One LLM call per function. Each function waits for functions it calls to resolve.`);
    console.log();

    const templateEngine = new TemplateEngine(options.projectRoot);
    const allContracts: Contract[] = [];
    const functionSources = new Map<string, string>();
    const allNewViolations: { violation: VerificationResult; context: string }[] = [];
    const allUnverifiedSignals: {
      signalKey: string; line: number; signalType: string; signalText: string;
      astContext: ASTContext; functionName: string; filePath: string;
    }[] = [];
    let completedFunctions = 0;
    let totalTemplateProofs = 0;
    const startTime = Date.now();
    const allPrinciples = principleStore.getAll();
    const principleList = allPrinciples.map((p) => `- ${p.id}: ${p.name}`).join("\n");
    const systemPrompt = `You are a formal verification engine. Produce SMT-LIB 2 formulas.

Every block MUST:
- Use \`\`\`smt2 fences
- Include (check-sat)
- Tag with ; PRINCIPLE: <id> or [NEW]
- Tag with ; LINE: <number>

Known principles:
${principleList}

Tag each block with the principle ID it matches. If a violation genuinely does not fit ANY known principle — do NOT stretch a principle to fit. Tag it [NEW]. Novel patterns are valuable — they become new principles automatically.

When you tag [NEW], the system will:
1. Save it as an observation with its AST context
2. Look for similar observations across other functions
3. When enough cluster, extract the common pattern into a new principle
4. That principle becomes a mechanical template — no LLM needed next time

So every [NEW] tag teaches the system something it will remember forever. Be eager to tag [NEW] when the pattern is genuinely novel.`;

    await dag.execute(async (node, resolvedDeps) => {
      const fn = node.data;
      completedFunctions++;

      const depContracts: Contract[] = [];
      for (const [, contracts] of resolvedDeps) {
        depContracts.push(...contracts);
      }
      const depKeys = depContracts.map((c) => c.key);
      const contextAccumulated = store.formatForPrompt(depKeys);

      const pct = Math.round((completedFunctions / functionNodes.size) * 100);
      console.log(`  [${completedFunctions}/${functionNodes.size}] (${pct}%) ${fn.relativePath}/${fn.functionName} — ${fn.signals.length} signals, ${resolvedDeps.size} deps`);

      const callSites = fn.signals.map((s) => s.callSite);
      const fnKey = `${fn.relativePath}/${fn.functionName}`;
      if (callSites[0]?.functionSource) {
        functionSources.set(fnKey, callSites[0].functionSource);
      }

      // Step 1: Run mechanical templates — instant, no LLM
      const fnNode = this.findFunctionNode(fn, options.projectRoot);
      const templateResults = fnNode
        ? templateEngine.generateProofs(fnNode, fn.functionName, fn.relativePath)
        : [];

      const templateVerifications: VerificationResult[] = [];
      for (const tr of templateResults) {
        const { result, error, witness } = verifyBlock(tr.smt2);
        templateVerifications.push({
          smt2: tr.smt2,
          z3Result: result,
          principle: tr.principle,
          error,
          witness,
          complexity: proofComplexity(tr.smt2),
          confidence: tr.confidence,
        });
      }
      let retryBudget = 2;
      for (const tv of templateVerifications) {
        if (retryBudget <= 0) break;
        if (tv.z3Result !== "error" && tv.z3Result !== "unknown") continue;
        retryBudget--;
        const claim = tv.smt2.split("\n").find((l) => l.trim().startsWith(";") && !/^;\s*(PRINCIPLE|LINE|REASON):/i.test(l.trim().replace(/^;\s*/, "")))?.trim().replace(/^;\s*/, "") || "(no claim)";
        const refined = await refineErrorBlock({
          smt2: tv.smt2,
          z3Error: tv.error || "",
          claim,
          provider,
          model,
        });
        if (refined && (refined.result === "sat" || refined.result === "unsat")) {
          tv.smt2 = refined.smt2;
          tv.z3Result = refined.result;
          tv.error = undefined;
          tv.judgeNote = (tv.judgeNote ? tv.judgeNote + "; " : "") + "retry-refined after Z3 error";
          console.log(`    retry: recovered ${refined.result} for ${tv.principle || "?"}`);
        }
      }

      totalTemplateProofs += templateVerifications.filter((v) => v.z3Result === "sat" || v.z3Result === "unsat").length;

      for (const tv of templateVerifications) {
        if (!tv.principle) continue;
        const verdict: "proven" | "violation" | "error" =
          tv.z3Result === "unsat" ? "proven" :
          tv.z3Result === "sat" ? "violation" : "error";
        principleStore.recordUse(tv.principle, verdict);
      }

      // Step 2: Build template context for the LLM — show what's already proven
      const templateContext = templateVerifications.length > 0
        ? "\n\n#### Mechanical proofs (already verified by Z3 — do NOT re-derive these):\n" +
          templateVerifications.map((v) => {
            const claim = v.smt2.split("\n").find((l) => l.startsWith("; ") && !l.startsWith("; PRINCIPLE") && !l.startsWith("; LINE"))?.replace(/^;\s*/, "") || "";
            return `  ${v.z3Result}: ${claim.slice(0, 80)}`;
          }).join("\n")
        : "";

      // Step 3: Record uncovered signals for pattern-gap discovery
      const templateCoveredLines = new Set(templateVerifications.filter((v) => v.z3Result === "sat" || v.z3Result === "unsat").map((v) => {
        const lineMatch = v.smt2.match(/;\s*LINE:\s*(\d+)/i);
        return lineMatch ? parseInt(lineMatch[1]!, 10) : -1;
      }));
      const uncoveredSignals = callSites.filter((cs) => !templateCoveredLines.has(cs.line));

      if (uncoveredSignals.length > 0) {
        for (const cs of uncoveredSignals) {
          const astCtx = this.extractASTContext(fn.filePath, cs.line, fnNode);
          if (astCtx) {
            allUnverifiedSignals.push({
              signalKey: `${fn.relativePath}/${fn.functionName}[${cs.line}]`,
              line: cs.line,
              signalType: cs.signalType,
              signalText: cs.signalText,
              astContext: astCtx,
              functionName: fn.functionName,
              filePath: fn.filePath,
            });
          }
        }
        console.log(`    ${templateVerifications.length} template, ${uncoveredSignals.length} uncovered (queued for pattern-gap analysis)`);
      } else {
        console.log(`    ${templateVerifications.length} template, all signals covered`);
      }

      const verifications = [...templateVerifications];

      const contracts = this.buildContracts(fn, verifications, depKeys, principleStore);

      const newKeys = new Set(contracts.map((c) => c.key));
      const existingForFn = store.getAll().filter((c) =>
        c.file === fn.filePath && c.function === fn.functionName && !newKeys.has(c.key)
      );
      for (const stale of existingForFn) {
        store.remove(stale.key);
      }

      for (const contract of contracts) {
        store.put(contract);
      }

      // Inline Phase 4: classify [NEW] violations immediately
      for (const contract of contracts) {
        for (const v of contract.violations) {
          if (!v.principle?.toUpperCase().includes("NEW")) continue;

          const astContext = this.extractASTContext(fn.filePath, contract.line, fnNode);
          const obsId = observationStore.nextId();
          observationStore.add({
            id: obsId,
            signalKey: contract.key,
            claim: v.claim,
            smt2: v.smt2,
            astContext,
            rejectedPrincipleName: "",
            rejectedPrincipleDescription: "",
            adversaryFeedback: "",
            observedAt: new Date().toISOString(),
          });
          console.log(`    [NEW] observation ${obsId} in ${contract.key} — attempting to generalize into principle...`);

          const violation = { smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined, complexity: 0 };
          const principle = await classifyAndGeneralize(
            violation, contract.key, principleStore.getAll(), model, provider
          );

          if (principle) {
            principle.id = principleStore.nextId();
            if (principle.validated) {
              principleStore.add(principle);
              discoveredPrinciples = principleStore.formatForPrompt();
              console.log(`    PROMOTED: observation ${obsId} → ${principle.id} — ${principle.name}`);
              console.log(`    Subsequent derivations will use this principle.`);
            } else {
              console.log(`    REJECTED as principle: ${principle.name}`);
              console.log(`    Observation ${obsId} remains (the bug is real, the generalization didn't survive)`);
            }
          }
        }
      }

      const totalProofs = contracts.reduce((n, c) => n + c.proven.length + c.violations.length, 0);
      const tpCount = templateVerifications.filter((v) => v.z3Result === "sat" || v.z3Result === "unsat").length;
      console.log(`    ${tpCount} template -> ${contracts.length} contracts, ${totalProofs} proofs`);

      return contracts;
    }, (fnKey, contracts) => {
      allContracts.push(...contracts);
      for (const c of contracts) {
        const newViolations = c.violations
          .filter((v) => v.principle?.toUpperCase().includes("NEW"))
          .map((v) => ({ violation: { smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined, complexity: 0 }, context: c.key }));
        allNewViolations.push(...newViolations);
      }
      const p = contracts.reduce((n, c) => n + c.proven.length, 0);
      const v = contracts.reduce((n, c) => n + c.violations.length, 0);
      if (p + v > 0) {
        console.log(`    >> ${fnKey}: ${p} proven, ${v} violations`);
      }
    });

    // Pattern-gap discovery: cluster unverified signals by AST shape
    if (allUnverifiedSignals.length > 0) {
      console.log(`  Pattern gaps: ${allUnverifiedSignals.length} unverified signals across ${functionNodes.size} functions`);

      const gaps = new Map<string, typeof allUnverifiedSignals>();
      for (const sig of allUnverifiedSignals) {
        const key = `${sig.astContext.nodeType}:${sig.astContext.operator || ""}:${sig.astContext.method || ""}`;
        if (!gaps.has(key)) gaps.set(key, []);
        gaps.get(key)!.push(sig);
      }

      const MIN_CLUSTER = 3;
      for (const [gapKey, signals] of gaps) {
        if (signals.length < MIN_CLUSTER) {
          console.log(`    gap ${gapKey}: ${signals.length} signals (below threshold ${MIN_CLUSTER}, accumulating)`);
          for (const sig of signals) {
            observationStore.add({
              id: observationStore.nextId(),
              signalKey: sig.signalKey,
              claim: `Unverified: ${sig.signalText.slice(0, 80)}`,
              smt2: "",
              astContext: sig.astContext,
              rejectedPrincipleName: "",
              rejectedPrincipleDescription: "",
              adversaryFeedback: "",
              observedAt: new Date().toISOString(),
            });
          }
          continue;
        }

        console.log(`    gap ${gapKey}: ${signals.length} signals — triggering LLM for new principle...`);

        const examples = signals.slice(0, 5).map((s, i) =>
          `${i + 1}. ${s.signalKey} [${s.signalType}]: ${s.signalText.slice(0, 100)}\n   AST: ${JSON.stringify(s.astContext)}`
        ).join("\n");

        const existingList = principleStore.getAll().map((p) =>
          `- ${p.id}: ${p.name}`
        ).join("\n");

        const exemplars = principleStore.getAll().filter((p) => p.astPatterns && p.smt2Template).slice(0, 3);
        const exemplarText = exemplars.map((p) =>
          `### ${p.id}: ${p.name}\nDescription: ${p.description}\nastPatterns: ${JSON.stringify(p.astPatterns, null, 2)}\nsmt2Template: ${p.smt2Template}\nTeaching example (${p.teachingExample.domain}): ${p.teachingExample.explanation}\nTeaching SMT-LIB:\n${p.teachingExample.smt2}`
        ).join("\n\n");

        const { LessonStore } = require("../lessons");
        const lessonsText = new LessonStore(options.projectRoot).formatForPrompt(8);

        try {
          const response = await provider.complete(`You are discovering a new atomic verification principle.

## Pattern gap

${signals.length} code locations share the same AST shape but no existing principle covers them:

${examples}

Common AST shape: ${gapKey}

## Existing principles (yours must be DIFFERENT)
${existingList}

## Examples of existing principle structure
${exemplarText}
${lessonsText}
## Your task

Produce ONE atomic principle for this pattern gap. It must have:
- A short name (2-4 words)
- A description stating the exact bug pattern
- An astPatterns array matching the AST shape above
- An smt2Template with {{variable}} holes that Z3 can check after filling
- A teachingExample in a completely different domain

\`\`\`json
{
  "name": "...",
  "description": "...",
  "astPatterns": [{"nodeType": "...", ...}],
  "smt2Template": "...",
  "teachingExample": {"domain": "...", "explanation": "...", "smt2": "..."}
}
\`\`\``, { model, systemPrompt: "You produce atomic verification principles — one AST pattern, one Z3 template, one bug. Respond with JSON only." });

          const jsonMatch = response.text.match(/```json\s*([\s\S]*?)```/);
          if (jsonMatch) {
            const parsed = JSON.parse(jsonMatch[1]!.trim());
            const judge = await judgeTeachingExample(
              {
                name: parsed.name,
                description: parsed.description,
                explanation: parsed.teachingExample.explanation,
                smt2: parsed.teachingExample.smt2,
              },
              provider,
              model
            );
            const newPrinciple = {
              id: parsed.name.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9-]/g, ""),
              name: parsed.name,
              description: parsed.description,
              astPatterns: parsed.astPatterns,
              smt2Template: parsed.smt2Template,
              teachingExample: parsed.teachingExample,
              provenance: {
                discoveredIn: signals.map((s) => s.signalKey).slice(0, 5).join(", "),
                violation: `Discovered from ${signals.length} unverified signals`,
                generalizedAt: new Date().toISOString(),
              },
              validated: judge.valid,
              ...(judge.valid ? {} : { validationFailure: `judge-rejected: ${judge.note}` }),
            };
            if (judge.valid) {
              principleStore.add(newPrinciple);
              console.log(`    DISCOVERED: ${newPrinciple.id} — ${newPrinciple.name} (from ${signals.length} signals)`);
            } else {
              console.log(`    REJECTED (judge): ${newPrinciple.name} — ${judge.note}`);
            }
          }
        } catch (err: any) {
          console.log(`    ERROR discovering principle for ${gapKey}: ${err.message?.slice(0, 60)}`);
        }
      }
    }

    principleStore.persistStats();
    const retired = principleStore.evaluateRetirements();
    if (retired.length > 0) {
      for (const r of retired) {
        console.log(`  RETIRED: ${r.id} — ${r.reason}`);
      }
    }

    const cegarStats = await this.cegarRefineViolations(
      allContracts,
      functionSources,
      provider,
      model,
      store
    );
    if (cegarStats.total > 0) {
      this.detail(`CEGAR: ${cegarStats.confirmed} confirmed | ${cegarStats.refined} refined | ${cegarStats.flipped} flipped to proven | ${cegarStats.skipped} skipped`);
    }

    const output: DerivationOutput = {
      contracts: allContracts,
      newViolations: allNewViolations,
      derivedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "derivation.json");
    writeFileSync(outPath, JSON.stringify({ derivedAt: output.derivedAt, contractCount: allContracts.length }, null, 2));

    const totalProven = allContracts.reduce((n, c) => n + c.proven.length, 0);
    const totalViolations = allContracts.reduce((n, c) => n + c.violations.length, 0);
    const unattributed = allContracts.filter((c) => c.proven.length === 0 && c.violations.length === 0).length;
    this.detail(`Derivation complete: ${this.formatDuration(Date.now() - startTime)}`);
    this.detail(`  ${allContracts.length} contracts across ${functionNodes.size} functions`);
    this.detail(`  ${totalProven} proven (unsat) | ${totalViolations} violations (sat) | ${unattributed} unattributed`);
    this.detail(`  ${allNewViolations.length} [NEW] violations for Phase 4`);
    if (unattributed > 0) {
      this.detail(`  WARNING: ${unattributed} signals received zero attributed SMT-LIB blocks`);
    }
    console.log();

    return { data: output, writtenTo: outPath };
  }

  private buildContracts(
    fn: FunctionNode,
    verifications: VerificationResult[],
    dependencyKeys: string[],
    principleStore: PrincipleStore
  ): Contract[] {
    const contracts: Contract[] = [];

    const signalLines = fn.signals.map((s) => s.callSite.line);

    for (const { callSite, key } of fn.signals) {
      const lineVerifications = verifications.filter((v) => {
        const lineMatch = v.smt2.match(/;\s*LINE:\s*(\d+)/i);
        if (!lineMatch || !lineMatch[1]) return false;
        const taggedLine = parseInt(lineMatch[1], 10);
        if (taggedLine === callSite.line) return true;
        const nearest = signalLines.reduce((best, sl) =>
          Math.abs(sl - taggedLine) < Math.abs(best - taggedLine) ? sl : best
        );
        return nearest === callSite.line;
      });

      const unmatched = lineVerifications.length === 0;

      const toUse = unmatched && fn.signals.length === 1 ? verifications : lineVerifications;

      const proven: ProvenProperty[] = [];
      const violations: Violation[] = [];

      for (const v of toUse) {
        const commentLines = v.smt2.split("\n").filter((l) => l.trim().startsWith(";")).map((l) => l.trim().replace(/^;\s*/, ""));
        const claim = commentLines.find((l) => !l.startsWith("PRINCIPLE:") && !l.startsWith("LINE:") && l.length > 10) || "(no claim extracted)";
        const pHash = v.principle ? this.resolvePrincipleHash(v.principle, principleStore) : "";

        if (v.z3Result === "unsat") {
          proven.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2 });
        } else if (v.z3Result === "sat") {
          violations.push({ principle: v.principle, principle_hash: pHash, claim, smt2: v.smt2, witness: v.witness, complexity: v.complexity, confidence: v.confidence });
        }
      }

      contracts.push({
        key,
        file: fn.relativePath,
        function: callSite.functionName,
        line: callSite.line,
        signal_hash: callSite.signalHash,
        proven,
        violations,
        depends_on: dependencyKeys,
        clause_history: [
          ...proven.map((p) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
          ...violations.map((v) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
        ],
      });
    }

    return contracts;
  }

  private resolvePrincipleHash(principleTag: string, store: PrincipleStore): string {
    const ids = principleTag.replace(/\[NEW\]/gi, "").split(/[,+&\s]+/).map((s) => s.trim()).filter((s) => /^P\d+$/i.test(s));
    if (ids.length === 0) return "";
    if (ids.length === 1) return store.hashForPrinciple(ids[0]!);
    const combined = createHash("sha256");
    for (const id of ids.sort()) { combined.update(id); combined.update(store.hashForPrinciple(id)); }
    return combined.digest("hex");
  }

  private extractASTContext(filePath: string, line: number, fnNode: Parser.SyntaxNode | null): ASTContext | undefined {
    try {
      const source = readFileSync(filePath, "utf-8");
      const tree = parseFile(source);
      if (line < 1) return undefined;
      const targetRow = line - 1;

      let bestNode: Parser.SyntaxNode | null = null;
      const findNode = (node: Parser.SyntaxNode): void => {
        if (node.startPosition.row <= targetRow && node.endPosition.row >= targetRow) {
          bestNode = node;
          for (const child of node.children) findNode(child);
        }
      };
      findNode(tree.rootNode);
      if (!bestNode) return undefined;

      const node: Parser.SyntaxNode = bestNode;
      let operator: string | undefined;
      let method: string | undefined;

      if (node.type === "binary_expression") {
        for (const child of node.children) {
          if (["+", "-", "*", "/", "%", "||", "&&", "??"].includes(child.type)) {
            operator = child.type;
          }
        }
      }

      if (node.type === "call_expression") {
        const fn = node.childForFieldName("function");
        if (fn?.type === "member_expression") {
          method = fn.childForFieldName("property")?.text;
        } else if (fn?.type === "identifier") {
          method = fn.text;
        }
      }

      const paramNames = new Set<string>();
      const enclosing = this.findEnclosingFunction(node);
      if (enclosing) {
        const params = enclosing.childForFieldName("parameters");
        if (params) {
          for (const child of params.namedChildren) {
            const nameNode = child.childForFieldName("pattern") || child.childForFieldName("name");
            if (nameNode) paramNames.add(nameNode.text);
          }
        }
      }

      const referencesParam = this.nodeReferencesParam(node, paramNames);

      const pathConditions: string[] = [];
      let current: Parser.SyntaxNode | null = node.parent;
      const fnBound = enclosing || tree.rootNode;
      while (current && current.id !== fnBound.id) {
        if (current.parent?.type === "if_statement") {
          const cond = current.parent.childForFieldName("condition");
          if (cond) pathConditions.unshift(cond.text);
        }
        current = current.parent;
      }

      let insideTryCatch = false;
      current = node.parent;
      while (current && current.id !== fnBound.id) {
        if (current.type === "try_statement") { insideTryCatch = true; break; }
        current = current.parent;
      }

      return {
        nodeType: node.type,
        operator,
        method,
        referencesParam,
        pathConditions,
        parentType: node.parent?.type,
        insideTryCatch,
      };
    } catch {
      return undefined;
    }
  }

  private findEnclosingFunction(node: Parser.SyntaxNode): Parser.SyntaxNode | null {
    let current: Parser.SyntaxNode | null = node.parent;
    while (current) {
      if (["function_declaration", "method_definition", "arrow_function", "function_expression"].includes(current.type)) return current;
      current = current.parent;
    }
    return null;
  }

  private nodeReferencesParam(node: Parser.SyntaxNode, params: Set<string>): boolean {
    if (node.type === "identifier" && params.has(node.text)) return true;
    for (const child of node.children) {
      if (this.nodeReferencesParam(child, params)) return true;
    }
    return false;
  }

  private findFunctionNode(fn: FunctionNode, projectRoot: string): Parser.SyntaxNode | null {
    try {
      const source = readFileSync(fn.filePath, "utf-8");
      const tree = parseFile(source);
      const targetLine = fn.signals[0]?.callSite.line || 0;

      let bestFn: Parser.SyntaxNode | null = null;
      const visit = (node: Parser.SyntaxNode): void => {
        if (node.type === "function_declaration" || node.type === "method_definition" ||
            node.type === "arrow_function" || node.type === "function_expression") {
          const nameNode = node.childForFieldName("name");
          const name = nameNode?.text || "";
          if (name === fn.functionName || (node.parent?.type === "variable_declarator" &&
              node.parent.childForFieldName("name")?.text === fn.functionName)) {
            bestFn = node;
          }
        }
        if (node.type === "export_statement" && node.firstNamedChild?.type === "function_declaration") {
          const inner = node.firstNamedChild;
          if (inner.childForFieldName("name")?.text === fn.functionName) {
            bestFn = inner;
          }
        }
        for (const child of node.children) visit(child);
      };
      visit(tree.rootNode);
      return bestFn;
    } catch {
      return null;
    }
  }

  private stripNegatedGoal(smt2: string): string {
    const lines = smt2.split("\n");
    const assertIndices: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.trim().startsWith("(assert")) assertIndices.push(i);
    }
    if (assertIndices.length === 0) return smt2;
    const goalIdx = assertIndices[assertIndices.length - 1]!;
    return lines.filter((_, i) => i !== goalIdx).join("\n");
  }

  private formatDuration(ms: number): string {
    if (ms < 1000) return `${Math.round(ms)}ms`;
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ${s % 60}s`;
    return `${Math.floor(m / 60)}h ${m % 60}m`;
  }

  private async cegarRefineViolations(
    contracts: Contract[],
    functionSources: Map<string, string>,
    provider: LLMProvider,
    model: string,
    store: ContractStore,
    maxCalls: number = 10
  ): Promise<{ total: number; confirmed: number; refined: number; flipped: number; skipped: number }> {
    let confirmed = 0, refined = 0, flipped = 0, skipped = 0, used = 0;

    for (const contract of contracts) {
      if (used >= maxCalls) {
        skipped += contract.violations.filter((v) => v.witness).length;
        continue;
      }

      const relPath = contract.key.split("/").slice(0, -1).join("/");
      const fnKey = `${relPath}/${contract.function}`;
      const source = functionSources.get(fnKey) || "(source unavailable)";
      let mutated = false;

      for (let i = 0; i < contract.violations.length; i++) {
        if (used >= maxCalls) { skipped++; continue; }
        const v = contract.violations[i]!;
        if (!v.witness) continue;
        if (v.judge_note?.startsWith("cegar-")) continue;
        used++;

        const prompt = `Z3 reports a violation. Decide whether the counterexample is a real reachable bug or an artifact of an incomplete SMT encoding.

## Code
\`\`\`typescript
${source}
\`\`\`

## Claim
${v.claim}

## SMT-LIB encoding
\`\`\`smt2
${v.smt2}
\`\`\`

## Z3 counterexample
\`\`\`
${v.witness}
\`\`\`

## Your task

Reply with exactly one of:

(1) If the counterexample is reachable from real code execution, reply on ONE line:
REACHABLE: <one-sentence attack path that a reader can verify by reading the code>

(2) If the counterexample is spurious — Z3 invented values the code actually rules out — emit a revised SMT-LIB block that adds the missing precondition, keeps \`; PRINCIPLE:\` and \`; LINE:\` tags, and adds \`; REASON:\` stating what was missing:
\`\`\`smt2
<revised block>
(check-sat)
\`\`\`

Do not restate the original block unchanged. Do not emit both a REACHABLE line and a block — pick one.`;

        let response;
        try {
          response = await provider.complete(prompt, {
            model,
            systemPrompt: "You refine SMT-LIB encodings based on Z3 counterexamples. Reply with REACHABLE: <path> on one line OR a single revised smt2 block. Nothing else.",
          });
        } catch {
          continue;
        }

        const text = response.text.trim();
        const firstLine = text.split("\n")[0] || "";
        if (/^REACHABLE\b/i.test(firstLine)) {
          v.confidence = "high";
          v.judge_note = `cegar-confirmed: ${firstLine.replace(/^REACHABLE:?\s*/i, "").trim().slice(0, 200)}`;
          confirmed++;
          mutated = true;
          continue;
        }

        const smtMatch = text.match(/```(?:smt2|smt-lib|smtlib2)?\s*\n([\s\S]*?)```/i);
        if (!smtMatch) continue;
        const revisedSmt = smtMatch[1]!.trim();
        if (!revisedSmt.includes("(check-sat)")) continue;
        if (revisedSmt === v.smt2) continue;

        const { result, witness: newWitness } = verifyBlock(revisedSmt);
        if (result === "unsat") {
          const premisesOnly = this.stripNegatedGoal(revisedSmt);
          const { result: premisesResult } = verifyBlock(premisesOnly);
          if (premisesResult !== "sat") {
            console.log(`    cegar-refined block is vacuously unsat (premises alone are ${premisesResult}); keeping original violation`);
            continue;
          }

          const bare = v.claim.replace(/^VIOLATION:\s*/i, "").trim();
          const flippedClaim = `PROVEN: ${bare} is prevented (CEGAR-refined encoding)`;
          contract.proven.push({
            principle: v.principle,
            principle_hash: v.principle_hash,
            claim: flippedClaim,
            smt2: revisedSmt,
            reason: extractReason(revisedSmt) || "cegar-refined precondition added",
            confidence: "high",
            judge_note: "cegar-flipped: violation became proof after tightening encoding (premises verified consistent)",
          });
          contract.violations.splice(i, 1);
          i--;
          flipped++;
          mutated = true;
        } else if (result === "sat") {
          v.smt2 = revisedSmt;
          v.witness = newWitness || v.witness;
          v.reason = extractReason(revisedSmt) || v.reason;
          v.confidence = "high";
          v.judge_note = "cegar-refined: bug persists after tightened encoding";
          refined++;
          mutated = true;
        }
      }

      if (mutated) {
        store.put(contract);
      }
    }

    return { total: confirmed + refined + flipped, confirmed, refined, flipped, skipped };
  }
}
