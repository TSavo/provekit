import Parser from "tree-sitter";
import { Signal, SignalGenerator } from "./Signal";
import { ASTSignalGenerator } from "./ASTSignalGenerator";
import { LogSignalGenerator } from "./LogSignalGenerator";
import { CommentSignalGenerator } from "./CommentSignalGenerator";
import { FunctionNameSignalGenerator } from "./FunctionNameSignalGenerator";
import { LLMSignalGenerator, LLMSignalConfig } from "./LLMSignalGenerator";

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

  static resolveCalledBy(signals: Signal[]): void {
    const byFunction = new Map<string, Signal[]>();
    for (const s of signals) {
      if (!byFunction.has(s.functionName)) byFunction.set(s.functionName, []);
      byFunction.get(s.functionName)!.push(s);
    }

    for (const s of signals) {
      s.calledBy = [];
    }

    for (const caller of signals) {
      for (const calleeName of caller.callees) {
        const targets = byFunction.get(calleeName);
        if (targets) {
          for (const target of targets) {
            if (target !== caller && !target.calledBy.includes(caller.functionName)) {
              target.calledBy.push(caller.functionName);
            }
          }
        }
      }
    }
  }

  static createDefault(): SignalRegistry {
    const registry = new SignalRegistry();
    registry.register(new ASTSignalGenerator());
    return registry;
  }

  static createLLM(config?: LLMSignalConfig): SignalRegistry {
    const registry = new SignalRegistry();
    registry.register(new LLMSignalGenerator(config));
    return registry;
  }

  static createRuleBased(): SignalRegistry {
    const registry = new SignalRegistry();
    registry.register(new LogSignalGenerator());
    registry.register(new CommentSignalGenerator());
    registry.register(new FunctionNameSignalGenerator());
    return registry;
  }
}
