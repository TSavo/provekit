import { canonicalEncode } from "../../../../implementations/typescript/src/claimEnvelope/canonicalize.js";
import { computeCid } from "../../../../implementations/typescript/src/canonicalizer/hash.js";
import { liftPath } from "../../../../implementations/typescript/src/lift/index.js";

const contractName = "LookupRequest";

export const boundaryPredicate = "maybe_null(name)";
export const sinkPredicate = "non_null(name)";
export const missingEdge = "maybe_null(name) => non_null(name)";

export const nullBoundaryIr = [
  {
    kind: "contract",
    symbol: "lookup",
    precondition: {
      kind: "atomic",
      name: "neq",
      args: [
        { kind: "var", name: "name" },
        { kind: "const", value: null, sort: { kind: "primitive", name: "Ref" } },
      ],
    },
  },
];

function hasLookupNameStringBoundary(pre: unknown): boolean {
  const text = JSON.stringify(pre);
  return (
    text.includes('"kind-of"') &&
    text.includes('"field"') &&
    text.includes('"name"') &&
    text.includes('"String"')
  );
}

function cidOfJson(value: unknown): string {
  return computeCid(canonicalEncode(value));
}

export function discoverBoundary(surface: string, workspaceRoot: string): unknown {
  if (!surface) {
    throw new Error("TypeScript discovery surface is required");
  }

  const report = liftPath(workspaceRoot);
  const decl = report.decls.find(
    (candidate) => candidate.name === contractName && candidate.adapter === surface,
  );
  if (!decl) {
    throw new Error(`missing ${surface} ${contractName} contract in ${workspaceRoot}`);
  }
  if (!hasLookupNameStringBoundary(decl.pre)) {
    throw new Error(`${surface} ${contractName} did not lift a name:string boundary`);
  }

  return {
    kind: "bug-zoo-discovery",
    language: "typescript",
    toolchain: "pnpm exec tsx",
    surface,
    boundary: boundaryPredicate,
    sink: sinkPredicate,
    missingEdge,
    evidence: {
      adapter: decl.adapter,
      contract: decl.name,
      lifter: "liftPath",
      sourcePath: decl.sourcePath,
      irEvidenceCid: cidOfJson({
        adapter: decl.adapter,
        contract: decl.name,
        pre: decl.pre,
        sourcePath: decl.sourcePath,
      }),
    },
  };
}
