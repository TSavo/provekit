import { Transform, TransformCallback } from "stream";
import { execFile } from "child_process";
import { Contract, ContractStore, ProvenProperty } from "./contracts";

/**
 * A pino destination stream that evaluates cached neurallog contracts
 * against live log data at runtime. Normal log output always passes
 * through; proof entries are interleaved as additional JSON lines.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TransportOptions {
  /** Absolute or relative path to the project root (contains .neurallog/) */
  projectRoot: string;
  /** Where to write normal log output. Defaults to process.stdout. */
  destination?: NodeJS.WritableStream;
}

interface CallSiteKey {
  file: string;
  line: number;
}

interface ProofEntry {
  neurallog: true;
  ts: string;
  callSite: { file: string; line: number; function: string };
  property: string;
  result: "pass" | "fail" | "error";
  error?: string;
}

// ---------------------------------------------------------------------------
// Stack trace parsing
// ---------------------------------------------------------------------------

/**
 * Parse a V8 stack trace to find the first frame outside of pino internals
 * and this transport. Returns file + line of the application call site.
 *
 * We walk the stack looking for frames that are NOT inside:
 *   - node_modules/pino
 *   - neurallog/dist/transport
 *   - neurallog/src/transport
 *   - node:internal
 */
function parseCallSite(): CallSiteKey | null {
  const stack = new Error().stack;
  if (!stack) return null;

  const lines = stack.split("\n").slice(1); // drop "Error" line

  for (const line of lines) {
    // Skip pino internals, our transport, and node internals
    if (
      line.includes("node_modules/pino") ||
      line.includes("node_modules\\pino") ||
      line.includes("neurallog/dist/transport") ||
      line.includes("neurallog/src/transport") ||
      line.includes("neurallog\\dist\\transport") ||
      line.includes("neurallog\\src\\transport") ||
      line.includes("node:internal") ||
      line.includes("node:events")
    ) {
      continue;
    }

    // Match patterns like:
    //   at functionName (/path/to/file.ts:42:10)
    //   at /path/to/file.ts:42:10
    const match = line.match(/\(([^)]+):(\d+):\d+\)/) ||
                  line.match(/at\s+([^:]+):(\d+):\d+/);
    if (match) {
      return { file: match[1]!, line: parseInt(match[2]!, 10) };
    }
  }

  return null;
}

// ---------------------------------------------------------------------------
// Contract index — maps file:line to contracts for O(1) lookup
// ---------------------------------------------------------------------------

type ContractIndex = Map<string, Contract>;

function buildContractIndex(contracts: Contract[]): ContractIndex {
  const index: ContractIndex = new Map();
  for (const c of contracts) {
    // Key by file:line. The contract's `file` field may be absolute or
    // relative — we store both forms to maximise hit rate.
    index.set(`${c.file}:${c.line}`, c);
  }
  return index;
}

function lookupContract(
  index: ContractIndex,
  key: CallSiteKey,
  projectRoot: string
): Contract | undefined {
  // Try the raw file path first, then try stripping the project root to get
  // a relative path (contracts may be stored either way).
  const abs = `${key.file}:${key.line}`;
  if (index.has(abs)) return index.get(abs);

  const rel = key.file.startsWith(projectRoot)
    ? key.file.slice(projectRoot.length).replace(/^[/\\]/, "")
    : key.file;
  const relKey = `${rel}:${key.line}`;
  if (index.has(relKey)) return index.get(relKey);

  // Line numbers from stack traces can be off-by-one compared to the
  // contract's recorded line. Try +/- 1.
  for (const delta of [-1, 1]) {
    const k1 = `${key.file}:${key.line + delta}`;
    const k2 = `${rel}:${key.line + delta}`;
    if (index.has(k1)) return index.get(k1);
    if (index.has(k2)) return index.get(k2);
  }

  return undefined;
}

// ---------------------------------------------------------------------------
// Z3 evaluation (async, non-blocking)
// ---------------------------------------------------------------------------

/**
 * Substitute runtime variable values into an SMT-LIB template.
 *
 * Contracts contain SMT-LIB blocks with symbolic variable declarations like:
 *   (declare-const x Int)
 * We replace those with concrete (define-fun x () Int <value>) when the
 * variable name appears as a key in the log's structured data.
 */
