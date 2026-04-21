import { readFileSync } from "fs";
import { resolve, isAbsolute } from "path";
import { ContractStore } from "../src/contracts";
import { createProvider } from "../src/llm";
import { synthesizeHarness, runHarness, HarnessCache } from "../src/harness";
import { parseFile } from "../src/parser";
import Parser from "tree-sitter";

async function main() {
  const projectRoot = process.cwd();
  const contractKey = process.argv[2];
  if (!contractKey) {
    console.error("usage: ts-node scripts/harness-probe.ts '<contract-key>'  [--proven | --violation]");
    console.error("example: ts-node scripts/harness-probe.ts 'examples/arithmetic.ts/safeDivide[8]'");
    process.exit(1);
  }

  const source: "proven" | "violation" = process.argv.includes("--violation") ? "violation" : "proven";
  const noSynth = process.argv.includes("--no-synth");
  const noCache = process.argv.includes("--no-cache");
  const model = process.env.NEURALLOG_HARNESS_MODEL || "claude-sonnet-4-6";

  const store = new ContractStore(projectRoot);
  const contract = store.get(contractKey);
  if (!contract) {
    console.error(`contract not found: ${contractKey}`);
    console.error(`available (first 10): ${store.getAll().slice(0, 10).map((c) => c.key).join("\n  ")}`);
    process.exit(1);
  }

  const list = source === "proven" ? contract.proven : contract.violations;
  if (list.length === 0) {
    console.error(`contract ${contractKey} has no ${source} entries`);
    process.exit(1);
  }
  const prop = list[0]!;

  const absPath = isAbsolute(contract.file) ? contract.file : resolve(projectRoot, contract.file);
  const info = extractFunctionInfo(absPath, contract.function);
  if (!info) {
    console.error(`could not extract function ${contract.function} from ${absPath}`);
    process.exit(1);
  }

  console.log("═══════════════════════════════════════════════════════");
  console.log(`contract:      ${contractKey}`);
  console.log(`source:        ${source}`);
  console.log(`function:      ${contract.function}${info.isStatic ? " (static)" : ""}${info.className ? ` on ${info.className}` : ""}`);
  console.log(`file:          ${contract.file}`);
  console.log(`principle:     ${prop.principle}`);
  console.log(`claim:         ${prop.claim}`);
  console.log("═══════════════════════════════════════════════════════");
  console.log("SMT-LIB:");
  console.log(prop.smt2);
  console.log("───────────────────────────────────────────────────────");
  console.log("Function source:");
  console.log(info.source);
  console.log("═══════════════════════════════════════════════════════");

  const cache = new HarnessCache(projectRoot);
  let harness: string | null = null;
  let untestable: string | null = null;
  let raw = "";

  if (!noCache) {
    const cached = cache.get(prop.smt2, info.source);
    if (cached) {
      harness = cached.harness || null;
      untestable = cached.untestable || null;
      console.log(`[cache] hit — ${harness ? "harness" : untestable ? "untestable" : "empty"}`);
    }
  }

  if (!harness && !untestable && !noSynth) {
    console.log(`[synth] calling ${model}…`);
    const provider = createProvider();
    const t0 = Date.now();
    const result = await synthesizeHarness(
      {
        functionSource: info.source,
        claim: prop.claim,
        smt2: prop.smt2,
        contractKey: contract.key,
        functionName: contract.function,
      },
      provider,
      model,
      projectRoot
    );
    console.log(`[synth] returned in ${Date.now() - t0}ms`);
    harness = result.harness;
    untestable = result.untestable;
    raw = result.raw;
    cache.put(prop.smt2, info.source, { harness, untestable });
    console.log("───────────────────────────────────────────────────────");
    console.log("Raw response (truncated):");
    console.log(raw.slice(0, 1500));
  }

  console.log("═══════════════════════════════════════════════════════");
  if (untestable) {
    console.log(`VERDICT: untestable`);
    console.log(`reason:  ${untestable}`);
    return;
  }
  if (!harness) {
    console.log(`VERDICT: synthesis-failed — no harness and no UNTESTABLE line`);
    return;
  }

  console.log("Harness:");
  console.log(harness);
  console.log("═══════════════════════════════════════════════════════");

  const { fn, fnClass } = loadCallable(absPath, contract.function, info);
  if (!fn) {
    console.log("VERDICT: could not load function for execution");
    return;
  }

  const timeoutMs = parseInt(process.env.NEURALLOG_HARNESS_TIMEOUT_MS || "3000", 10);
  console.log(`[run] executing with timeout ${timeoutMs}ms…`);
  const t0 = Date.now();
  const outcome = await runHarness(harness, fn, fnClass, timeoutMs);
  console.log(`[run] returned in ${Date.now() - t0}ms`);

  console.log("═══════════════════════════════════════════════════════");
  console.log(`VERDICT:  ${outcome.kind}`);
  console.log(`message:  ${outcome.message}`);
}

