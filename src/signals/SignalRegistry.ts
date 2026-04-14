import Parser from "tree-sitter";
import { Signal, SignalGenerator } from "./Signal";
import { LogSignalGenerator } from "./LogSignalGenerator";
import { CommentSignalGenerator } from "./CommentSignalGenerator";
import { FunctionNameSignalGenerator } from "./FunctionNameSignalGenerator";

export class SignalRegistry {
  private generators: SignalGenerator[] = [];

  register(generator: SignalGenerator): void {
    this.generators.push(generator);
  }

  findAll(filePath: string, source: string, tree: Parser.Tree): Signal[] {
    const signals: Signal[] = [];
    for (const gen of this.generators) {
      if (gen.async) continue;
      const result = gen.findSignals(filePath, source, tree);
      if (Array.isArray(result)) signals.push(...result);
    }
    signals.sort((a, b) => a.line - b.line);
    return signals;
  }

  async findAllAsync(filePath: string, source: string, tree: Parser.Tree): Promise<Signal[]> {
    const signals: Signal[] = [];
    for (const gen of this.generators) {
      const result = gen.findSignals(filePath, source, tree);
      if (result instanceof Promise) {
        signals.push(...await result);
      } else {
        signals.push(...result);
      }
    }
    signals.sort((a, b) => a.line - b.line);
    return signals;
  }

  hasAsyncGenerators(): boolean {
    return this.generators.some((g) => g.async);
  }

  getGeneratorNames(): string[] {
    return this.generators.map((g) => g.name);
  }

  static createDefault(): SignalRegistry {
    const registry = new SignalRegistry();
    registry.register(new LogSignalGenerator());
    registry.register(new CommentSignalGenerator());
    registry.register(new FunctionNameSignalGenerator());
    return registry;
  }
}
