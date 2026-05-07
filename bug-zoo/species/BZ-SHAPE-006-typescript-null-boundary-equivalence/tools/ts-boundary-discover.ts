import { discoverBoundary } from "./ts-boundary-discovery.js";

process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

const [surface, workspaceRoot] = process.argv.slice(2);
if (!surface || !workspaceRoot) {
  console.error("Usage: ts-boundary-discover.ts <surface> <workspaceRoot>");
  process.exit(1);
}

process.stdout.write(JSON.stringify(discoverBoundary(surface, workspaceRoot)) + "\n");
