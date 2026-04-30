import type { Config } from "drizzle-kit";

export default {
  schema: "./implementations/typescript/src/db/schema/index.ts",
  out: "./drizzle",
  dialect: "sqlite",
  dbCredentials: {
    url: ".provekit/provekit.db",
  },
} satisfies Config;
