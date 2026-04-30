import { describe, it, expect } from "vitest";
import type { CliBlock } from "../manifest.js";
import { parseArgv, makeParseArgvStage } from "./parseArgv.js";

const blocks: Record<string, CliBlock> = {
  must: {
    description: "Add an invariant from natural-language intent",
    args: [
      { name: "targetFile", positional: true, required: true, type: "path" },
      { name: "intent", positional: true, required: true, type: "string" },
      { name: "append", flag: true, default: false },
    ],
  },
  refute: {
    description: "Find a counterexample to a propertyHash via Z3",
    args: [
      { name: "propertyHash", positional: true, required: true, type: "string" },
      { name: "timeoutMs", type: "int", default: 5000 },
    ],
  },
  explain: {
    description: "Render a memento's local proof chain",
    args: [{ name: "startCid", positional: true, required: true, type: "string" }],
  },
};

describe("parseArgv", () => {
  it("returns top-level help when argv is empty", () => {
    const out = parseArgv([], blocks);
    expect(out.kind).toBe("help");
    if (out.kind === "help") {
      expect(out.helpText).toContain("Usage: provekit <command> [args]");
      expect(out.helpText).toContain("must");
      expect(out.helpText).toContain("refute");
      expect(out.helpText).toContain("explain");
    }
  });

  it("returns top-level help on --help", () => {
    const out = parseArgv(["--help"], blocks);
    expect(out.kind).toBe("help");
  });

  it("returns top-level help on -h", () => {
    const out = parseArgv(["-h"], blocks);
    expect(out.kind).toBe("help");
  });

  it("hides underscore-prefixed workflows from the help table", () => {
    const withInternal = {
      ...blocks,
      _dispatch: { description: "internal" } satisfies CliBlock,
    };
    const out = parseArgv([], withInternal);
    if (out.kind !== "help") throw new Error("expected help");
    expect(out.helpText).not.toContain("_dispatch");
  });

  it("rejects invocation of underscore-prefixed commands", () => {
    const out = parseArgv(["_dispatch", "x"], blocks);
    expect(out.kind).toBe("unknown");
    if (out.kind === "unknown") {
      expect(out.command).toBe("_dispatch");
      expect(out.helpText).toContain("internal workflow");
    }
  });

  it("returns unknown when command is not in cliBlocks", () => {
    const out = parseArgv(["bogus"], blocks);
    expect(out.kind).toBe("unknown");
    if (out.kind === "unknown") expect(out.command).toBe("bogus");
  });

  it("parses positional args by name in declared order", () => {
    const out = parseArgv(["must", "src/foo.ts", "must hold"], blocks);
    expect(out.kind).toBe("command");
    if (out.kind === "command") {
      expect(out.command).toBe("must");
      expect(out.parsedArgs).toEqual({
        targetFile: "src/foo.ts",
        intent: "must hold",
        append: false,
      });
    }
  });

  it("returns per-command help on `<cmd> --help`", () => {
    const out = parseArgv(["must", "--help"], blocks);
    expect(out.kind).toBe("help");
    if (out.kind === "help") {
      expect(out.helpText).toContain("Usage: provekit must");
      expect(out.helpText).toContain("targetFile");
      expect(out.helpText).toContain("intent");
    }
  });

  it("treats boolean flags as switches without values", () => {
    const out = parseArgv(["must", "src/foo.ts", "must hold", "--append"], blocks);
    if (out.kind !== "command") throw new Error("expected command");
    expect(out.parsedArgs.append).toBe(true);
  });

  it("parses value flags with int coercion", () => {
    const out = parseArgv(["refute", "0xabc", "--timeoutMs", "12000"], blocks);
    if (out.kind !== "command") throw new Error("expected command");
    expect(out.parsedArgs).toEqual({ propertyHash: "0xabc", timeoutMs: 12000 });
  });

  it("falls back to flag default when not supplied", () => {
    const out = parseArgv(["refute", "0xabc"], blocks);
    if (out.kind !== "command") throw new Error("expected command");
    expect(out.parsedArgs.timeoutMs).toBe(5000);
  });

  it("throws on unknown flag", () => {
    expect(() => parseArgv(["must", "src/foo.ts", "x", "--bogus"], blocks)).toThrow(
      /unknown flag --bogus/,
    );
  });

  it("throws on unexpected positional", () => {
    expect(() => parseArgv(["explain", "cidA", "extra"], blocks)).toThrow(
      /unexpected positional/,
    );
  });

  it("throws on missing required positional", () => {
    expect(() => parseArgv(["must", "src/foo.ts"], blocks)).toThrow(
      /missing required positional/,
    );
  });

  it("throws on int coercion failure", () => {
    expect(() => parseArgv(["refute", "0xabc", "--timeoutMs", "abc"], blocks))
      .toThrow(/expected int/);
  });
});

describe("makeParseArgvStage", () => {
  it("serializes input with sorted cliBlock keys for stable hashing", () => {
    const stage = makeParseArgvStage();
    const a = stage.serializeInput({
      argv: ["must", "f", "i"],
      cliBlocks: { z: blocks.refute!, must: blocks.must! },
    });
    const b = stage.serializeInput({
      argv: ["must", "f", "i"],
      cliBlocks: { must: blocks.must!, z: blocks.refute! },
    });
    expect(JSON.stringify(a)).toBe(JSON.stringify(b));
  });

  it("round-trips serializeOutput / deserializeOutput", async () => {
    const stage = makeParseArgvStage();
    const out = await stage.run({
      argv: ["must", "src/foo.ts", "x"],
      cliBlocks: blocks,
    });
    const witness = stage.serializeOutput(out);
    const restored = stage.deserializeOutput(witness);
    expect(restored).toEqual(out);
  });
});
