/**
 * Tests for the command-alias expansion.
 *
 * The aliases are purely lexical — argv[0] is rewritten to the
 * canonical verb before parseArgv sees it. Test exercises the
 * expansion function directly; spawning the binary would re-run the
 * workflow setup per case, which the unit-test layer doesn't need.
 */

import { describe, it, expect } from "vitest";
import { expandCommandAlias } from "./cli.js";

describe("expandCommandAlias", () => {
  it("rewrites 'will' to 'must'", () => {
    expect(expandCommandAlias(["will", "X", "Y"])).toEqual(["must", "X", "Y"]);
  });

  it("rewrites 'always' to 'must'", () => {
    expect(expandCommandAlias(["always", "X"])).toEqual(["must", "X"]);
  });

  it("rewrites 'shall' to 'must'", () => {
    expect(expandCommandAlias(["shall", "X"])).toEqual(["must", "X"]);
  });

  it("rewrites 'verifies' to 'verify'", () => {
    expect(expandCommandAlias(["verifies", "--ci"])).toEqual(["verify", "--ci"]);
  });

  it("rewrites 'changes' to 'change'", () => {
    expect(expandCommandAlias(["changes", "X"])).toEqual(["change", "X"]);
  });

  it("rewrites 'proves' to 'prove'", () => {
    expect(expandCommandAlias(["proves", "X"])).toEqual(["prove", "X"]);
  });

  it("leaves unrecognized commands unchanged", () => {
    expect(expandCommandAlias(["banana", "X"])).toEqual(["banana", "X"]);
  });

  it("leaves canonical verbs unchanged", () => {
    expect(expandCommandAlias(["must", "X"])).toEqual(["must", "X"]);
    expect(expandCommandAlias(["verify"])).toEqual(["verify"]);
  });

  it("leaves empty argv alone", () => {
    expect(expandCommandAlias([])).toEqual([]);
  });

  it("only rewrites argv[0], not later positions", () => {
    expect(expandCommandAlias(["must", "will-make"])).toEqual(["must", "will-make"]);
  });

  it("preserves all flags after the verb", () => {
    expect(expandCommandAlias(["verifies", "--hook", "--ci", "--verbose"])).toEqual([
      "verify",
      "--hook",
      "--ci",
      "--verbose",
    ]);
  });
});
