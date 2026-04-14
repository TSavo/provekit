import { query } from "@anthropic-ai/claude-agent-sdk";
import { LogCallSite } from "./parser";
import { ContractStore } from "./contracts";
import { PrincipleStore } from "./principles";
import { readFileSync } from "fs";
import { join } from "path";
import Handlebars from "handlebars";

const compiledTemplate = loadAndCompileTemplate();

function loadAndCompileTemplate(): HandlebarsTemplateDelegate {
  const candidates = [
    join(__dirname, "..", "prompts", "invariant_derivation.md"),
    join(process.cwd(), "prompts", "invariant_derivation.md"),
  ];

  for (const path of candidates) {
    try {
      const raw = readFileSync(path, "utf-8");
      const promptStart = raw.indexOf("## Prompt\n");
      const template =
        promptStart !== -1
          ? raw.slice(promptStart + "## Prompt\n\n".length)
          : raw;
      return Handlebars.compile(template, { noEscape: true });
    } catch {
      continue;
    }
  }

  throw new Error(
    "Could not find prompts/invariant_derivation.md. Run from project root."
  );
}

export interface DerivationResult {
  callSite: LogCallSite;
  filePath: string;
  rawResponse: string;
}

export async function deriveContract(
  callSite: LogCallSite,
  fileSource: string,
  filePath: string,
  model: string = "sonnet",
  contractStore?: ContractStore,
  principleStore?: PrincipleStore,
  verbose: boolean = false
): Promise<DerivationResult> {
  const prompt = buildPrompt(callSite, fileSource, filePath, contractStore, principleStore);

  let rawResponse = "";

  for await (const message of query({
    prompt,
    options: {
      maxTurns: 1,
      model,
      systemPrompt:
        `You are a formal verification engine. You produce SMT-LIB 2 formulas. Be precise and concise. Every SMT-LIB block MUST use \`\`\`smt2 fences and include (check-sat). Tag every block with ; PRINCIPLE: P1-P${7 + (principleStore?.getAll().length ?? 0)} or [NEW].`,
    },
  })) {
    if (verbose) {
      if (message.type === "assistant") {
        const content = (message as any).message?.content;
        if (Array.isArray(content)) {
          for (const block of content) {
            if (block.type === "text" && block.text?.trim()) {
              const firstLine = block.text.trim().split("\n")[0]!;
              process.stderr.write(`    ${firstLine.slice(0, 120)}\n`);
            }
            if (block.type === "tool_use") {
              process.stderr.write(`    [tool] ${block.name}(${JSON.stringify(block.input).slice(0, 80)})\n`);
            }
          }
        }
      }
    }

    if (message.type === "assistant") {
      const content = (message as any).message?.content;
      if (Array.isArray(content)) {
        rawResponse += content
          .filter((block: any) => block.type === "text")
          .map((block: any) => block.text)
          .join("");
      }
    }
    if (message.type === "result" && message.subtype === "success") {
      rawResponse = message.result;
    }
  }

  return {
    callSite,
    filePath,
    rawResponse,
  };
}

function buildPrompt(
  callSite: LogCallSite,
  fileSource: string,
  filePath: string,
  contractStore?: ContractStore,
  principleStore?: PrincipleStore
): string {
  const isExported = callSite.functionSource.includes("export ");
  const visibility = isExported ? "public (exported)" : "module-private";

  const existingContracts = contractStore
    ? contractStore.formatForPrompt()
    : "(no existing contracts yet — first pass)";

  const discoveredPrinciples = principleStore
    ? principleStore.formatForPrompt()
    : "";

  // Inject discovered principles into the prompt after the seed principles
  let prompt = compiledTemplate({
    TARGET_FILE: filePath,
    TARGET_FUNCTION: callSite.functionName,
    TARGET_LINE: String(callSite.line),
    TARGET_STATEMENT: callSite.logText,
    TARGET_FILE_SOURCE: fileSource,
    IMPORT_SOURCES: "(single-file analysis — no imports resolved yet)",
    EXISTING_CONTRACTS: existingContracts,
    CALLING_CONTEXT: `${callSite.functionName} is ${visibility}. ${
      isExported
        ? "Any caller can pass any arguments."
        : "Only called within this module."
    }`,
  });

  // Append discovered principles after the seed principles section
  if (discoveredPrinciples) {
    const insertPoint = prompt.indexOf("### SMT-LIB 2 Grammar");
    if (insertPoint !== -1) {
      prompt =
        prompt.slice(0, insertPoint) +
        discoveredPrinciples +
        "\n\n" +
        prompt.slice(insertPoint);
    }
  }

  return prompt;
}
