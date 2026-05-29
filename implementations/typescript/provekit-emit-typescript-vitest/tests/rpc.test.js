"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const { dispatch } = require("../src/rpc");

test("rpc invoke emits TypeScript Vitest source", () => {
  const response = dispatch({
    id: 1,
    method: "provekit.plugin.invoke",
    params: {
      function: "identity",
      predicates: [
        {
          kind: "op",
          name: "concept:eq",
          args: [
            { kind: "const", value: 2 },
            { kind: "const", value: 2 },
          ],
        },
      ],
    },
  });

  assert.equal(response.jsonrpc, "2.0");
  assert.equal(response.id, 1);
  assert.ok(!response.error, `unexpected error: ${JSON.stringify(response.error)}`);
  assert.equal(response.result.path, "provekit_identity.test.ts");
  assert.match(response.result.source, /expect\(2\)\.toEqual\(2\);/);
});

test("rpc check runs Vitest on emitted artifact", () => {
  const outDir = fs.mkdtempSync(path.join(os.tmpdir(), "provekit-ts-vitest-check-"));
  const artifactPath = path.join(outDir, "provekit_identity.test.ts");
  fs.writeFileSync(
    artifactPath,
    'describe("provekit", () => {\n  it("passes", () => {\n    expect(2).toEqual(2);\n  });\n});\n',
  );

  const response = dispatch({
    id: 2,
    method: "provekit.plugin.check",
    params: { out_dir: outDir, artifact_path: artifactPath },
  });

  assert.equal(response.jsonrpc, "2.0");
  assert.equal(response.id, 2);
  assert.equal(response.result.ok, true, `stdout:\n${response.result.stdout}\nstderr:\n${response.result.stderr}`);
});
