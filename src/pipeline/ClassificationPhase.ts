import { writeFileSync } from "fs";
import { join } from "path";
import { Phase, PhaseResult, PhaseOptions } from "./Phase";
import { DerivationOutput } from "./DerivationPhase";
import { PrincipleStore, classifyAndGeneralize } from "../principles";

export interface ClassificationOutput {
  discovered: number;
  validated: number;
  rejected: number;
  classifiedAt: string;
}

export interface ClassificationInput {
  derivation: DerivationOutput;
  model: string;
}

export class ClassificationPhase extends Phase<ClassificationInput, ClassificationOutput> {
  readonly name = "Principle Classification";
  readonly phaseNumber = 4;

  async execute(input: ClassificationInput, options: PhaseOptions): Promise<PhaseResult<ClassificationOutput>> {
    const newViolations = input.derivation.newViolations;

    if (newViolations.length === 0) {
      this.log("No [NEW] violations to classify.");
      console.log();
      const output: ClassificationOutput = { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() };
      const outPath = join(options.projectRoot, ".neurallog", "classification.json");
      writeFileSync(outPath, JSON.stringify(output, null, 2));
      return { data: output, writtenTo: outPath };
    }

    this.log(`Classifying ${newViolations.length} [NEW] violation${newViolations.length === 1 ? "" : "s"}...`);

    const principleStore = new PrincipleStore(options.projectRoot);
    const existingCount = principleStore.getAll().length;
    let validated = 0;
    let rejected = 0;

    for (const { violation, context } of newViolations) {
      process.stdout.write(`  ${context} ... `);

      const principle = await classifyAndGeneralize(
        violation, context, principleStore.getAll(), input.model
      );

      if (principle) {
        principle.id = principleStore.nextId();
        if (principle.validated) {
          principleStore.add(principle);
          validated++;
          console.log(`VALIDATED: ${principle.id} -- ${principle.name}`);
        } else {
          rejected++;
          console.log(`REJECTED: ${principle.id} -- ${principle.name}`);
          if (principle.validationFailure) {
            console.log(`    Reason: ${principle.validationFailure}`);
          }
        }
      } else {
        console.log("mapped to existing principle");
      }
    }

    const discovered = principleStore.getAll().length - existingCount;
    const output: ClassificationOutput = {
      discovered,
      validated,
      rejected,
      classifiedAt: new Date().toISOString(),
    };

    const outPath = join(options.projectRoot, ".neurallog", "classification.json");
    writeFileSync(outPath, JSON.stringify(output, null, 2));

    this.detail(`${discovered} new, ${validated} validated, ${rejected} rejected`);
    console.log();

    return { data: output, writtenTo: outPath };
  }
}
