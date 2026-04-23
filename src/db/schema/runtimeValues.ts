import { sqliteTable, integer, text, real, index, primaryKey } from "drizzle-orm/sqlite-core";

export const runtimeValues = sqliteTable(
  "runtime_values",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    kind: text("kind", {
      enum: [
        "number",
        "string",
        "bool",
        "null",
        "undefined",
        "object",
        "array",
        "function",
        "bigint",
        "symbol",
        "nan",
        "infinity",
        "neg_infinity",
        "circular",
        "truncated",
      ],
    }).notNull(),
    numberValue: real("number_value"),
    stringValue: text("string_value"),
    boolValue: integer("bool_value", { mode: "boolean" }),
    circularTargetId: integer("circular_target_id").references((): any => runtimeValues.id),
    truncationNote: text("truncation_note"),
  },
  (t) => ({
    byKind: index("rv_by_kind").on(t.kind),
    byKindNumber: index("rv_by_kind_number").on(t.kind, t.numberValue),
    byKindString: index("rv_by_kind_string").on(t.kind, t.stringValue),
    byKindBool: index("rv_by_kind_bool").on(t.kind, t.boolValue),
  }),
);

export const runtimeValueObjectMembers = sqliteTable(
  "runtime_value_object_members",
  {
    parentValueId: integer("parent_value_id")
      .notNull()
      .references(() => runtimeValues.id, { onDelete: "cascade" }),
    key: text("key").notNull(),
    childValueId: integer("child_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.parentValueId, t.key] }),
    byChild: index("rvom_by_child").on(t.childValueId),
  }),
);

export const runtimeValueArrayElements = sqliteTable(
  "runtime_value_array_elements",
  {
    parentValueId: integer("parent_value_id")
      .notNull()
      .references(() => runtimeValues.id, { onDelete: "cascade" }),
    elementIndex: integer("element_index").notNull(),
    childValueId: integer("child_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.parentValueId, t.elementIndex] }),
    byChild: index("rvae_by_child").on(t.childValueId),
  }),
);
