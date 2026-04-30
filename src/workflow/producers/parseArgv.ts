/**
 * parse-argv Stage — first node of the meta-dispatcher workflow.
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 *
 * Pure over `(argv, cliBlocks)`. The dispatcher cli.ts walks
 * `src/workflows/` once at startup, extracts every workflow's `cli:`
 * block, and threads the resulting map into `$input.cliBlocks`. This
 * Stage produces the parsed command name and per-arg values — no
 * filesystem reads, no global state, fully cache-friendly.
 *
 * Help is data, not a side effect. The Stage emits `helpRequested`
 * + `helpText` when argv is empty / `--help` / `-h` is seen, or when
 * `<command> --help` is seen. The dispatcher (cli.ts) prints + exits;
 * the Stage stays pure.
 *
 * Workflows whose name starts with `_` are dispatcher internals and
 * are filtered from the help table. They cannot be invoked as
 * `provekit _dispatch ...` (the parser rejects underscore-prefixed
 * commands explicitly).
 */

import type { Stage } from "../types.js";
import type { CliArg, CliBlock } from "../manifest.js";

export const PARSE_ARGV_CAPABILITY = "parse-argv";

export interface ParseArgvStageInput {
  /** Process argv WITHOUT node + script path — i.e. process.argv.slice(2). */
  argv: readonly string[];
  /**
   * Map keyed by command name → that workflow's parsed `cli:` block.
   * cli.ts walks src/workflows/ and threads this in. Workflows without
   * a cli: block are not included — they're not addressable as commands.
   */
  cliBlocks: Record<string, CliBlock>;
}

export type ParseArgvOutput =
  | {
      kind: "help";
      /** Pre-rendered text the dispatcher will print before exit(0). */
      helpText: string;
    }
  | {
      kind: "unknown";
      /** Command the user typed that has no matching cli: block. */
      command: string;
      /** Pre-rendered "unknown command" text + help table. */
      helpText: string;
    }
  | {
      kind: "command";
      /** The matched command name. */
      command: string;
      /**
       * Per-arg parsed values, keyed by arg name. Positional args land
       * by name; flags land by name; defaults are filled in. Type
       * coercion (path | string | int | bool) is applied here.
       */
      parsedArgs: Record<string, unknown>;
    };

export interface MakeParseArgvStageDeps {
  producerVersion?: string;
}

export function makeParseArgvStage(
  deps: MakeParseArgvStageDeps = {},
): Stage<ParseArgvStageInput, ParseArgvOutput> {
  const producedBy = deps.producerVersion ?? "parse-argv@v1";

  return {
    name: "parse-argv",
    producedBy,

    serializeInput(input) {
      return {
        argv: [...input.argv],
        cliBlocks: canonicalizeCliBlocks(input.cliBlocks),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as ParseArgvOutput;
    },

    async run(input) {
      return parseArgv(input.argv, input.cliBlocks);
    },
  };
}

// ---------------------------------------------------------------------------
// Pure implementation — exported for direct testing without Stage wrapping.
// ---------------------------------------------------------------------------

export function parseArgv(
  argv: readonly string[],
  cliBlocks: Record<string, CliBlock>,
): ParseArgvOutput {
  // No args → top-level help.
  if (argv.length === 0) {
    return { kind: "help", helpText: renderTopLevelHelp(cliBlocks) };
  }

  const first = argv[0]!;
  if (first === "--help" || first === "-h") {
    return { kind: "help", helpText: renderTopLevelHelp(cliBlocks) };
  }

  // Reject underscore-prefixed commands (dispatcher internals).
  if (first.startsWith("_")) {
    return {
      kind: "unknown",
      command: first,
      helpText:
        `provekit: "${first}" is an internal workflow and cannot be invoked directly.\n\n` +
        renderTopLevelHelp(cliBlocks),
    };
  }

  const block = cliBlocks[first];
  if (!block) {
    return {
      kind: "unknown",
      command: first,
      helpText:
        `provekit: unknown command "${first}".\n\n` + renderTopLevelHelp(cliBlocks),
    };
  }

  const rest = argv.slice(1);

  // Per-command help.
  if (rest.includes("--help") || rest.includes("-h")) {
    return { kind: "help", helpText: renderCommandHelp(first, block) };
  }

  const parsedArgs = parseArgsForBlock(block, rest);
  return { kind: "command", command: first, parsedArgs };
}

function parseArgsForBlock(
  block: CliBlock,
  tokens: readonly string[],
): Record<string, unknown> {
  const args = block.args ?? [];
  const positionals = args.filter((a) => a.positional);
  const flags = args.filter((a) => !a.positional);
  const flagByName = new Map<string, CliArg>();
  for (const f of flags) flagByName.set(f.name, f);

  const out: Record<string, unknown> = {};

  // Seed defaults.
  for (const a of args) {
    if (a.default !== undefined) out[a.name] = a.default;
    else if (a.flag) out[a.name] = false;
  }

  const positionalQueue = [...positionals];
  const remaining: string[] = [];

  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i]!;
    if (t.startsWith("--")) {
      const name = t.slice(2);
      const flag = flagByName.get(name);
      if (!flag) {
        throw new Error(`unknown flag --${name}`);
      }
      if (flag.flag) {
        out[name] = true;
      } else {
        const next = tokens[i + 1];
        if (next === undefined) {
          throw new Error(`flag --${name} requires a value`);
        }
        out[name] = coerce(next, flag.type);
        i++;
      }
    } else {
      remaining.push(t);
    }
  }

  for (const t of remaining) {
    const arg = positionalQueue.shift();
    if (!arg) {
      throw new Error(`unexpected positional argument "${t}"`);
    }
    out[arg.name] = coerce(t, arg.type);
  }

  // Required positionals not consumed → missing.
  for (const arg of positionalQueue) {
    if (arg.required && out[arg.name] === undefined) {
      throw new Error(`missing required positional argument "${arg.name}"`);
    }
  }
  for (const arg of flags) {
    if (arg.required && out[arg.name] === undefined) {
      throw new Error(`missing required flag --${arg.name}`);
    }
  }

  return out;
}

