
import { bridge } from "/Users/tsavo/provekit/.claude/worktrees/formulate-via-lifter/src/ir/symbolic/index.js";

bridge("parseIntBridgesV8", {
  sourceSymbol: "global.parseInt",
  sourceLayer: "ts-kit@1.0",
  targetContractCid: "deadbeef".repeat(4),
  targetLayer: "V8@12.4 parseInt",
});
