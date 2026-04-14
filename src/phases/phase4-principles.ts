/**
 * Phase 4: Principle Classification
 *
 * Input:  .neurallog/derivation.json (from Phase 3)
 * Output: .neurallog/principles/*.json (immutable, append-only)
 *
 * Classifies [NEW] violations. Two-stage semantic diff, adversarial
 * validation with different model. Only validated principles are committed.
 */

import { writeFileSync, mkdirSync, readFileSync } from "fs";
import { join } from "path";
import { DerivationOutput } from "./phase3-derivation";
import { PrincipleStore, classifyAndGeneralize } from "../principles";

export interface PrincipleOutput {
  discovered: number;
  validated: number;
  rejected: number;
  classifiedAt: string;
}

export async function classifyPrinciples(
  derivation: DerivationOutput,
  projectRoot: string,
  model: string
): Promise<PrincipleOutput> {
  const newViolations = derivation.newViolations;

  if (newViolations.length === 0) {
    console.log("Phase 4: No [NEW] violations to classify.");
    console.log();
    return { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() };
  }

  console.log(`Phase 4: Classifying ${newViolations.length} [NEW] violation${newViolations.length === 1 ? "" : "s"}...`);

  const principleStore = new PrincipleStore(projectRoot);
  const existingCount = principleStore.getAll().length;
  let validated = 0;
  let rejected = 0;

  for (const { violation, context } of newViolations) {
    process.stdout.write(`  ${context} ... `);

    const principle = await classifyAndGeneralize(
      violation, context, principleStore.getAll(), model
    );

    if (principle) {
      principle.id = principleStore.nextId();
      if (principle.validated) {
        principleStore.add(principle);
        validated++;
        console.log(`VALIDATED: ${principle.id} — ${principle.name}`);
      } else {
        rejected++;
        console.log(`REJECTED: ${principle.id} — ${principle.name}`);
        if (principle.validationFailure) {
          console.log(`    Reason: ${principle.validationFailure}`);
        }
      }
    } else {
      console.log("mapped to existing principle");
    }
  }

  const discovered = principleStore.getAll().length - existingCount;

  const output: PrincipleOutput = {
    discovered,
    validated,
    rejected,
    classifiedAt: new Date().toISOString(),
  };

  writeFileSync(
    join(projectRoot, ".neurallog", "classification.json"),
    JSON.stringify(output, null, 2)
  );

  console.log(`  ${discovered} new, ${validated} validated, ${rejected} rejected`);
  console.log();

  return output;
}
