import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import Parser from "tree-sitter";
import { SignalRegistry } from "./SignalRegistry";
import type { Signal, SignalGenerator } from "./Signal";
import { parseFile } from "../parser";

function fakeSignal(overrides: Partial<Signal> = {}): Signal {
  return {
    file: "x.ts",
    line: 1,
    column: 0,
    type: "fake",
    text: "",
    functionName: "fn",
    functionSource: "",
    functionStartLine: 1,
    functionEndLine: 1,
    parameters: [],
    returnType: "unknown",
    pathConditions: [],
    localTypes: {},
    callees: [],
    calledBy: [],
    ...overrides,
  };
}

class SyncGen implements SignalGenerator {
  readonly name = "sync";
  readonly async = false;
  constructor(private signals: Signal[]) {}
  findSignals(): Signal[] {
    return this.signals;
  }
}

class AsyncGen implements SignalGenerator {
  readonly name = "async";
  readonly async = true;
  constructor(private signals: Signal[]) {}
  async findSignals(): Promise<Signal[]> {
    return this.signals;
  }
}

describe("SignalRegistry", () => {
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });
  afterEach(() => {
    logSpy.mockRestore();
  });

  it("findAll skips async generators and sorts by line", () => {
    const reg = new SignalRegistry();
    reg.register(new SyncGen([fakeSignal({ line: 30 }), fakeSignal({ line: 10 })]));
    reg.register(new AsyncGen([fakeSignal({ line: 5 })]));

    const tree = parseFile("const x = 1;");
    const result = reg.findAll("x.ts", "const x = 1;", tree);
    expect(result.map((s) => s.line)).toEqual([10, 30]);
  });

  it("findAllAsync includes results from both sync and async generators", async () => {
    const reg = new SignalRegistry();
    reg.register(new SyncGen([fakeSignal({ line: 20 })]));
    reg.register(new AsyncGen([fakeSignal({ line: 5 })]));

    const tree = parseFile("const x = 1;");
    const result = await reg.findAllAsync("x.ts", "const x = 1;", tree);
    expect(result.map((s) => s.line)).toEqual([5, 20]);
  });

  it("hasAsyncGenerators reflects registration", () => {
    const reg = new SignalRegistry();
    expect(reg.hasAsyncGenerators()).toBe(false);
    reg.register(new SyncGen([]));
    expect(reg.hasAsyncGenerators()).toBe(false);
    reg.register(new AsyncGen([]));
    expect(reg.hasAsyncGenerators()).toBe(true);
  });

  it("getGeneratorNames returns the names in registration order", () => {
    const reg = new SignalRegistry();
    reg.register(new SyncGen([]));
    reg.register(new AsyncGen([]));
    expect(reg.getGeneratorNames()).toEqual(["sync", "async"]);
  });

  it("createDefault registers exactly the AST generator", () => {
    expect(SignalRegistry.createDefault().getGeneratorNames()).toEqual(["ast"]);
  });

  it("createRuleBased registers log/comment/function-name", () => {
    expect(SignalRegistry.createRuleBased().getGeneratorNames()).toEqual([
      "log",
      "comment",
      "function-name",
    ]);
  });

  it("resolveCalledBy populates calledBy from callees", () => {
    const a = fakeSignal({ functionName: "alpha", callees: ["beta"] });
    const b = fakeSignal({ functionName: "beta", callees: [] });
    const signals = [a, b];

    SignalRegistry.resolveCalledBy(signals);
    expect(b.calledBy).toEqual(["alpha"]);
    expect(a.calledBy).toEqual([]);
  });

  it("resolveCalledBy de-dupes repeated callers", () => {
    const a = fakeSignal({ functionName: "alpha", callees: ["beta", "beta", "beta"] });
    const b = fakeSignal({ functionName: "beta", callees: [] });
    SignalRegistry.resolveCalledBy([a, b]);
    expect(b.calledBy).toEqual(["alpha"]);
  });

  it("resolveCalledBy never lists a function as caller of itself", () => {
    const a = fakeSignal({ functionName: "alpha", callees: ["alpha"] });
    SignalRegistry.resolveCalledBy([a]);
    expect(a.calledBy).toEqual([]);
  });
});
