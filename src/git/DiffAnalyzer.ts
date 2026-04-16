import { execFileSync } from "child_process";
import { resolve, relative } from "path";

export interface DiffResult {
  changedFiles: string[];
  addedFiles: string[];
  deletedFiles: string[];
  modifiedFiles: string[];
}

export class DiffAnalyzer {
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
  }

  getStagedChanges(): DiffResult {
    return this.parseDiff(["diff", "--cached", "--name-status"]);
  }

  getUnstagedChanges(): DiffResult {
    return this.parseDiff(["diff", "--name-status"]);
  }

  getChangesSince(ref: string): DiffResult {
    return this.parseDiff(["diff", ref, "--name-status"]);
  }

  getChangedTypeScriptFiles(): string[] {
    const staged = this.getStagedChanges();
    const tsFiles = [...staged.addedFiles, ...staged.modifiedFiles]
      .filter((f) => /\.(ts|tsx)$/.test(f) && !f.includes("node_modules") && !f.endsWith(".d.ts"));
    return tsFiles.map((f) => resolve(this.projectRoot, f));
  }

  getWorkingTreeChangedFiles(): string[] {
    const staged = this.getStagedChanges();
    const unstaged = this.getUnstagedChanges();
    const allChanged = new Set([
      ...staged.addedFiles, ...staged.modifiedFiles,
      ...unstaged.addedFiles, ...unstaged.modifiedFiles,
    ]);
    return [...allChanged]
      .filter((f) => /\.(ts|tsx)$/.test(f) && !f.includes("node_modules") && !f.endsWith(".d.ts"))
      .map((f) => resolve(this.projectRoot, f));
  }

  getChangedFilesSince(ref: string): string[] {
    const diff = this.getChangesSince(ref);
    return [...diff.addedFiles, ...diff.modifiedFiles]
      .filter((f) => /\.(ts|tsx)$/.test(f) && !f.includes("node_modules") && !f.endsWith(".d.ts"))
      .map((f) => resolve(this.projectRoot, f));
  }

  isGitRepo(): boolean {
    try {
      const output = execFileSync("git", ["rev-parse", "--is-inside-work-tree"], {
        cwd: this.projectRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      }).trim();
      return output === "true";
    } catch {
      return false;
    }
  }

  getHead(): string | null {
    try {
      return execFileSync("git", ["rev-parse", "HEAD"], {
        cwd: this.projectRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      }).trim();
    } catch {
      return null;
    }
  }

  private parseDiff(args: string[]): DiffResult {
    const result: DiffResult = {
      changedFiles: [],
      addedFiles: [],
      deletedFiles: [],
      modifiedFiles: [],
    };

    let output: string;
    try {
      output = execFileSync("git", args, {
        cwd: this.projectRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      }).trim();
    } catch {
      return result;
    }

    if (!output) return result;

    for (const line of output.split("\n")) {
      const parts = line.split("\t");
      if (parts.length < 2) continue;
      const status = parts[0]!.trim();
      const file = parts[parts.length - 1]!.trim();

      result.changedFiles.push(file);
      if (status.startsWith("A")) result.addedFiles.push(file);
      else if (status.startsWith("D")) result.deletedFiles.push(file);
      else if (status.startsWith("M") || status.startsWith("R")) result.modifiedFiles.push(file);
    }

    return result;
  }
}
