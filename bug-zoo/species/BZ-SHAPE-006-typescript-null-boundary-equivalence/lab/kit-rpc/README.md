The lab state has no contract lift. It is ordinary TypeScript that compiles and
runs while the null-boundary bug remains latent.

There are two separate responsibilities:

- `pnpm exec tsx tools/ts-boundary-discover.ts <surface> <workspaceRoot>` runs
  the TypeScript implementation lifter and emits native discovery evidence for
  the null-boundary bug.
- The self-contained Bug Zoo runner invokes the lifter RPC, receives canonical
  Bug Zoo ProofIR, and verifies the IR CID against the checked-in witness bytes.
