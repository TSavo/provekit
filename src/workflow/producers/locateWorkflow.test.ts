import { describe, it, expect } from "vitest";
import { makeLocateWorkflowStage } from "./locateWorkflow.js";

const SAMPLE_YAML = `
name: example
cid: bafy-example-v1
description: example
nodes:
  - id: only
    capability: noop
    input:
      x: $input.x
output: $node.only.output
`;

describe("locateWorkflow", () => {
  it("reads and parses the YAML at the registered path", async () => {
    const reads: string[] = [];
    const stage = makeLocateWorkflowStage({
      readFile: (p) => {
        reads.push(p);
        return SAMPLE_YAML;
      },
    });
    const out = await stage.run({
      command: "example",
      manifestPaths: { example: "/abs/path/to/example.workflow.yaml" },
    });
    expect(reads).toEqual(["/abs/path/to/example.workflow.yaml"]);
    expect(out.command).toBe("example");
    expect(out.workflow.name).toBe("example");
    expect(out.workflow.cid).toBe("bafy-example-v1");
    expect(out.workflow.nodes).toHaveLength(1);
  });

  it("throws when the command has no registered path", async () => {
    const stage = makeLocateWorkflowStage({
      readFile: () => SAMPLE_YAML,
    });
    await expect(
      stage.run({
        command: "missing",
        manifestPaths: { example: "/abs/path/to/example.workflow.yaml" },
      }),
    ).rejects.toThrow(/no manifest path registered for command "missing"/);
  });

  it("propagates parser errors when the YAML is malformed", async () => {
    const stage = makeLocateWorkflowStage({
      readFile: () => "name: bad\ncid: x\nnodes: not-an-array\noutput: $node.x.output",
    });
    await expect(
      stage.run({
        command: "bad",
        manifestPaths: { bad: "/abs/path/to/bad.workflow.yaml" },
      }),
    ).rejects.toThrow(/manifest\.nodes must be array/);
  });

  it("serializes input with sorted manifestPaths keys for stable hashing", () => {
    const stage = makeLocateWorkflowStage({ readFile: () => SAMPLE_YAML });
    const a = stage.serializeInput({
      command: "example",
      manifestPaths: { z: "/z.yaml", example: "/e.yaml" },
    });
    const b = stage.serializeInput({
      command: "example",
      manifestPaths: { example: "/e.yaml", z: "/z.yaml" },
    });
    expect(JSON.stringify(a)).toBe(JSON.stringify(b));
  });

  it("round-trips serializeOutput / deserializeOutput", async () => {
    const stage = makeLocateWorkflowStage({ readFile: () => SAMPLE_YAML });
    const out = await stage.run({
      command: "example",
      manifestPaths: { example: "/e.yaml" },
    });
    const witness = stage.serializeOutput(out);
    const restored = stage.deserializeOutput(witness);
    expect(restored).toEqual(out);
  });
});
