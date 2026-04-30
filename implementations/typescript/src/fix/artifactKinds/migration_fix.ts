import { registerArtifactKind } from "../artifactKindRegistry.js";
import { primaryFixFiles, touchesAny, isMigrationFile } from "./helpers.js";

export function registerMigrationFix() {
  registerArtifactKind({
    name: "migration_fix",
    description: "DB migration correction or follow-up; new file in drizzle/ or migrations/.",
    oraclesThatApply: [11, 14],
    isPresent: (a) => touchesAny(primaryFixFiles(a), isMigrationFile),
    bundleTypeScope: "both",
  });
}
registerMigrationFix();
