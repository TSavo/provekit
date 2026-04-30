/**
 * invoke-z3 Stage tests. Mocks child_process.spawn via a test-seam
 * factory so the unit tests don't require the real z3 binary.
 *
 * One opt-in integration test (gated by PROVEKIT_Z3_REAL=1) exercises
 * the real binary if installed.
 */

import { describe, it, expect } from "vitest";
import { Readable, Writable } from "stream";
import { EventEmitter } from "events";
import {
  makeInvokeZ3Stage,
  INVOKE_Z3_CAPABILITY,
  InvokeZ3Error,
} from "./invokeZ3.js";

interface FakeChildOpts {
  stdout?: string;
  stderr?: string;
  exitCode?: number;
  delayMs?: number;
  spawnError?: NodeJS.ErrnoException;
}

function fakeSpawn(opts: FakeChildOpts) {
  return ((..._args: unknown[]) => {
    const ee = new EventEmitter() as EventEmitter & {
      stdin: Writable;
      stdout: Readable;
      stderr: Readable;
      kill(_signal: string): boolean;
    };
    const stdoutChunks: Buffer[] = [];
    if (opts.stdout) stdoutChunks.push(Buffer.from(opts.stdout));
    const stdoutStream = Readable.from(stdoutChunks);
    const stderrStream = Readable.from(
      opts.stderr ? [Buffer.from(opts.stderr)] : [],
    );
    const stdinStream = new Writable({
      write(_chunk, _enc, cb) {
        cb();
      },
    });
    ee.stdin = stdinStream;
    ee.stdout = stdoutStream;
    ee.stderr = stderrStream;
    ee.kill = (_signal: string) => true;

    if (opts.spawnError) {
      setImmediate(() => ee.emit("error", opts.spawnError));
      return ee;
    }

    const finish = () => ee.emit("close", opts.exitCode ?? 0);
    if (opts.delayMs && opts.delayMs > 0) {
      setTimeout(finish, opts.delayMs);
    } else {
      setImmediate(finish);
    }
    return ee;
  }) as unknown as typeof import("child_process").spawn;
}

describe("invoke-z3 stage", () => {
  it("has the expected capability constant", () => {
    expect(INVOKE_Z3_CAPABILITY).toBe("invoke-z3");
  });

  it("returns z3Verdict=unsat when z3 prints unsat", async () => {
    const stage = makeInvokeZ3Stage({
      spawnFn: fakeSpawn({ stdout: "unsat\n" }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.z3Verdict).toBe("unsat");
    expect(out.stdout).toContain("unsat");
  });

  it("returns z3Verdict=sat and parses the model on sat", async () => {
    const modelOutput =
      "sat\n((define-fun x () Int 7)\n (define-fun y () Bool true))\n";
    const stage = makeInvokeZ3Stage({
      spawnFn: fakeSpawn({ stdout: modelOutput }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.z3Verdict).toBe("sat");
    expect(out.counterexample).toBeDefined();
    expect(out.counterexample!["x"]).toEqual({ sort: "Int", bigintString: "7" });
    expect(out.counterexample!["y"]).toEqual({ sort: "Bool", value: true });
  });

  it("returns z3Verdict=unknown when z3 prints unknown", async () => {
    const stage = makeInvokeZ3Stage({
      spawnFn: fakeSpawn({ stdout: "unknown\n" }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.z3Verdict).toBe("unknown");
  });

  it("returns z3Verdict=timeout when the process exceeds timeoutMs", async () => {
    const stage = makeInvokeZ3Stage({
      spawnFn: fakeSpawn({ stdout: "", delayMs: 200 }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n", timeoutMs: 20 });
    expect(out.z3Verdict).toBe("timeout");
  });

  it("throws InvokeZ3Error with a clear message when z3 isn't installed (ENOENT)", async () => {
    const enoent = Object.assign(new Error("spawn z3 ENOENT"), {
      code: "ENOENT",
    }) as NodeJS.ErrnoException;
    const stage = makeInvokeZ3Stage({
      spawnFn: fakeSpawn({ spawnError: enoent }),
    });
    await expect(
      stage.run({ smtLib: "(check-sat)\n", binary: "z3" }),
    ).rejects.toBeInstanceOf(InvokeZ3Error);
    await expect(
      stage.run({ smtLib: "(check-sat)\n", binary: "z3" }),
    ).rejects.toMatchObject({
      message: expect.stringContaining("install z3"),
    });
  });

  it("serializeInput includes timeoutMs so different timeouts cache separately", () => {
    const stage = makeInvokeZ3Stage({ spawnFn: fakeSpawn({ stdout: "unsat\n" }) });
    const a = stage.serializeInput({ smtLib: "x", timeoutMs: 1000 });
    const b = stage.serializeInput({ smtLib: "x", timeoutMs: 2000 });
    expect(a).not.toEqual(b);
  });

  it("output round-trips through serialize/deserialize, with bigintString for Int sorts", () => {
    const stage = makeInvokeZ3Stage({ spawnFn: fakeSpawn({ stdout: "" }) });
    const sample = {
      z3Verdict: "sat" as const,
      stdout: "sat\n",
      stderr: "",
      z3RunMs: 12,
      counterexample: {
        x: { sort: "Int" as const, bigintString: "42" },
        y: { sort: "Bool" as const, value: true },
      },
    };
    const witness = stage.serializeOutput(sample);
    const restored = stage.deserializeOutput(witness);
    expect(restored).toEqual(sample);
  });
});

const realZ3 = process.env.PROVEKIT_Z3_REAL === "1";
describe.skipIf(!realZ3)("invoke-z3 stage (real z3 binary)", () => {
  it("returns unsat for a true assertion", async () => {
    const stage = makeInvokeZ3Stage();
    const out = await stage.run({
      smtLib: "(set-logic ALL)\n(assert (not (= 1 1)))\n(check-sat)\n",
    });
    expect(out.z3Verdict).toBe("unsat");
  });

  it("returns sat with a model for a false assertion", async () => {
    const stage = makeInvokeZ3Stage();
    const out = await stage.run({
      smtLib:
        "(set-logic ALL)\n(declare-fun x () Int)\n(assert (not (> x 0)))\n(check-sat)\n",
    });
    expect(out.z3Verdict).toBe("sat");
    expect(out.counterexample).toBeDefined();
  });
});
