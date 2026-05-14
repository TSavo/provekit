const path = require("node:path");

const { createRealizer } = require("../../provekit-realize-typescript-core/src/realizer");

module.exports = createRealizer(
  path.join(
    "menagerie",
    "typescript-language-signature",
    "specs",
    "body-templates",
    "typescript-canonical-bodies-pg.json",
  ),
);
