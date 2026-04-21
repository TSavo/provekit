/**
 * Runtime observation API for capturing real values at signal points.
 *
 * Users wrap important call sites with `observe(signalKey, values)`. The
 * values are appended as NDJSON to `.neurallog/witnesses/<slug>.ndjson`.
 * PropertyTestChecker can later read these and use them as alternative
 * inputs alongside (or instead of) Z3-generated models — a Daikon-style
 * runtime-seeded verification input source.
 *
 * Usage:
 *   import { observe } from "neurallog/runtime";
 *
 *   function transfer(from, to, amount) {
 *     observe("transfer", { from, to, amount });
 *     // ... real implementation
 *   }
 *
 * Observations are only written when NEURALLOG_OBSERVE=1 is set in the
 * environment. This keeps the API zero-cost when running in production
 * without observability enabled.
 */

import { appendFileSync, mkdirSync, existsSync, readFileSync, readdirSync } from "fs";
import { join, dirname } from "path";

const MAX_LINE_BYTES = 16 * 1024;
const MAX_FILE_BYTES = 10 * 1024 * 1024;

function slugify(key: string): string {
  return key.replace(/[^A-Za-z0-9._:-]/g, "_").slice(0, 200);
}

function witnessPath(root: string, signalKey: string): string {
  return join(root, ".neurallog", "witnesses", slugify(signalKey) + ".ndjson");
}

function sanitizeForSerialization(v: any, depth: number = 0): any {
  if (depth > 4) return "<max-depth>";
  if (v === null || v === undefined) return v;
  const t = typeof v;
  if (t === "number") {
    if (!Number.isFinite(v)) return { __nonfinite__: String(v) };
    return v;
  }
  if (t === "string" || t === "boolean") return v;
  if (t === "bigint") return { __bigint__: String(v) };
  if (t === "function") return { __function__: v.name || "<anonymous>" };
  if (t === "symbol") return { __symbol__: String(v) };

  if (Array.isArray(v)) {
    return v.slice(0, 50).map((x) => sanitizeForSerialization(x, depth + 1));
  }
  if (v instanceof Map) {
    const entries: any[] = [];
    let i = 0;
    for (const [k, val] of v.entries()) {
      if (i++ >= 20) break;
      entries.push([sanitizeForSerialization(k, depth + 1), sanitizeForSerialization(val, depth + 1)]);
    }
    return { __map__: entries };
  }
  if (v instanceof Set) {
    const items: any[] = [];
    let i = 0;
    for (const x of v.values()) {
      if (i++ >= 20) break;
      items.push(sanitizeForSerialization(x, depth + 1));
    }
    return { __set__: items };
  }
  if (v instanceof Error) {
    return { __error__: { name: v.name, message: v.message } };
  }
  if (t === "object") {
    const out: Record<string, any> = {};
    let i = 0;
    for (const k of Object.keys(v)) {
      if (i++ >= 30) break;
      try {
        out[k] = sanitizeForSerialization(v[k], depth + 1);
      } catch {
        out[k] = "<serialize-error>";
      }
    }
    return out;
  }
  return String(v);
}

/**
 * Record an observation at a signal point. Signal key identifies the
 * point (usually function name or function:line). Values object holds
 * the runtime state snapshot worth capturing. Opt-in via
 * NEURALLOG_OBSERVE=1.
 *
 * Safe to call hot paths — if the env gate is off, this is a no-op.
 * Values are sanitized before write (deep-truncated, nonfinite numbers
 * tagged, references flattened) so the function cannot crash a logged
 * call site because of an unserializable payload.
 */
export function observe(signalKey: string, values: Record<string, any>, projectRoot: string = process.cwd()): void {
  if (process.env.NEURALLOG_OBSERVE !== "1") return;

  try {
    const path = witnessPath(projectRoot, signalKey);
    mkdirSync(dirname(path), { recursive: true });

    const stat = (() => { try { return require("fs").statSync(path); } catch { return null; } })();
    if (stat && stat.size > MAX_FILE_BYTES) return;

    const sanitized = sanitizeForSerialization(values);
    let line = JSON.stringify({ ts: Date.now(), values: sanitized });
    if (line.length > MAX_LINE_BYTES) {
      line = JSON.stringify({ ts: Date.now(), values: { __truncated__: line.length } });
    }
    appendFileSync(path, line + "\n", "utf-8");
  } catch {
    // observe is best-effort; never fail the caller
  }
}

export interface Observation {
  ts: number;
  values: Record<string, any>;
}

/**
 * Read observations for a signal key. Returns empty array if no
 * witnesses exist. Used by PropertyTestChecker (and anyone else) to
 * get actual runtime values as alternatives to Z3-synthesized ones.
 */
export function readObservations(signalKey: string, projectRoot: string = process.cwd()): Observation[] {
  const path = witnessPath(projectRoot, signalKey);
  if (!existsSync(path)) return [];
  try {
    return readFileSync(path, "utf-8")
      .split("\n")
      .filter((l) => l.length > 0)
      .map((l) => {
        try { return JSON.parse(l); } catch { return null; }
      })
      .filter((o): o is Observation => o !== null && typeof o.ts === "number" && typeof o.values === "object");
  } catch {
    return [];
  }
}

/**
 * List all signal keys for which observations exist. Useful for a
 * coverage report: which signals have been exercised at runtime vs
 * only observed via static analysis.
 */
export function listObservedSignals(projectRoot: string = process.cwd()): string[] {
  const dir = join(projectRoot, ".neurallog", "witnesses");
  if (!existsSync(dir)) return [];
  try {
    return readdirSync(dir)
      .filter((f) => f.endsWith(".ndjson"))
      .map((f) => f.slice(0, -".ndjson".length));
  } catch {
    return [];
  }
}
