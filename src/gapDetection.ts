import type { Db } from "./db/index.js";
import { gapReports, traceValues, runtimeValues, clauseBindings } from "./db/schema/index.js";
import { parseZ3Model } from "./z3/modelParser.js";
import { persistWitness } from "./z3/persistWitness.js";
import { runHarnessWithTrace } from "./harness.js";
import { validateBindings, type Binding } from "./bindings/validator.js";
import { ieeeSpecialsAgent } from "./comparator/agents/ieeeSpecials.js";
import { outcomeMismatchAgent } from "./comparator/agents/outcomeMismatch.js";
import { pathNotTakenAgent } from "./comparator/agents/pathNotTaken.js";
import { serializeValue } from "./runtime/valueSerializer.js";
import type { Z3Value } from "./z3/modelParser.js";
import { eq } from "drizzle-orm";
import { readFileSync } from "fs";

export interface DetectGapsArgs {
  db: Db;
  clauseId: number;
  sourcePath: string;
  functionName: string;
  signalLine: number;
  bindings: Binding[];
  z3WitnessText: string;
  inputs: Record<string, unknown>;
}

export async function detectGaps(args: DetectGapsArgs): Promise<void> {
  const { db, clauseId, sourcePath, functionName, signalLine, bindings, z3WitnessText, inputs } = args;

  // 1. Validate bindings against source text
  const source = readFileSync(sourcePath, "utf-8");
  const { valid, invalid } = validateBindings(source, bindings);
  for (const bad of invalid) {
    db.insert(gapReports).values({
      clauseId,
      kind: "invalid_binding",
      smtConstant: bad.binding.smtConstant,
      explanation: bad.reason,
    }).run();
  }
  if (valid.length === 0) return;

  // 2. Insert clause_bindings rows for valid bindings (FK prerequisite for clause_witnesses)
  for (const b of valid) {
    db.insert(clauseBindings).values({
      clauseId,
      smtConstant: b.smtConstant,
      sourceLine: b.sourceLine,
      sourceExpr: b.sourceExpr,
      sort: b.sort,
    }).run();
  }

  // 3. Parse Z3 model + persist witnesses (requires bindings to exist for composite FK)
  const parsedModel = parseZ3Model(z3WitnessText);
  persistWitness(db, clauseId, parsedModel);

  // 4. Run harness with trace
  const captureNames = valid.map((b) => b.smtConstant);
  const runResult = await runHarnessWithTrace({
    db,
    clauseId,
    sourcePath,
    functionName,
    signalLine,
    captureNames,
    inputs,
  });

  // 5. Pull runtime values per binding from traceValues + runtime_values
  const tvRows = db
    .select({
      nodeId: traceValues.nodeId,
      rootValueId: traceValues.rootValueId,
      kind: runtimeValues.kind,
      numberValue: runtimeValues.numberValue,
      stringValue: runtimeValues.stringValue,
      boolValue: runtimeValues.boolValue,
    })
    .from(traceValues)
    .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
    .where(eq(traceValues.traceId, runResult.traceId))
    .all();

  // nodeId format from runHarnessWithTrace: "<sourcePath>:<line>:<name>"
  const runtimeByConstant = new Map<string, typeof tvRows[0]>();
  for (const row of tvRows) {
    const name = row.nodeId.split(":").pop();
    if (name) runtimeByConstant.set(name, row);
  }

  // 6. Compute visited lines (for path-not-taken)
  const visitedLines = new Set<number>();
  for (const row of tvRows) {
    const parts = row.nodeId.split(":");
    // parts: [sourcePath (may contain colons on win?), line, name]; take second-to-last
    const lineStr = parts[parts.length - 2];
    const line = parseInt(lineStr || "0", 10);
    if (Number.isFinite(line) && line > 0) visitedLines.add(line);
  }

  // 7. Run sort-specific agents per binding
  for (const b of valid) {
    const witness = parsedModel.get(b.smtConstant);
    if (!witness) continue;
    const runtimeRow = runtimeByConstant.get(b.smtConstant);
    if (!runtimeRow) continue;

    const runtimeValueLite = {
      kind: runtimeRow.kind,
      numberValue: runtimeRow.numberValue,
      stringValue: runtimeRow.stringValue,
      boolValue: runtimeRow.boolValue,
    };

    const ieeeGap = ieeeSpecialsAgent({ binding: b, witness, runtimeValue: runtimeValueLite });
    if (ieeeGap) {
      const smtValueId = serializeWitnessForGap(db, witness);
      db.insert(gapReports).values({
        clauseId,
        traceId: runResult.traceId,
        kind: "ieee_specials",
        smtConstant: b.smtConstant,
        atNodeRef: `${sourcePath}:${b.sourceLine}`,
        smtValueId: smtValueId ?? undefined,
        runtimeValueId: runtimeRow.rootValueId,
        explanation: ieeeGap.explanation,
      }).run();
    }
  }

  // 8. Outcome mismatch. Phase A-thin assumes SMT models a returned outcome.
  const smtOutcome = { kind: "returned" as const };
  const rtOutcome =
    runResult.outcomeKind === "returned"
      ? { kind: "returned" as const }
      : runResult.outcomeKind === "threw"
        ? { kind: "threw" as const, error: runResult.error }
        : { kind: "untestable" as const };
  const outcomeGap = outcomeMismatchAgent({
    smtOutcome,
    runtimeOutcome: rtOutcome,
    smtConstant: valid[0]?.smtConstant ?? "<signal>",
  });
  if (outcomeGap) {
    db.insert(gapReports).values({
      clauseId,
      traceId: runResult.traceId,
      kind: "outcome_mismatch",
      smtConstant: outcomeGap.smtConstant,
      explanation: outcomeGap.explanation,
    }).run();
  }

  // 9. Path not taken
  const pathGap = pathNotTakenAgent({
    signalLine,
    visitedLines,
    smtConstant: valid[0]?.smtConstant ?? "<signal>",
  });
  if (pathGap) {
    db.insert(gapReports).values({
      clauseId,
      traceId: runResult.traceId,
      kind: "path_not_taken",
      smtConstant: pathGap.smtConstant,
      explanation: pathGap.explanation,
    }).run();
  }
}

function serializeWitnessForGap(db: Db, witness: Z3Value): number | null {
  if (witness.sort === "Real") {
    if (typeof witness.value === "number") return serializeValue(db, witness.value);
    if (witness.value === "nan" || witness.value === "div_by_zero") return serializeValue(db, NaN);
    if (witness.value === "+infinity") return serializeValue(db, Infinity);
    if (witness.value === "-infinity") return serializeValue(db, -Infinity);
  }
  if (witness.sort === "Int") return serializeValue(db, Number(witness.value));
  if (witness.sort === "Bool") return serializeValue(db, witness.value);
  if (witness.sort === "String") return serializeValue(db, witness.value);
  return null;
}
