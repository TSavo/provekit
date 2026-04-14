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
    console.log(`Phase ${this.phaseNumber}: ${message}`);
  }

  protected detail(message: string): void {
    console.log(`  ${message}`);
  }
}
