import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts", "prompts/**/*.test.ts"],
    exclude: ["dist/**", "node_modules/**", "examples/**", ".neurallog/**", ".worktrees/**"],
  },
});
