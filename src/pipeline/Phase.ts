export interface PhaseResult<T> {
  data: T;
  writtenTo: string;
}

export interface PhaseOptions {
  projectRoot: string;
  verbose: boolean;
}

export abstract class Phase<TInput, TOutput> {
  abstract readonly name: string;
  abstract readonly phaseNumber: number;

  abstract execute(input: TInput, options: PhaseOptions): Promise<PhaseResult<TOutput>> | PhaseResult<TOutput>;

  protected log(message: string): void {
    if (this.phaseNumber < 1 || this.phaseNumber > 10) {
      throw new Error(`Invalid phaseNumber: ${this.phaseNumber}`);
    }
    console.log(`Phase ${this.phaseNumber}: ${message || this.name}`);
  }

  protected detail(message: string): void {
    if (!message) return;
    console.log(`  ${message}`);
  }
}
