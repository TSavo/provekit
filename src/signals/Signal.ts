import Parser from "tree-sitter";

export interface ParameterType {
  name: string;
  type: string;
}

export interface Signal {
  file: string;
  line: number;
  column: number;
  type: string;
  text: string;
  functionName: string;
  functionSource: string;
  functionStartLine: number;
  functionEndLine: number;
  parameters: ParameterType[];
  returnType: string;
  pathConditions: string[];
  localTypes: Record<string, string>;
}

export interface SignalGenerator {
  readonly name: string;
  readonly async: boolean;
  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] | Promise<Signal[]>;
}