function substituteSmt2(smt2: string, bindings: Record<string, unknown>): string {
  let result = smt2;

  for (const [name, value] of Object.entries(bindings)) {
    if (value === undefined || value === null) continue;

    // Determine the SMT sort from the JS type
    let smtValue: string;
    let sort: string;
    if (typeof value === "number") {
      sort = Number.isInteger(value) ? "Int" : "Real";
      smtValue = typeof value === "number" && value < 0 ? `(- ${Math.abs(value)})` : String(value);
    } else if (typeof value === "boolean") {
      sort = "Bool";
      smtValue = value ? "true" : "false";
    } else if (typeof value === "string") {
      sort = "String";
      smtValue = `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
    } else {
      continue; // skip complex types
    }

    // Replace (declare-const <name> <Sort>) with (define-fun <name> () <Sort> <value>)
    const declareRe = new RegExp(
      `\\(declare-const\\s+${escapeRegExp(name)}\\s+(Int|Real|Bool|String)\\)`,
      "g"
    );
    result = result.replace(declareRe, `(define-fun ${name} () ${sort} ${smtValue})`);
  }

  return result;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Run Z3 on an SMT-LIB string asynchronously.
 * Returns "sat", "unsat", "unknown", or "error".
 */
function runZ3Async(smt2: string): Promise<{ result: "sat" | "unsat" | "unknown" | "error"; error?: string }> {
  return new Promise((resolve) => {
    const child = execFile(
      "z3",
      ["-in", "-T:2"],
      { timeout: 5000 },
      (err, stdout, stderr) => {
        const out = (stdout || "").trim();
        if (out === "sat") return resolve({ result: "sat" });
        if (out === "unsat") return resolve({ result: "unsat" });
        if (out === "unknown") return resolve({ result: "unknown" });
        resolve({ result: "error", error: stderr || out || (err?.message ?? "unknown") });
      }
    );
    if (child.stdin) {
      child.stdin.write(smt2);
      child.stdin.end();
    }
  });
}

// ---------------------------------------------------------------------------
// Evaluate a contract's proven properties against live data
// ---------------------------------------------------------------------------

async function evaluateContract(
  contract: Contract,
  logData: Record<string, unknown>,
  projectRoot: string
): Promise<ProofEntry[]> {
  const entries: ProofEntry[] = [];
  const now = new Date().toISOString();

  const callSite = {
    file: contract.file,
    line: contract.line,
    function: contract.function,
  };

  // Evaluate proven properties — these should still hold at runtime.
  // If Z3 returns sat (the negation is satisfiable), the live data violates the invariant.
  for (const prop of contract.proven) {
    const grounded = substituteSmt2(prop.smt2, logData);
    if (grounded === prop.smt2) continue;

    const { result, error } = await runZ3Async(grounded);

    let proofResult: "pass" | "fail" | "error";
    if (result === "unsat") proofResult = "pass";
    else if (result === "sat") proofResult = "fail";
    else proofResult = "error";

    const entry: ProofEntry = {
      neurallog: true,
      ts: now,
      callSite,
      property: prop.claim,
      result: proofResult,
    };
    if (error) entry.error = error;
    entries.push(entry);
  }

  // Evaluate violations — these are bugs that were statically reachable.
  // At runtime, substitute concrete values and check: is this specific call
  // actually triggering the violation? If Z3 returns sat with the concrete
  // values, the bug is happening RIGHT NOW, not just theoretically reachable.
  for (const violation of contract.violations) {
    const grounded = substituteSmt2(violation.smt2, logData);
    if (grounded === violation.smt2) continue;

    const { result, error } = await runZ3Async(grounded);

    // For violations: sat means the bug is active with these values.
    // unsat means these specific values don't trigger it.
    let proofResult: "pass" | "fail" | "error";
    if (result === "unsat") proofResult = "pass";    // this call is safe
    else if (result === "sat") proofResult = "fail";  // bug is happening NOW
    else proofResult = "error";

    const entry: ProofEntry = {
      neurallog: true,
      ts: now,
      callSite,
      property: `VIOLATION CHECK: ${violation.claim}`,
      result: proofResult,
    };
    if (error) entry.error = error;
    entries.push(entry);
  }

  return entries;
}

// ---------------------------------------------------------------------------
// Transport stream
// ---------------------------------------------------------------------------

class NeurallogTransform extends Transform {
  private contractIndex: ContractIndex;
  private projectRoot: string;
  private destination: NodeJS.WritableStream;

  constructor(opts: TransportOptions) {
    super({ objectMode: false });
    this.projectRoot = require("path").resolve(opts.projectRoot);
    this.destination = opts.destination || process.stdout;

    // Load contracts from disk once at startup
    const store = new ContractStore(this.projectRoot);
    const contracts = store.getAll();
    this.contractIndex = buildContractIndex(contracts);
  }

  _transform(chunk: Buffer, _encoding: string, callback: TransformCallback): void {
    const line = chunk.toString();

    // Always pass the original log line through immediately (non-blocking)
    this.destination.write(line);

    // Try to parse as JSON (pino output)
    let logObj: Record<string, unknown>;
    try {
      logObj = JSON.parse(line);
    } catch {
      // Not JSON — plain text log. Pass through, nothing to evaluate.
      callback();
      return;
    }

    // Attempt to identify the call site.
    // In pino transport mode the stack trace won't point back to the
    // original logger.info() call. Instead we look for a `caller` or
    // `src` field that pino can add (when callers: true), or a neurallog-
    // specific field.
    const callSite = this.extractCallSite(logObj);

    if (!callSite) {
      callback();
      return;
    }

    const contract = lookupContract(this.contractIndex, callSite, this.projectRoot);
    if (!contract || contract.proven.length === 0) {
      callback();
      return;
    }

    // Evaluate asynchronously — do NOT block the stream
    evaluateContract(contract, logObj, this.projectRoot)
      .then((entries) => {
        for (const entry of entries) {
          this.destination.write(JSON.stringify(entry) + "\n");
        }
      })
      .catch(() => {
        // Swallow errors — neurallog must never break the app
      })
      .finally(() => {
        callback();
      });
  }

  /**
   * Extract call site info from the log object.
   *
   * Strategy (in priority order):
   * 1. Explicit neurallog metadata: { _nl: { file, line } }
   * 2. Pino caller info (requires callers: true): { caller: "file:line" }
   * 3. Pino src field: { src: { file, line } }
   */
  private extractCallSite(logObj: Record<string, unknown>): CallSiteKey | null {
    // 1. Explicit neurallog metadata
    const nl = logObj._nl as Record<string, unknown> | undefined;
    if (nl && typeof nl.file === "string" && typeof nl.line === "number") {
      return { file: nl.file, line: nl.line };
    }

    // 2. Pino caller string: "path/to/file.ts:42:10"
    if (typeof logObj.caller === "string") {
      const m = logObj.caller.match(/^(.+):(\d+):\d+$/);
      if (m) return { file: m[1]!, line: parseInt(m[2]!, 10) };
    }

    // 3. Pino src object
    const src = logObj.src as Record<string, unknown> | undefined;
    if (src && typeof src.file === "string" && typeof src.line === "number") {
      return { file: src.file, line: src.line };
    }

    return null;
  }

  _flush(callback: TransformCallback): void {
    callback();
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Create a neurallog pino destination stream.
 *
 * Usage:
 * ```ts
 * import pino from 'pino';
 * import { createNeurallogTransport } from 'neurallog';
 * const logger = pino({}, createNeurallogTransport({ projectRoot: '.' }));
 * ```
 *
 * For call-site identification to work, configure pino with caller info:
 * ```ts
 * const logger = pino(
 *   { transport: undefined },  // use destination mode, not transport mode
 *   createNeurallogTransport({ projectRoot: '.' })
 * );
 * ```
 *
 * Or inject call-site metadata manually:
 * ```ts
 * logger.info({ _nl: { file: __filename, line: __LINE__ }, userId: 42 }, 'login');
 * ```
 */
export function createNeurallogTransport(opts: TransportOptions): NeurallogTransform {
  return new NeurallogTransform(opts);
}

/**
 * Pino transport entry point — used when configured as:
 * ```ts
 * pino({ transport: { target: 'neurallog/dist/transport' } })
 * ```
 *
 * Pino transports receive newline-delimited JSON on stdin-like readable.
 * We use pino-abstract-transport to handle the plumbing.
 */
export default async function pinoTransport(opts: { projectRoot?: string } = {}) {
  const build = require("pino-abstract-transport");
  const { resolve } = require("path");

  const projectRoot = resolve(opts.projectRoot || ".");
  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();
  const contractIndex = buildContractIndex(contracts);

  return build(
    async function (source: AsyncIterable<Record<string, unknown>>) {
      for await (const logObj of source) {
        // Always emit the original log line
        const logLine = JSON.stringify(logObj) + "\n";
        process.stdout.write(logLine);

        // Extract call site
        const callSite = extractCallSiteFromObj(logObj);
        if (!callSite) continue;

        const contract = lookupContract(contractIndex, callSite, projectRoot);
        if (!contract || contract.proven.length === 0) continue;

        // Evaluate async — fire and forget to avoid blocking
        evaluateContract(contract, logObj as Record<string, unknown>, projectRoot)
          .then((entries) => {
            for (const entry of entries) {
              process.stdout.write(JSON.stringify(entry) + "\n");
            }
          })
          .catch(() => {
            // Swallow — neurallog must never break the app
          });
      }
    },
    { parse: "lines" }
  );
}

function extractCallSiteFromObj(logObj: Record<string, unknown>): CallSiteKey | null {
  const nl = logObj._nl as Record<string, unknown> | undefined;
  if (nl && typeof nl.file === "string" && typeof nl.line === "number") {
    return { file: nl.file, line: nl.line };
  }

  if (typeof logObj.caller === "string") {
    const m = logObj.caller.match(/^(.+):(\d+):\d+$/);
    if (m) return { file: m[1]!, line: parseInt(m[2]!, 10) };
  }

  const src = logObj.src as Record<string, unknown> | undefined;
  if (src && typeof src.file === "string" && typeof src.line === "number") {
    return { file: src.file, line: src.line };
  }

  return null;
}
