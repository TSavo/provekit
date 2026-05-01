/**
 * provekit-lift vitest plugin.
 *
 * Drop-in Vite/Vitest plugin that runs the TS lift pipeline at test
 * startup. After this plugin lands in your `vitest.config.ts`, every
 * `pnpm test` (or `pnpm vitest run`) walks the source tree, lifts every
 * recognized zod schema / fast-check property / class-validator class,
 * mints a `.proof` catalog, and writes it to `node_modules/.cache/provekit/`
 * (or a configurable path).
 *
 * Sir T's design principle: invoking lift as `npx provekit-lift` is
 * wrong UX. Lift should run automatically as part of the standard TS
 * workflow. This plugin is the canonical adoption path; the standalone
 * CLI stays available for non-vitest users (CI scripts, language
 * tooling, etc.) but is no longer the recommended on-ramp.
 *
 * STRICT MODE
 * -----------
 * `strict: true` (or env PROVEKIT_LIFT_STRICT=1) causes the plugin to
 * THROW from buildStart if any of the following hold:
 *   - one or more files failed to parse,
 *   - one or more adapters emitted warnings (skipped contracts),
 *   - the workspace had zero liftable contracts.
 *
 * This is the "violation detected" knob promised by the spec: in the
 * lift toolchain there's no notion of a runtime contract failure;
 * instead, the structurally lift-relevant signals are parse errors and
 * adapter skip-warnings. Strict mode treats those as failures so CI
 * surfaces them; loose mode (the default) prints them to stderr.
 *
 * The plugin's report appears alongside vitest's normal output:
 *   ProvekIt: lifted N contracts (5 zod, 3 fast-check, 4 class-validator); minted .proof at <path>; <CID>.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  liftPath,
  mintProof,
  defaultLiftOptions,
  type LiftOptions,
  type LiftReport,
  type AdapterReport,
} from "./index.js";

export interface ProvekitLiftPluginOptions {
  /** Source root to walk. Default: `<cwd>/src`, falling back to `<cwd>`. */
  workspace?: string;
  /** Where to write `<cid>.proof`. Default: `node_modules/.cache/provekit`. */
  outDir?: string;
  /**
   * Adapters to dispatch. Default: all three. The current TS lift core
   * always runs all adapters; the option exists so callers can document
   * which adapter set their build expects, and so future per-adapter
   * disable wiring has a place to land.
   */
  adapters?: Array<"zod" | "fast-check" | "class-validator">;
  /**
   * Strict mode. When true (or env PROVEKIT_LIFT_STRICT is "1"/"true"),
   * the plugin THROWS from buildStart on parse errors, adapter skip
   * warnings, or zero-decls workspaces. Default false (warn only).
   */
  strict?: boolean;
  /**
   * Override mint options (signer seed, producedAt, etc.). Defaults
   * give cross-impl deterministic CIDs against fixtures, which is what
   * tests want.
   */
  liftOptions?: Partial<LiftOptions>;
  /**
   * Suppress all stdout/stderr output. Useful in test harnesses that
   * inspect the result programmatically.
   */
  silent?: boolean;
}

/**
 * Minimal Vite plugin shape. We don't import from "vite" because the
 * provekit core package is not vite-aware; vitest accepts plain objects
 * matching this shape via its `plugins` array.
 */
export interface ProvekitLiftPlugin {
  name: string;
  enforce: "pre";
  buildStart: () => void;
  /** Surfaced for tests: the most recent run's structured result. */
  __lastRun: ProvekitLiftRunResult | null;
}

export interface ProvekitLiftRunResult {
  report: LiftReport;
  cid: string;
  outPath: string;
  perAdapter: Record<string, { lifted: number; skipped: number }>;
  totalLifted: number;
  summary: string;
}

/**
 * Build the plugin. Exported as the default so callers can write:
 *
 *   import provekitLift from "@provekit/.../vitest-plugin";
 *   plugins: [provekitLift({ strict: false })]
 */
export default function provekitLiftPlugin(
  opts: ProvekitLiftPluginOptions = {},
): ProvekitLiftPlugin {
  const plugin: ProvekitLiftPlugin = {
    name: "provekit-lift",
    enforce: "pre",
    __lastRun: null,
    buildStart() {
      const result = runLiftOnce(opts);
      plugin.__lastRun = result;
    },
  };
  return plugin;
}

