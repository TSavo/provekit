import { relative } from "node:path";

import { liftPath } from "../../../../implementations/typescript/src/lift/index.js";

const workspaceRoot = process.argv[2];
if (!workspaceRoot) {
  process.stderr.write("usage: supply-chain-ts-contracts.ts <workspace-root>\n");
  process.exit(2);
}

function supplyChainName(decl: { name: string; targetContract?: string }): string {
  const target = decl.targetContract ?? "";
  const prefix = "supply-chain:";
  if (target.startsWith(prefix)) {
    return target.slice(prefix.length);
  }
  return decl.name;
}

const report = liftPath(workspaceRoot);
const contracts = report.decls.map((decl) => ({
  name: supplyChainName(decl),
  sourceName: decl.name,
  adapter: decl.adapter,
  sourcePath: relative(workspaceRoot, decl.sourcePath),
  outBinding: decl.outBinding,
  ...(decl.pre !== undefined ? { pre: decl.pre } : {}),
  ...(decl.post !== undefined ? { post: decl.post } : {}),
  ...(decl.inv !== undefined ? { inv: decl.inv } : {}),
  ...(decl.targetContract !== undefined ? { targetContract: decl.targetContract } : {}),
}));

process.stdout.write(
  JSON.stringify(
    {
      kind: "typescript-contract-report",
      lifter: "provekit-lift-ts",
      filesScanned: report.filesScanned,
      adapterReports: report.adapterReports,
      parseErrors: report.parseErrors,
      contracts,
    },
    null,
    2,
  ) + "\n",
);
