import { resolve } from "node:path";

import { liftPath } from "./index.js";
import type { ContractDecl, AdapterWarning } from "./types.js";

export interface VitestTestsIrDocument {
  kind: "ir-document";
  ir: Record<string, unknown>[];
  callEdges: [];
  diagnostics: Array<{ severity: "warning" | "error"; message: string }>;
}

export function liftVitestTestsIrDocument(
  workspaceRoot: string,
  sourcePaths: readonly string[],
): VitestTestsIrDocument {
  const paths = sourcePaths.length > 0 ? sourcePaths : ["."];
  const ir: Record<string, unknown>[] = [];
  const diagnostics: VitestTestsIrDocument["diagnostics"] = [];
  const seen = new Set<string>();

  for (const sourcePath of paths) {
    const report = liftPath(resolve(workspaceRoot, sourcePath));
    diagnostics.push(
      ...report.parseErrors.map((error) => ({
        severity: "error" as const,
        message: `${error.path}: ${error.message}`,
      })),
    );
    const vitestReport = report.adapterReports.find((entry) => entry.adapter === "vitest-tests");
    if (vitestReport) {
      diagnostics.push(...vitestReport.warnings.map(warningToDiagnostic));
    }
    for (const decl of report.decls.filter((entry) => entry.adapter === "vitest-tests")) {
      const item = contractDeclToIrEntry(decl);
      const key = String(item.name ?? "");
      if (key && seen.has(key)) continue;
      if (key) seen.add(key);
      ir.push(item);
    }
  }

  return { kind: "ir-document", ir, callEdges: [], diagnostics };
}

function contractDeclToIrEntry(decl: ContractDecl): Record<string, unknown> {
  const entry: Record<string, unknown> = {
    schemaVersion: "1",
    kind: "contract",
    name: decl.name,
    outBinding: decl.outBinding,
  };
  if (decl.pre !== undefined) entry.pre = decl.pre;
  if (decl.post !== undefined) entry.post = decl.post;
  if (decl.inv !== undefined) entry.inv = decl.inv;
  return entry;
}

function warningToDiagnostic(warning: AdapterWarning): { severity: "warning"; message: string } {
  return {
    severity: "warning",
    message: `${warning.sourcePath}:${warning.itemName}: ${warning.reason}`,
  };
}