/**
 * Pure entry point. Exported separately so unit tests can drive the
 * plugin without instantiating vitest. Returns a structured result and
 * (in non-silent mode) prints the summary line to stdout. In strict
 * mode, throws on parse errors / adapter warnings / zero-decls.
 */
export function runLiftOnce(
  opts: ProvekitLiftPluginOptions = {},
): ProvekitLiftRunResult {
  const cwd = process.cwd();
  const workspace = resolve(opts.workspace ?? defaultWorkspace(cwd));
  const outDir = resolve(opts.outDir ?? join(cwd, "node_modules", ".cache", "provekit"));
  const strict = resolveStrict(opts.strict);
  const silent = opts.silent ?? false;

  const report = liftPath(workspace);

  // Strict mode: parse errors are fatal regardless of contract count.
  if (strict && report.parseErrors.length > 0) {
    throw new Error(
      `[provekit-lift] strict mode: ${report.parseErrors.length} parse error(s); first: ${report.parseErrors[0]!.path}: ${report.parseErrors[0]!.message}`,
    );
  }

  const perAdapter: Record<string, { lifted: number; skipped: number }> = {};
  for (const ar of report.adapterReports) {
    perAdapter[ar.adapter] = { lifted: ar.lifted, skipped: ar.warnings.length };
  }

  const totalSkipped = report.adapterReports.reduce(
    (n, ar) => n + ar.warnings.length,
    0,
  );
  if (strict && totalSkipped > 0) {
    const first = firstSkipReason(report.adapterReports);
    throw new Error(
      `[provekit-lift] strict mode: ${totalSkipped} contract(s) skipped with warnings; first: ${first}`,
    );
  }

  if (report.decls.length === 0) {
    if (strict) {
      throw new Error(
        `[provekit-lift] strict mode: no liftable contracts found at ${workspace}`,
      );
    }
    const summary = `ProvekIt: no liftable contracts found at ${workspace}.`;
    if (!silent) process.stderr.write(`${summary}\n`);
    return {
      report,
      cid: "",
      outPath: "",
      perAdapter,
      totalLifted: 0,
      summary,
    };
  }

  const liftOptions: LiftOptions = { ...defaultLiftOptions(), ...opts.liftOptions };
  const minted = mintProof(report.decls, liftOptions);
  mkdirSync(outDir, { recursive: true });
  const outPath = join(outDir, `${minted.cid}.proof`);
  writeFileSync(outPath, Buffer.from(minted.bytes));

  const adapterParts = report.adapterReports
    .map((a) => `${a.lifted} ${a.adapter}`)
    .join(", ");
  const summary = `ProvekIt: lifted ${minted.memberCount} contracts (${adapterParts}); minted .proof at ${outPath}; ${minted.cid}.`;
  if (!silent) {
    process.stdout.write(`${summary}\n`);
    if (totalSkipped > 0) {
      for (const ar of report.adapterReports) {
        for (const w of ar.warnings) {
          process.stderr.write(
            `  [provekit-lift] warn: ${w.adapter} skipped "${w.itemName}" in ${w.sourcePath}: ${w.reason}\n`,
          );
        }
      }
    }
  }

  return {
    report,
    cid: minted.cid,
    outPath,
    perAdapter,
    totalLifted: minted.memberCount,
    summary,
  };
}

function defaultWorkspace(cwd: string): string {
  // Prefer <cwd>/src if it exists; otherwise the cwd itself. We don't
  // import fs.statSync at the top because the file is also used in
  // tests that mock or override the workspace explicitly.
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { statSync } = require("node:fs") as typeof import("node:fs");
    const candidate = join(cwd, "src");
    if (statSync(candidate).isDirectory()) return candidate;
  } catch {
    // fall through
  }
  return cwd;
}

function resolveStrict(optStrict: boolean | undefined): boolean {
  if (optStrict !== undefined) return optStrict;
  const env = process.env.PROVEKIT_LIFT_STRICT;
  return env === "1" || env === "true";
}

function firstSkipReason(reports: AdapterReport[]): string {
  for (const r of reports) {
    for (const w of r.warnings) {
      return `${w.adapter} skipped "${w.itemName}" in ${w.sourcePath}: ${w.reason}`;
    }
  }
  return "(no detail)";
}
