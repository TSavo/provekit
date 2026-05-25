const { createRealizerFromShimProof } = require("../../provekit-realize-typescript-core/src/realizer");

// Pass THIS kit's module resolution paths so the shim is resolved from the
// better-sqlite3 kit's own node_modules (where it is a declared dependency),
// independent of the process cwd the substrate launches us from.
module.exports = createRealizerFromShimProof(
  "provekit-shim-better-sqlite3",
  "better-sqlite3",
  module.paths,
);
