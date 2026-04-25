import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts", "prompts/**/*.test.ts"],
    exclude: ["dist/**", "node_modules/**", "examples/**", ".neurallog/**", ".worktrees/**"],
    // Opt every test out of B3 recognize by default. Tests that exercise B3
    // explicitly delete the env var or set it to "0" before runFixLoop. This
    // prevents test fixtures from triggering recognize -> mechanical-mode ->
    // provenance writes against the canonical .provekit/principles/*.json.
    env: {
      PROVEKIT_DISABLE_RECOGNIZE: "1",
    },
  },
});
