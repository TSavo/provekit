import { describe, it, expect, beforeEach } from "vitest";
import {
  parseBugSignal,
  detectAndParseBugSignal,
  _clearIntakeRegistry,
} from "./intake.js";
import { StubLLMProvider } from "./types.js";
import { registerAll } from "./intakeAdapters/index.js";
import { getIntakeAdapter } from "./intakeRegistry.js";

// ---------------------------------------------------------------------------
// Shared fixture: re-populate adapters before each test.
// ---------------------------------------------------------------------------

beforeEach(() => {
  _clearIntakeRegistry();
  registerAll();
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function reportLlm(overrides?: Record<string, string>): StubLLMProvider {
  const base: Record<string, string> = {
    "Bug report": JSON.stringify({
      summary: "Division crashes when denominator is 0.",
      failureDescription: "A division-by-zero error occurs in the calculate function.",
      fixHint: "Guard denominator before dividing.",
      codeReferences: [{ file: "src/math.ts", line: 42, function: "calculate" }],
      bugClassHint: "divide-by-zero",
    }),
  };
  return new StubLLMProvider(new Map(Object.entries({ ...base, ...overrides })));
}

function gapLlm(): StubLLMProvider {
  return new StubLLMProvider(
    new Map([
      [
        "SAST gap finding",
        JSON.stringify({
          summary: "Possible null dereference at src/api.ts:10.",
          failureDescription: "Value may be null at assignment in fetchUser.",
        }),
      ],
    ]),
  );
}

function testFailureLlm(): StubLLMProvider {
  return new StubLLMProvider(
    new Map([
      [
        "failing test",
        JSON.stringify({
          summary: "Test 'should return user' failed: TypeError.",
          failureDescription: "fetchUser returned undefined instead of a User object.",
        }),
      ],
    ]),
  );
}

function runtimeLogLlm(): StubLLMProvider {
  return new StubLLMProvider(
    new Map([
      [
        "runtime-log analyst",
        JSON.stringify({
          summary: "Unhandled TypeError in processPayment.",
          failureDescription: "Cannot read property 'amount' of undefined.",
          bugClassHint: "null-dereference",
        }),
      ],
    ]),
  );
}

// ---------------------------------------------------------------------------
// "report" adapter tests
// ---------------------------------------------------------------------------

describe("parseBugSignal — source=report", () => {
  it("parses a plain bug report text into a BugSignal", async () => {
    const result = await parseBugSignal(
      { text: "Division crashes when denominator is 0", source: "report" },
      reportLlm(),
    );
    expect(result.source).toBe("report");
    expect(result.summary).toContain("Division");
    expect(result.failureDescription).toContain("division-by-zero");
    expect(result.codeReferences).toHaveLength(1);
    expect(result.codeReferences[0].file).toBe("src/math.ts");
    expect(result.codeReferences[0].line).toBe(42);
    expect(result.codeReferences[0].function).toBe("calculate");
    expect(result.bugClassHint).toBe("divide-by-zero");
    expect(result.fixHint).toBeDefined();
  });

  it("preserves rawText from input", async () => {
    const text = "Something crashes in production";
    const result = await parseBugSignal({ text, source: "report" }, reportLlm());
    expect(result.rawText).toBe(text);
  });
});

// ---------------------------------------------------------------------------
// "gap_report" adapter tests
// ---------------------------------------------------------------------------

describe("parseBugSignal — source=gap_report", () => {
  it("extracts codeReferences from sourceLine mechanically", async () => {
    const result = await parseBugSignal(
      {
        text: "",
        source: "gap_report",
        context: {
          gapReportId: 7,
          reason: "Possible null dereference",
          sourceLine: "src/api.ts:10:fetchUser",
          principleId: "null-safety",
        },
      },
      gapLlm(),
    );
    expect(result.source).toBe("gap_report");
    expect(result.codeReferences).toHaveLength(1);
    expect(result.codeReferences[0].file).toBe("src/api.ts");
    expect(result.codeReferences[0].line).toBe(10);
    expect(result.codeReferences[0].function).toBe("fetchUser");
    expect(result.bugClassHint).toBe("null-safety");
  });

  it("falls back to file+line fields when sourceLine is absent", async () => {
    const result = await parseBugSignal(
      {
        text: "gap reason",
        source: "gap_report",
        context: {
          gapReportId: 3,
          reason: "Unsafe cast",
          file: "src/core.ts",
          line: 88,
        },
      },
      gapLlm(),
    );
    expect(result.codeReferences[0].file).toBe("src/core.ts");
    expect(result.codeReferences[0].line).toBe(88);
  });

  it("throws when context is missing required fields", async () => {
    await expect(
      parseBugSignal(
        { text: "", source: "gap_report", context: { gapReportId: 1 } },
        gapLlm(),
      ),
    ).rejects.toThrow("gap_report adapter");
  });
});

// ---------------------------------------------------------------------------
// "test_failure" adapter tests
// ---------------------------------------------------------------------------

describe("parseBugSignal — source=test_failure", () => {
  const stack = `Error: expected 1 to be 2
    at Object.<anonymous> (src/math.test.ts:15:5)
    at runTest (node_modules/vitest/dist/index.js:100:3)`;

  it("extracts file+line from stack trace", async () => {
    const result = await parseBugSignal(
      {
        text: "",
        source: "test_failure",
        context: {
          testName: "should return user",
          errorMessage: "TypeError: Cannot read property",
          stack,
        },
      },
      testFailureLlm(),
    );
    expect(result.source).toBe("test_failure");
    expect(result.codeReferences.length).toBeGreaterThan(0);
    const first = result.codeReferences[0];
    expect(first.file).toContain("math.test.ts");
    expect(first.line).toBe(15);
  });

  it("returns BugSignal with LLM-generated summary", async () => {
    const result = await parseBugSignal(
      {
        text: "",
        source: "test_failure",
        context: {
          testName: "should return user",
          errorMessage: "TypeError: Cannot read property",
          stack,
        },
      },
      testFailureLlm(),
    );
    expect(result.summary).toContain("Test");
    expect(result.bugClassHint).toBe("test-failure");
  });

  it("throws when context is missing testName", async () => {
    await expect(
      parseBugSignal(
        {
          text: "",
          source: "test_failure",
          context: { errorMessage: "some error" },
        },
        testFailureLlm(),
      ),
    ).rejects.toThrow("test_failure adapter");
  });
});

// ---------------------------------------------------------------------------
// "runtime_log" adapter tests
// ---------------------------------------------------------------------------

describe("parseBugSignal — source=runtime_log", () => {
  const stackTrace = `TypeError: Cannot read property 'amount' of undefined
    at processPayment (src/payment.ts:55:12)
    at handleRequest (src/server.ts:30:5)`;

  it("extracts codeReferences from stack trace text", async () => {
    const result = await parseBugSignal(
      { text: stackTrace, source: "runtime_log" },
      runtimeLogLlm(),
    );
    expect(result.source).toBe("runtime_log");
    expect(result.codeReferences.length).toBeGreaterThan(0);
    const first = result.codeReferences[0];
    expect(first.file).toContain("payment.ts");
    expect(first.line).toBe(55);
    expect(first.function).toBe("processPayment");
  });

  it("sets bugClassHint from LLM response", async () => {
    const result = await parseBugSignal(
      { text: stackTrace, source: "runtime_log" },
      runtimeLogLlm(),
    );
    expect(result.bugClassHint).toBe("null-dereference");
  });
});

// ---------------------------------------------------------------------------
// Unknown source
// ---------------------------------------------------------------------------

describe("parseBugSignal — unknown source", () => {
  it("throws with a helpful error listing registered adapter names", async () => {
    await expect(
      parseBugSignal(
        { text: "something", source: "sentry" },
        new StubLLMProvider(new Map()),
      ),
    ).rejects.toThrow(/unknown intake source 'sentry'. Registered: /);
  });

  it("error message lists all four v1 adapter names", async () => {
    try {
      await parseBugSignal(
        { text: "x", source: "does_not_exist" },
        new StubLLMProvider(new Map()),
      );
      expect.fail("should have thrown");
    } catch (e) {
      const msg = (e as Error).message;
      expect(msg).toContain("report");
      expect(msg).toContain("gap_report");
      expect(msg).toContain("test_failure");
      expect(msg).toContain("runtime_log");
    }
  });
});

// ---------------------------------------------------------------------------
// detectAndParseBugSignal — auto-routing
// ---------------------------------------------------------------------------

describe("detectAndParseBugSignal", () => {
  it("routes to gap_report (score 1.0) over report (score 0.5)", async () => {
    const result = await detectAndParseBugSignal(
      {
        text: "",
        context: {
          gapReportId: 5,
          reason: "Unsafe cast",
          file: "src/foo.ts",
          line: 1,
        },
      },
      gapLlm(),
    );
    expect(result.source).toBe("gap_report");
  });

  it("routes to test_failure (score 1.0) when context has testName+errorMessage", async () => {
    const result = await detectAndParseBugSignal(
      {
        text: "",
        context: {
          testName: "my test",
          errorMessage: "assertion failed",
        },
      },
      testFailureLlm(),
    );
    expect(result.source).toBe("test_failure");
  });

  it("routes to runtime_log (score 0.7) for stack-trace text over report (0.5)", async () => {
    const text = "Error\n    at processPayment (src/payment.ts:55:12)";
    const result = await detectAndParseBugSignal({ text }, runtimeLogLlm());
    expect(result.source).toBe("runtime_log");
  });

  it("falls back to report (0.5) for plain unstructured text", async () => {
    const result = await detectAndParseBugSignal(
      { text: "Division crashes when denominator is 0" },
      reportLlm(),
    );
    expect(result.source).toBe("report");
  });
});

// ---------------------------------------------------------------------------
// Adapter detect() confidence tests
// ---------------------------------------------------------------------------

describe("adapter detect() functions", () => {
  it("report adapter returns 0.5 for any input", () => {
    const adapter = getIntakeAdapter("report")!;
    expect(adapter.detect!({ text: "anything" })).toBe(0.5);
    expect(adapter.detect!({ text: "", context: { gapReportId: 1, reason: "x" } })).toBe(0.5);
  });

  it("gap_report adapter returns 1.0 when context has gapReportId+reason, 0 otherwise", () => {
    const adapter = getIntakeAdapter("gap_report")!;
    expect(adapter.detect!({ text: "", context: { gapReportId: 1, reason: "r" } })).toBe(1.0);
    expect(adapter.detect!({ text: "plain text" })).toBe(0);
    expect(adapter.detect!({ text: "", context: { gapReportId: 1 } })).toBe(0);
  });

  it("test_failure adapter returns 1.0 when context has testName+errorMessage, 0 otherwise", () => {
    const adapter = getIntakeAdapter("test_failure")!;
    expect(
      adapter.detect!({ text: "", context: { testName: "t", errorMessage: "e" } }),
    ).toBe(1.0);
    expect(adapter.detect!({ text: "stack trace" })).toBe(0);
    expect(adapter.detect!({ text: "", context: { testName: "t" } })).toBe(0);
  });

  it("runtime_log adapter returns 0.7 for stack-trace text, 0 for plain text", () => {
    const adapter = getIntakeAdapter("runtime_log")!;
    expect(adapter.detect!({ text: "    at processPayment (src/payment.ts:55:12)" })).toBe(0.7);
    expect(adapter.detect!({ text: "everything is fine" })).toBe(0);
  });
});
