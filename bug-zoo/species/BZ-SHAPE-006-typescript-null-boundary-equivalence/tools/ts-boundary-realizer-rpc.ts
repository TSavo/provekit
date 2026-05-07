import readline from "node:readline";

import { computeCid } from "../../../../implementations/typescript/src/canonicalizer/hash.js";

const nullBoundaryIr = [
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

const postLiftCid =
  "blake3-512:39b71418bfeb1c92ac9de53b818339bffe8a9b0be843832b59c7cc7e31411b72842dc76d7b9620c9c06dbdc0981cff5aa771a9c8d9c846d12dc704987eb858a6";
const closureWitnessCid =
  "blake3-512:ba2b46e885eb6812b6e2ccbbc2489c2f41fe92d031e2f150b763c26a01658207dc40815db4c17d5cb7f08b838d42dc547d752cfa1ab056dd6c190b9c98ab61ce";

process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

function cidOfBytes(text: string): string {
  return computeCid(Buffer.from(text, "utf8"));
}

function realizeSource(source: string): string {
  const expected = `export function lookup(name: string): string {
  return "user:" + name.toUpperCase();
}
`;
  if (source !== expected) {
    throw new Error("unsupported source shape for TypeScript null-boundary realizer");
  }
  return `export function lookup(name: string | null | undefined): string {
  if (name == null) {
    throw new TypeError("name must be non-null");
  }
  return "user:" + name.toUpperCase();
}
`;
}

function closureWitnessBody(
  gapCid: string,
  policyCid: string,
  postLiftCid: string,
  sourcePredicate: string,
  targetPredicate: string,
  transformedArtifactCid: string,
): Record<string, unknown> {
  return {
    kind: "TruthDischargeBodyClaim",
    claimKind: "closure",
    gapCid,
    policyCid,
    postLiftCid,
    sourcePredicate,
    targetPredicate,
    transformedArtifactCid,
  };
}

function realize(plan: Record<string, unknown>): Record<string, unknown> {
  const source = plan.source;
  if (typeof source !== "string") {
    throw new Error("realizer plan source must be a string");
  }
  const missingEdge = "maybe_null(name) => non_null(name)";
  const sourcePredicate = "maybe_null(name)";
  const targetPredicate = "non_null(name)";
  const gapCid = String(plan.gapCid);
  const policyCid = String(plan.policyCid);
  const modifiedSource = realizeSource(source);
  const transformedArtifactCid = cidOfBytes(modifiedSource);
  const postLift = {
    kind: "ir-document",
    ir: nullBoundaryIr,
    source: {
      adapter: "typescript-native-dropper",
      contract: "lookup",
      sourcePath: "dropped/typescript-native/library/src/UserDirectory.ts",
    },
  };
  const closureWitness = closureWitnessBody(
    gapCid,
    policyCid,
    postLiftCid,
    sourcePredicate,
    targetPredicate,
    transformedArtifactCid,
  );

  if (
    plan.sourcePredicate !== sourcePredicate ||
    plan.targetPredicate !== targetPredicate ||
    plan.targetSymbol !== "lookup" ||
    plan.proofVar !== "name" ||
    plan.surface !== "typescript-native"
  ) {
    throw new Error(`unsupported TypeScript realizer plan for ${missingEdge}`);
  }

  return {
    status: "closed",
    modifiedSource,
    gapCid,
    transformedArtifactCid,
    postLiftCid,
    postLift,
    closureWitness,
    closureWitnessCid,
  };
}

const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

rl.on("line", (line: string) => {
  let id: unknown = null;
  try {
    const request = JSON.parse(line);
    id = request.id;
    if (request.method === "initialize") {
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: {
            name: "bug-zoo-typescript-null-boundary-realizer",
            version: "0",
            capabilities: ["typescript-null-boundary"],
          },
        }) + "\n",
      );
      return;
    }
    if (request.method === "realize") {
      const plan = request.params?.plan;
      if (!plan || typeof plan !== "object") {
        throw new Error("realize params.plan must be an object");
      }
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: { output: realize(plan) },
        }) + "\n",
      );
      return;
    }
    if (request.method === "shutdown") {
      rl.close();
      return;
    }
    throw new Error(`unsupported method ${request.method}`);
  } catch (error) {
    process.stdout.write(
      JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: { code: -32000, message: error instanceof Error ? error.message : String(error) },
      }) + "\n",
    );
  }
});
