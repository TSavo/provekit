import { SignalRegistry } from "../signals";
import { DependencyPhase, DependencyGraph } from "./DependencyPhase";
import { ContextPhase, ContextBundle } from "./ContextPhase";
import { DerivationPhase, DerivationOutput } from "./DerivationPhase";
import { ClassificationPhase, ClassificationOutput } from "./ClassificationPhase";
import { AxiomPhase, AxiomReport } from "./AxiomPhase";
import { PhaseOptions } from "./Phase";

export interface PipelineConfig {
  entryFilePath: string;
  projectRoot: string;
  model: string;
  verbose: boolean;
  changedFiles?: string[];
  signalRegistry?: SignalRegistry;
}

export interface PipelineResult {
  graph: DependencyGraph;
  bundles: ContextBundle[];
  derivation: DerivationOutput;
  classification: ClassificationOutput;
  report: AxiomReport;
}

export class Pipeline {
  private dependencyPhase = new DependencyPhase();
  private contextPhase = new ContextPhase();
  private derivationPhase = new DerivationPhase();
  private classificationPhase = new ClassificationPhase();
  private axiomPhase = new AxiomPhase();

  async runFull(config: PipelineConfig): Promise<PipelineResult> {
    const signalRegistry = config.signalRegistry || SignalRegistry.createDefault();
    const options: PhaseOptions = {
      projectRoot: config.projectRoot,
      verbose: config.verbose,
    };

    const { data: graph } = this.dependencyPhase.execute(
      { entryFilePath: config.entryFilePath, signalRegistry, changedFiles: config.changedFiles },
      options
    );

    const { data: bundles } = await this.contextPhase.execute(
      { graph, signalRegistry },
      options
    );

    if (bundles.length === 0) {
      console.log("No signals found in the dependency graph.");
      const emptyDerivation: DerivationOutput = { contracts: [], newViolations: [], derivedAt: new Date().toISOString() };
      const emptyClassification: ClassificationOutput = { discovered: 0, validated: 0, rejected: 0, classifiedAt: new Date().toISOString() };
      const { data: report } = this.axiomPhase.execute(undefined, options);
      return { graph, bundles, derivation: emptyDerivation, classification: emptyClassification, report };
    }

    const { data: derivation } = await this.derivationPhase.execute(
      { bundles, model: config.model },
      options
    );

    const { data: classification } = await this.classificationPhase.execute(
      { derivation, model: config.model },
      options
    );

    const { data: report } = this.axiomPhase.execute(undefined, options);

    return { graph, bundles, derivation, classification, report };
  }

  runVerifyOnly(projectRoot: string, verbose: boolean = false): AxiomReport {
    const options: PhaseOptions = { projectRoot, verbose };
    const { data: report } = this.axiomPhase.execute(undefined, options);
    return report;
  }
}
