const assert = require("node:assert/strict");
const test = require("node:test");

const { assembleResponse } = require("../src/assemble");

test("assembleResponse renders a TypeScript compilation unit from fragments", () => {
  const result = assembleResponse({
    file_basename: "users module",
    fragments: [
      {
        imports: ['import { Pool } from "pg"'],
        helpers: ["const TABLE = \"users\";"],
        source: "async function getUser(pool, id) {\n  return pool.query(\"select * from users where id = $1\", [id]);\n}\n",
      },
      {
        imports: ['import { Pool } from "pg"'],
        helpers: ["const TABLE = \"users\";"],
        source: "function rowCount(rows) {\n  return rows.length;\n}\n",
      },
    ],
  });

  assert.deepEqual(result.compile_classpath, []);
  assert.equal(result.files.length, 1);
  assert.equal(result.files[0].path, "users_module.ts");
  assert.equal(
    result.files[0].content,
    'import { Pool } from "pg";\n\nconst TABLE = "users";\n\nasync function getUser(pool, id) {\n  return pool.query("select * from users where id = $1", [id]);\n}\n\nfunction rowCount(rows) {\n  return rows.length;\n}\n',
  );
});

test("assembleResponse rejects non-object params", () => {
  assert.throws(() => assembleResponse(null), /params must be an object/);
});
