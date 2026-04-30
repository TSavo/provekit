/**
 * invoke-cvc5 Stage tests. Mocks child_process.spawn via a test-seam
 * factory so the unit tests don't require the real cvc5 binary.
 *
 * One opt-in integration test (gated by PROVEKIT_CVC5_REAL=1) exercises
 * the real binary if installed.
 */

import { describe, it, expect } from "vitest";
import { Readable, Writable } from "stream";
import { EventEmitter } from "events";
import {
  makeInvokeCvc5Stage,
  INVOKE_CVC5_CAPABILITY,
  InvokeCvc5Error,
} from "./invokeCvc5.js";

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

describe("invoke-cvc5 stage", () => {
  it("has the expected capability constant", () => {
    expect(INVOKE_CVC5_CAPABILITY).toBe("invoke-cvc5");
  });

  it("returns cvc5Verdict=unsat when cvc5 prints unsat", async () => {
    const stage = makeInvokeCvc5Stage({
      spawnFn: fakeSpawn({ stdout: "unsat\n" }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.cvc5Verdict).toBe("unsat");
    expect(out.stdout).toContain("unsat");
  });

  it("returns cvc5Verdict=sat when cvc5 prints sat", async () => {
    const stage = makeInvokeCvc5Stage({
      spawnFn: fakeSpawn({ stdout: "sat\n" }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.cvc5Verdict).toBe("sat");
  });

  it("returns cvc5Verdict=unknown when cvc5 prints unknown", async () => {
    const stage = makeInvokeCvc5Stage({
      spawnFn: fakeSpawn({ stdout: "unknown\n" }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n" });
    expect(out.cvc5Verdict).toBe("unknown");
  });

  it("returns cvc5Verdict=timeout when the process exceeds timeoutMs", async () => {
    const stage = makeInvokeCvc5Stage({
      spawnFn: fakeSpawn({ stdout: "", delayMs: 200 }),
    });
    const out = await stage.run({ smtLib: "(check-sat)\n", timeoutMs: 20 });
    expect(out.cvc5Verdict).toBe("timeout");
  });

  it("throws InvokeCvc5Error with a clear message when cvc5 isn't installed (ENOENT)", async () => {
    const enoent = Object.assign(new Error("spawn cvc5 ENOENT"), {
      code: "ENOENT",
    }) as NodeJS.ErrnoException;
    const stage = makeInvokeCvc5Stage({
      spawnFn: fakeSpawn({ spawnError: enoent }),
    });
    await expect(
      stage.run({ smtLib: "(check-sat)\n", binary: "cvc5" }),
    ).rejects.toBeInstanceOf(InvokeCvc5Error);
    await expect(
      stage.run({ smtLib: "(check-sat)\n", binary: "cvc5" }),
    ).rejects.toMatchObject({
      message: expect.stringContaining("install cvc5"),
    });
  });

  it("serializeInput includes timeoutMs so different timeouts cache separately", () => {
    const stage = makeInvokeCvc5Stage({ spawnFn: fakeSpawn({ stdout: "unsat\n" }) });
    const a = stage.serializeInput({ smtLib: "x", timeoutMs: 1000 });
    const b = stage.serializeInput({ smtLib: "x", timeoutMs: 2000 });
    expect(a).not.toEqual(b);
  });

  it("output round-trips through serialize/deserialize", () => {
    const stage = makeInvokeCvc5Stage({ spawnFn: fakeSpawn({ stdout: "" }) });
    const sample = {
      cvc5Verdict: "unsat" as const,
      stdout: "unsat\n",
      stderr: "",
      cvc5RunMs: 12,
    };
    const witness = stage.serializeOutput(sample);
    const restored = stage.deserializeOutput(witness);
    expect(restored).toEqual(sample);
  });

  it("uses a producer identity distinct from the Z3 invoker", () => {
    const stage = makeInvokeCvc5Stage();
    expect(stage.producedBy).toContain("cvc5");
    expect(stage.producedBy).not.toContain("z3");
  });
});

const realCvc5 = process.env.PROVEKIT_CVC5_REAL === "1";
describe.skipIf(!realCvc5)("invoke-cvc5 stage (real cvc5 binary)", () => {
  it("returns unsat for a true assertion", async () => {
    const stage = makeInvokeCvc5Stage();
    const out = await stage.run({
      smtLib: "(set-logic ALL)\n(assert (not (= 1 1)))\n(check-sat)\n",
    });
    expect(out.cvc5Verdict).toBe("unsat");
  });

  it("returns sat for an existentially-satisfiable assertion", async () => {
    const stage = makeInvokeCvc5Stage();
    const out = await stage.run({
      smtLib:
        "(set-logic ALL)\n(declare-fun x () Int)\n(assert (not (> x 0)))\n(check-sat)\n",
    });
    expect(out.cvc5Verdict).toBe("sat");
  });
});