function coerce(raw: string, type: CliArg["type"] | undefined): unknown {
  if (!type || type === "string" || type === "path") return raw;
  if (type === "int") {
    const n = Number.parseInt(raw, 10);
    if (Number.isNaN(n)) throw new Error(`expected int, got "${raw}"`);
    return n;
  }
  if (type === "bool") {
    if (raw === "true") return true;
    if (raw === "false") return false;
    throw new Error(`expected bool, got "${raw}"`);
  }
  return raw;
}

// ---------------------------------------------------------------------------
// Help rendering
// ---------------------------------------------------------------------------

function renderTopLevelHelp(cliBlocks: Record<string, CliBlock>): string {
  const visibleNames = Object.keys(cliBlocks)
    .filter((n) => !n.startsWith("_"))
    .sort();
  const lines: string[] = [];
  lines.push("Usage: provekit <command> [args]");
  lines.push("");
  lines.push("Commands:");
  if (visibleNames.length === 0) {
    lines.push("  (none)");
  } else {
    const width = Math.max(...visibleNames.map((n) => n.length));
    for (const name of visibleNames) {
      const desc = cliBlocks[name]!.description;
      lines.push(`  ${name.padEnd(width)}  ${desc}`);
    }
  }
  lines.push("");
  lines.push("Run `provekit <command> --help` for command-specific usage.");
  return lines.join("\n");
}

function renderCommandHelp(name: string, block: CliBlock): string {
  const lines: string[] = [];
  const positionals = (block.args ?? []).filter((a) => a.positional);
  const flags = (block.args ?? []).filter((a) => !a.positional);
  const positionalUsage = positionals
    .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
    .join(" ");
  const flagUsage = flags.length > 0 ? " [flags]" : "";
  lines.push(
    `Usage: provekit ${name}${positionalUsage ? " " + positionalUsage : ""}${flagUsage}`,
  );
  lines.push("");
  lines.push(block.description);
  if (positionals.length > 0) {
    lines.push("");
    lines.push("Arguments:");
    const width = Math.max(...positionals.map((a) => a.name.length));
    for (const a of positionals) {
      const tag = a.required ? "required" : "optional";
      const ty = a.type ? ` (${a.type})` : "";
      lines.push(`  ${a.name.padEnd(width)}  ${tag}${ty}`);
    }
  }
  if (flags.length > 0) {
    lines.push("");
    lines.push("Flags:");
    const width = Math.max(...flags.map((a) => a.name.length + 2));
    for (const a of flags) {
      const label = `--${a.name}`.padEnd(width);
      const ty = a.flag ? "" : a.type ? ` (${a.type})` : "";
      const def =
        a.default !== undefined ? ` [default: ${JSON.stringify(a.default)}]` : "";
      lines.push(`  ${label}  ${a.flag ? "boolean" : "value"}${ty}${def}`);
    }
  }
  return lines.join("\n");
}

function canonicalizeCliBlocks(
  blocks: Record<string, CliBlock>,
): Record<string, CliBlock> {
  const out: Record<string, CliBlock> = {};
  for (const name of Object.keys(blocks).sort()) {
    out[name] = blocks[name]!;
  }
  return out;
}
