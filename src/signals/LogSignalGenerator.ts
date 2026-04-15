import Parser from "tree-sitter";
import { Signal, SignalGenerator } from "./Signal";
import { parseFile, findLogStatements } from "../parser";

export class LogSignalGenerator implements SignalGenerator {
  readonly name = "log";
  readonly async = false;

  findSignals(filePath: string, source: string, tree: Parser.Tree): Signal[] {
    const callSites = findLogStatements(tree, source);
    return callSites.map((site) => ({
      file: filePath,
      line: site.line,
      column: site.column,
      type: "log",
      text: site.logText,
      functionName: site.functionName,
      functionSource: site.functionSource,
      functionStartLine: site.functionStartLine,
      functionEndLine: site.functionEndLine,
      parameters: site.parameters,
      returnType: site.returnType,
      pathConditions: site.pathConditions,
      localTypes: site.localTypes,
      callees: site.callees,
    }));
  }
}
