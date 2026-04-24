import { join, dirname, resolve, isAbsolute } from "path";
import { existsSync, readFileSync } from "fs";
import { fileURLToPath } from "url";
import { createHash } from "crypto";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

import { Phase, PhaseResult, PhaseOptions } from "./Phase.js";
import { DerivationOutput } from "./DerivationPhase.js";
import { Contract } from "../contracts.js";
import { openDb } from "../db/index.js";
import { gapReports, clauses } from "../db/schema/index.js";
import { detectGaps } from "../gapDetection.js";
import { parseZ3Model } from "../z3/modelParser.js";
import { synthesizeInputs } from "../inputs/synthesizer.js";
import type { Binding } from "../bindings/validator.js";

export interface GapDetectionInput {
  derivation: DerivationOutput;
  projectRoot: string;
  contracts: Contract[];
}

export interface GapDetectionOutput {
  reportsWritten: number;
  skipped: {
    missingBindings: number;
    missingWitness: number;
    untestable: number;
  };
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export class GapDetectionPhase extends Phase<GapDetectionInput, GapDetectionOutput> {
  readonly name = "Gap Detection";
  readonly phaseNumber = 4;

  async execute(
    input: GapDetectionInput,
    options: PhaseOptions,
  ): Promise<PhaseResult<GapDetectionOutput>> {
    const { projectRoot, contracts } = input;

    const dbPath = join(projectRoot, ".provekit", "provekit.db");
    const db = openDb(dbPath);

    const migrationsFolder = join(__dirname, "..", "..", "drizzle");
    migrate(db, { migrationsFolder });

    const output: GapDetectionOutput = {
      reportsWritten: 0,
      skipped: {
        missingBindings: 0,
        missingWitness: 0,
        untestable: 0,
      },
    };

    for (const contract of contracts) {
      for (const violation of contract.violations) {
        // Skip: no bindings
        if (!violation.smt_bindings || violation.smt_bindings.length === 0) {
          output.skipped.missingBindings++;
          continue;
        }

        // Skip: no witness text
        if (!violation.witness) {
          output.skipped.missingWitness++;
          continue;
        }

        // Resolve absolute path for the source file
        const absolutePath = isAbsolute(contract.file)
          ? contract.file
          : resolve(projectRoot, contract.file);

        if (!existsSync(absolutePath)) {
          output.skipped.untestable++;
          continue;
        }

        // Map SmtBinding (snake_case) → Binding (camelCase) for detectGaps
        const bindings: Binding[] = violation.smt_bindings.map((b) => ({
          smtConstant: b.smt_constant,
          sourceLine: b.source_line,
          sourceExpr: b.source_expr,
          sort: b.sort,
        }));

        // Parse Z3 witness
        let z3Model: Map<string, import("../z3/modelParser.js").Z3Value>;
        try {
          z3Model = parseZ3Model(violation.witness);
        } catch {
          output.skipped.untestable++;
          continue;
        }

        // Read function source and synthesize inputs
        let functionSource: string;
        let inputs: Record<string, unknown>;
        try {
          functionSource = readFileSync(absolutePath, "utf-8");
          inputs = synthesizeInputs({
            functionSource,
            functionName: contract.function,
            bindings: violation.smt_bindings,
            z3Model,
          });
        } catch {
          output.skipped.untestable++;
          continue;
        }

        // Insert a clauses row (detectGaps needs the FK)
        const clauseHash = createHash("sha256")
          .update(violation.smt2)
          .digest("hex")
          .slice(0, 16);

        let clauseId: number;
        try {
          const inserted = db
            .insert(clauses)
            .values({
              contractKey: contract.key,
              verdict: "violation",
              smt2: violation.smt2,
              clauseHash,
            })
            .returning({ id: clauses.id })
            .get();
          clauseId = inserted.id;
        } catch {
          output.skipped.untestable++;
          continue;
        }

        // Call detectGaps; wrap in try/catch — throws mean untestable
        try {
          await detectGaps({
            db,
            clauseId,
            sourcePath: absolutePath,
            functionName: contract.function,
            signalLine: contract.line,
            bindings,
            z3WitnessText: violation.witness,
            inputs,
          });
        } catch {
          output.skipped.untestable++;
          continue;
        }

        // Count gap_reports rows written for this clauseId
        const rows = db
          .select({ id: gapReports.id })
          .from(gapReports)
          .where(eq(gapReports.clauseId, clauseId))
          .all();
        output.reportsWritten += rows.length;
      }
    }

    if (options.verbose || output.reportsWritten > 0 || Object.values(output.skipped).some((n) => n > 0)) {
      const { missingBindings, missingWitness, untestable } = output.skipped;
      const totalSkipped = missingBindings + missingWitness + untestable;
      console.log(
        `GapDetection: ${output.reportsWritten} reports written, ${totalSkipped} skipped` +
          ` (${missingBindings} missing bindings, ${missingWitness} missing witness, ${untestable} untestable)`,
      );
    }

    return { data: output, writtenTo: dbPath };
  }
}
