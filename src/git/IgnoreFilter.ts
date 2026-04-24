import { readFileSync, existsSync } from "fs";
import { join, relative } from "path";

export class IgnoreFilter {
  private patterns: string[] = [];
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.load();
  }

  private load(): void {
    const ignorePath = join(this.projectRoot, ".provekitignore");
    if (!existsSync(ignorePath)) return;

    const content = readFileSync(ignorePath, "utf-8");
    this.patterns = content
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0 && !l.startsWith("#"));

    if (this.patterns.length > 0) {
      console.log(`[provekit] Loaded ${this.patterns.length} ignore patterns from .provekitignore`);
    }
  }

  isIgnored(filePath: string): boolean {
    if (this.patterns.length === 0) return false;

    const rel = filePath.startsWith(this.projectRoot)
      ? relative(this.projectRoot, filePath)
      : filePath;

    for (const pattern of this.patterns) {
      if (this.matches(rel, pattern)) return true;
      if (this.matches(filePath, pattern)) return true;
    }

    return false;
  }

  private matches(path: string, pattern: string): boolean {
    if (pattern.includes("*")) {
      const regex = new RegExp(
        "^" + pattern.replace(/\./g, "\\.").replace(/\*\*/g, "{{GLOBSTAR}}").replace(/\*/g, "[^/]*").replace(/\{\{GLOBSTAR\}\}/g, ".*") + "$"
      );
      return regex.test(path);
    }

    if (path === pattern) return true;
    if (path.startsWith(pattern + "/")) return true;
    if (path.endsWith("/" + pattern)) return true;
    if (path.includes("/" + pattern + "/")) return true;
    if (path.includes("/" + pattern)) return true;

    return false;
  }

  getPatterns(): string[] {
    return [...this.patterns];
  }
}
