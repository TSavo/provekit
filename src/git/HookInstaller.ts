import { writeFileSync, readFileSync, existsSync, chmodSync, mkdirSync } from "fs";
import { join } from "path";
import { execFileSync } from "child_process";

const HOOK_MARKER = "# neurallog pre-commit hook";

const HOOK_SCRIPT = `#!/bin/sh
${HOOK_MARKER}
# Installed by neurallog init. Runs Phase 5 (Z3 only, no LLM, no network).
# To remove: neurallog hook --uninstall

# Use local dev binary if available, otherwise installed package
if [ -f "src/cli.ts" ]; then
  npx tsx src/cli.ts verify --ci 2>&1
else
  npx neurallog verify --ci 2>&1
fi
exit_code=$?

if [ $exit_code -ne 0 ]; then
  echo ""
  echo "neurallog: commit blocked. Fix violations or override:"
  echo "  neurallog override --reason \\"intentional\\""
  echo "  git commit --no-verify"
  echo ""
fi

exit $exit_code
`;

export class HookInstaller {
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
  }

  install(): { installed: boolean; path: string; message: string } {
    const hooksDir = this.getHooksDir();
    if (!hooksDir) {
      return { installed: false, path: "", message: "Not a git repository" };
    }

    mkdirSync(hooksDir, { recursive: true });
    const hookPath = join(hooksDir, "pre-commit");

    if (existsSync(hookPath)) {
      const existing = readFileSync(hookPath, "utf-8");
      if (existing.includes(HOOK_MARKER)) {
        return { installed: true, path: hookPath, message: "Hook already installed" };
      }

      const combined = existing.trimEnd() + "\n\n" + HOOK_SCRIPT;
      writeFileSync(hookPath, combined);
      chmodSync(hookPath, 0o755);
      return { installed: true, path: hookPath, message: "Hook appended to existing pre-commit" };
    }

    writeFileSync(hookPath, HOOK_SCRIPT);
    chmodSync(hookPath, 0o755);
    return { installed: true, path: hookPath, message: "Hook installed" };
  }

  uninstall(): { removed: boolean; message: string } {
    const hooksDir = this.getHooksDir();
    if (!hooksDir) {
      return { removed: false, message: "Not a git repository" };
    }

    const hookPath = join(hooksDir, "pre-commit");
    if (!existsSync(hookPath)) {
      return { removed: false, message: "No pre-commit hook found" };
    }

    const content = readFileSync(hookPath, "utf-8");
    if (!content.includes(HOOK_MARKER)) {
      return { removed: false, message: "Pre-commit hook exists but was not installed by neurallog" };
    }

    const lines = content.split("\n");
    const markerIdx = lines.findIndex((l) => l.includes(HOOK_MARKER));

    if (markerIdx <= 1 && lines.filter((l) => l.trim() && !l.startsWith("#")).length <= 5) {
      const { unlinkSync } = require("fs");
      unlinkSync(hookPath);
      return { removed: true, message: "Hook removed" };
    }

    const before = lines.slice(0, markerIdx).join("\n");
    const afterStart = lines.findIndex((l, i) => i > markerIdx && l.startsWith("exit $exit_code"));
    const after = afterStart !== -1 ? lines.slice(afterStart + 1).join("\n") : "";
    const cleaned = (before + "\n" + after).trim() + "\n";
    writeFileSync(hookPath, cleaned);
    chmodSync(hookPath, 0o755);
    return { removed: true, message: "neurallog hook removed, other hooks preserved" };
  }

  isInstalled(): boolean {
    const hooksDir = this.getHooksDir();
    if (!hooksDir) return false;

    const hookPath = join(hooksDir, "pre-commit");
    if (!existsSync(hookPath)) return false;

    return readFileSync(hookPath, "utf-8").includes(HOOK_MARKER);
  }

  private getHooksDir(): string | null {
    try {
      const gitDir = execFileSync("git", ["rev-parse", "--git-dir"], {
        cwd: this.projectRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      }).trim();

      const resolvedGitDir = join(this.projectRoot, gitDir);
      return join(resolvedGitDir, "hooks");
    } catch {
      return null;
    }
  }
}