function extractFunctionInfo(filePath: string, fnName: string): { paramNames: string[]; source: string; isStatic: boolean; className: string | null } | null {
  try {
    const source = readFileSync(filePath, "utf-8");
    const tree = parseFile(source);
    let target: Parser.SyntaxNode | null = null;
    const visit = (node: Parser.SyntaxNode): void => {
      if (target) return;
      if (
        node.type === "function_declaration" ||
        node.type === "method_definition" ||
        node.type === "arrow_function" ||
        node.type === "function_expression"
      ) {
        const nameNode = node.childForFieldName("name");
        const name = nameNode?.text;
        if (name === fnName) target = node;
        else if (
          node.parent?.type === "variable_declarator" &&
          node.parent.childForFieldName("name")?.text === fnName
        ) {
          target = node;
        }
      }
      for (const child of node.children) visit(child);
    };
    visit(tree.rootNode);
    if (!target) return null;

    const node: Parser.SyntaxNode = target;
    const paramsNode = node.childForFieldName("parameters");
    const names: string[] = [];
    if (paramsNode) {
      for (const child of paramsNode.namedChildren) {
        const patternNode = child.childForFieldName("pattern") || child.childForFieldName("name");
        if (patternNode?.type === "identifier") names.push(patternNode.text);
      }
    }

    const isStatic = node.children.some((c) => c.text === "static" && c.type === "static");
    let className: string | null = null;
    let cur: Parser.SyntaxNode | null = node.parent;
    while (cur) {
      if (cur.type === "class_declaration" || cur.type === "class") {
        const n = cur.childForFieldName("name");
        if (n) { className = n.text; break; }
      }
      cur = cur.parent;
    }

    return { paramNames: names, source: node.text, isStatic, className };
  } catch {
    return null;
  }
}

let tsNodeRegistered = false;
function ensureTsNodeTranspile(): void {
  if (tsNodeRegistered) return;
  try {
    require("ts-node").register({ transpileOnly: true, compilerOptions: { module: "commonjs" } });
    tsNodeRegistered = true;
  } catch {
    try {
      require("ts-node/register/transpile-only");
      tsNodeRegistered = true;
    } catch {}
  }
}

function loadCallable(filePath: string, fnName: string, info: { className: string | null; isStatic: boolean }): { fn: any; fnClass: any } {
  if (filePath.endsWith(".ts")) ensureTsNodeTranspile();
  let mod: any;
  try {
    delete require.cache[require.resolve(filePath)];
    mod = require(filePath);
  } catch (e: any) {
    console.log(`require failed for ${filePath}: ${e?.message?.slice(0, 120)}`);
    return { fn: null, fnClass: null };
  }

  const top = mod?.[fnName] || mod?.default?.[fnName];
  if (typeof top === "function") return { fn: top, fnClass: null };

  if (info.className) {
    const cls = mod?.[info.className] || mod?.default?.[info.className];
    if (typeof cls === "function") {
      if (info.isStatic) {
        const fn = cls[fnName];
        return { fn: typeof fn === "function" ? fn.bind(cls) : null, fnClass: cls };
      }
      if (typeof cls.prototype?.[fnName] === "function") {
        const ctorAttempts: any[][] = [[], [process.cwd()], [{}], [{ projectRoot: process.cwd() }]];
        for (const args of ctorAttempts) {
          try {
            const instance = new cls(...args);
            return { fn: (cls.prototype[fnName] as Function).bind(instance), fnClass: cls };
          } catch {}
        }
      }
    }
  }
  return { fn: null, fnClass: null };
}

main().catch((e) => { console.error("FATAL:", e); process.exit(1); });
