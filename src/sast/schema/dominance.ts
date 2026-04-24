import {
  sqliteTable,
  text,
  index,
  primaryKey,
} from "drizzle-orm/sqlite-core";
import { nodes } from "./nodes.js";

export const dominance = sqliteTable(
  "dominance",
  {
    dominator: text("dominator").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    dominated: text("dominated").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.dominator, t.dominated] }),
    byDominated: index("dominance_by_dominated_dominator").on(t.dominated, t.dominator),
  }),
);

export const postDominance = sqliteTable(
  "post_dominance",
  {
    postDominator: text("post_dominator").notNull().references(() => nodes.id, { onDelete: "cascade" }),
    postDominated: text("post_dominated").notNull().references(() => nodes.id, { onDelete: "cascade" }),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.postDominator, t.postDominated] }),
    byPostDominated: index("post_dominance_by_post_dominated_post_dominator").on(t.postDominated, t.postDominator),
  }),
);
