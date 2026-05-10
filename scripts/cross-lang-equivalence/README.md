# Cross-language equivalence regression

Substrate-identity gate. For every fixture, runs the IR-emission path in
TypeScript, Rust, Go, and C++; asserts byte-identical compact JSON
across all four kits; asserts SHA256 matches a locked golden value.

This is the load-bearing test for the architectural claim:
**same canonical IR → same propertyHash, regardless of host language.**

## What it catches

| Failure mode                                | Reported as |
|---------------------------------------------|-------------|
| One kit's output diverges from the others   | DIVERGE     |
| All kits agree but hash drifted from golden | DRIFT       |
| Kit binary crashes                          | crashed     |
| Toolchain missing                           | skipped     |

A DIVERGE is a per-kit bug: find which kit is producing the wrong
bytes and fix it. A DRIFT means a canonical-form change happened
intentionally and goldens.txt needs updating, OR an accidental drift
got into all kits at once (much rarer; investigate before updating).

## Running locally

```bash
./harness.sh
```

Builds Rust + C++ runners on first invocation; subsequent runs use
cached binaries.

The harness is also wired into the vitest suite at
`cross-language.test.ts`. It runs as part of `npm test` when all four
toolchains (npx, cargo, go, clang++) are present, and skips otherwise.

## Adding a fixture

1. Append the fixture name to `fixtures.txt`.
2. Add a `case` arm in each runner emitting the same logical claim:
   - `ts-runner.ts`
   - `rust-runner/src/main.rs`
   - `go-runner/main.go`
   - `cpp-runner/main.cpp`
3. Run `./harness.sh` once; it will print the new fixture's SHA256.
4. Add the SHA to `goldens.txt` to lock it.

## Toolchains

| Kit  | Compiler   | Source                                        |
|------|------------|-----------------------------------------------|
| TS   | tsx (Node) | `src/ir/symbolic/`                            |
| Rust | cargo      | `implementations/rust/provekit-ir-symbolic/`             |
| Go   | go         | `implementations/go/provekit-ir-symbolic/`               |
| C++  | clang++    | `implementations/cpp/provekit-ir-symbolic/include/`      |

The C++ kit is currently minimum-viable (header-only, only the types
required for the fixture set). Connectives, exists, bridge, parseInt,
the full primitive set, and the AST canonicalizer are tracked for
future work: not blockers on the architectural claim, since
substrate identity is already proven across four languages on the
fixtures that exist.
