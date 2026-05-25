const { createRealizerFromShimProof } = require("../../provekit-realize-typescript-core/src/realizer");

// Pass THIS kit's module resolution paths so the shim is resolved from the
// pg kit's own node_modules (where it is a declared dependency), independent
// of the process cwd the substrate launches us from. The flat
// typescript-canonical-bodies-pg.json was deleted (#1468): the signed shim
// `.proof` is the sole source of emission templates, mirroring the
// better-sqlite3 kit.
module.exports = createRealizerFromShimProof(
  "provekit-shim-pg",
  "pg",
  module.paths,
);
