# ProvekIt — top-level orchestrator
#
# Five-language polyglot. Each language owns its native build tool;
# this Makefile is glue, not a build system. `make ci` runs the same
# gate the GitHub Actions workflow runs.
#
# Mainline targets:
#   make help        — print this help
#   make ci          — full conformance gate (catalog + protocol + 5 mints + tests)
#   make conformance — catalog + protocol + 5 mint CIDs match pinned values
#   make all-mint    — run all 5 mint commands; print CIDs
#   make test-all    — run every language-native test suite
#
# Per-language targets:
#   make build-rust  — cargo build --release for workspace + tools
#   make build-cpp   — vendored-blake3 clang++ build of the C++ orchestrator
#   make test-rust / test-go / test-ts / test-csharp / test-python
#
# Determinism:
#   make ci is the contract. If it's green, every peer's self-contracts
#   round-trip to its pinned CID, the catalog v1.2.0 hash matches, and
#   every native test suite passes. Anything else is decoration.

.DEFAULT_GOAL := help

# --- Pinned CIDs (Catalog v1.2.0) -------------------------------------------
# These are the source of truth. CI greps for these in mint output.
# To change a CID: change the source bytes, update both this Makefile and
# .github/workflows/ci.yml, and document the why in the commit message.
CATALOG_CID := blake3-512:1e5cfee6043d485d276c26a8da17830fe828c5b7b395a5fb1f042e7442407a37c39c59c0e002ca18857b12d3efb0d86687b9a3a0e3f6e3e933856f0717d0579f
RUST_CID    := blake3-512:b692f43a151f88aa31b998adaa091b2ac7ebad231c3c2b63426d93a8090de688bc8f12e02fe6ef901a513c4bf89dbffc884cd1164fa566fd1a757cf478434dfe
GO_CID      := blake3-512:906fa4f3ca32d97710e327c9e6e914e5c476a3cfdc326459b31dade24d9625c96f7f0595e3d91f316f73e2709a7f05ac79dd0ca768b6ff23cc2b384923487ac3
CPP_CID     := blake3-512:9335e6376d776819cfd3b2458da29bc258e7c2ebaad542a8613dd84f50c51c31d6e1a4346cea3903b8ad12294d96aef445d0ed838aa630835b9be0bc17e62842
TS_CID      := blake3-512:449339930add6457bf25542f2117a025daada4a4bd1de704737750ad6d1c1be814c284d31bb97159ca0b2d2c52f8c043a64533d3432195f5a0f338c5d4904d44
CSHARP_CID  := blake3-512:45d7cdbd0d5bfba5a1ee9e8386eb4d7dc1eab0882105753504a1f5c06de6f9fc4bd7038f56c7fcea693b152e2ab83de40ca4964a920816142ea43d5b9076415c

PROVEKIT := implementations/rust/target/release/provekit

.PHONY: help
help:
	@echo "ProvekIt — top-level orchestrator"
	@echo ""
	@echo "Mainline:"
	@echo "  make ci             full gate (conformance + test-all)"
	@echo "  make conformance    catalog + protocol + 5 mint CIDs match pinned"
	@echo "  make all-mint       run all 5 mint commands; print CIDs"
	@echo "  make test-all       run all language-native test suites"
	@echo ""
	@echo "Per-language build:"
	@echo "  make build-rust     cargo build --release (workspace + tools)"
	@echo "  make build-cpp      clang++ + vendored-blake3"
	@echo "  make build-go       go build per Go module"
	@echo "  make build-ts       pnpm install"
	@echo "  make build-csharp   dotnet build"
	@echo ""
	@echo "Per-language test:"
	@echo "  make test-rust  test-go  test-cpp  test-ts  test-csharp  test-python"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean          remove build artifacts"
	@echo ""
	@echo "Pinned CIDs (catalog v1.2.0):"
	@echo "  catalog: $(CATALOG_CID)"
	@echo "  rust:    $(RUST_CID)"
	@echo "  go:      $(GO_CID)"
	@echo "  cpp:     $(CPP_CID)"
	@echo "  ts:      $(TS_CID)"
	@echo "  csharp:  $(CSHARP_CID)"

# --- Per-language builds -----------------------------------------------------

.PHONY: build-rust
build-rust:
	cargo build --release --manifest-path implementations/rust/Cargo.toml
	cargo build --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo build --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: build-cpp
build-cpp:
	tools/build-cpp-self-contracts.sh --build-only

.PHONY: build-go
build-go:
	cd implementations/go/provekit-ir-symbolic && go build ./...
	cd implementations/go/provekit-self-contracts && go build ./...
	cd implementations/go/provekit-lift-go-tests && go build ./...

.PHONY: build-ts
build-ts:
	pnpm install --frozen-lockfile

.PHONY: build-csharp
build-csharp:
	dotnet build implementations/csharp/Provekit.sln --configuration Release --nologo

# --- Mint targets ------------------------------------------------------------

# Each mint target builds its peer + dispatches via `provekit mint`,
# then asserts the printed CID equals the pinned value. CI uses these.

.PHONY: mint-rust
mint-rust: build-rust
	@echo ">> minting rust self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/rust --quiet); \
	echo "  cid: $$out"; \
	test "$$out" = "$(RUST_CID)" || \
		(echo "FAIL: rust CID mismatch (expected $(RUST_CID))" && exit 1)

.PHONY: mint-go
mint-go: build-rust
	@echo ">> minting go self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/go --quiet); \
	echo "  cid: $$out"; \
	test "$$out" = "$(GO_CID)" || \
		(echo "FAIL: go CID mismatch (expected $(GO_CID))" && exit 1)

.PHONY: mint-cpp
mint-cpp: build-rust build-cpp
	@echo ">> minting cpp self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/cpp --quiet); \
	echo "  cid: $$out"; \
	test "$$out" = "$(CPP_CID)" || \
		(echo "FAIL: cpp CID mismatch (expected $(CPP_CID))" && exit 1)

.PHONY: mint-ts
mint-ts: build-ts
	@echo ">> minting ts self-contracts (vitest path)"
	pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts

.PHONY: mint-csharp
mint-csharp:
	@echo ">> minting csharp self-contracts"
	@out=$$(cd implementations/csharp/Provekit.SelfContracts && \
		dotnet run -c Release 2>/dev/null \
		| grep -F 'catalog CID:' | awk '{print $$NF}' | head -1); \
	echo "  cid: $$out"; \
	test "$$out" = "$(CSHARP_CID)" || \
		(echo "FAIL: csharp CID mismatch (expected $(CSHARP_CID))" && exit 1)

.PHONY: all-mint
all-mint: mint-rust mint-go mint-cpp mint-ts mint-csharp
	@echo ""
	@echo "==== all 5 self-contract CIDs match pinned values ===="
	@printf "  %-8s  %s\n" "rust"   "$(RUST_CID)"
	@printf "  %-8s  %s\n" "go"     "$(GO_CID)"
	@printf "  %-8s  %s\n" "cpp"    "$(CPP_CID)"
	@printf "  %-8s  %s\n" "ts"     "$(TS_CID)"
	@printf "  %-8s  %s\n" "csharp" "$(CSHARP_CID)"

# --- Conformance gate --------------------------------------------------------

.PHONY: catalog-verify
catalog-verify:
	cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify

.PHONY: protocol-verify
protocol-verify: build-rust
	$(PROVEKIT) verify-protocol --signed

.PHONY: conformance
conformance: catalog-verify protocol-verify all-mint
	@echo ""
	@echo "==== conformance: PASS ===="

# --- Per-language test suites ------------------------------------------------

.PHONY: test-rust
test-rust:
	cargo test --release --manifest-path implementations/rust/Cargo.toml
	cargo test --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo test --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: test-go
test-go:
	cd implementations/go/provekit-ir-symbolic && go test ./...
	cd implementations/go/provekit-self-contracts && go test ./...
	cd implementations/go/provekit-lift-go-tests && go test ./...

.PHONY: test-cpp
test-cpp: build-cpp
	@echo "test-cpp: cpp suite is the mint round-trip; covered by mint-cpp"

.PHONY: test-ts
test-ts:
	pnpm test

.PHONY: test-csharp
test-csharp:
	dotnet test implementations/csharp/Provekit.sln --nologo --verbosity quiet

.PHONY: test-python
test-python:
	cd implementations/python/provekit-lift-py-tests && \
		pip install --quiet -e . && \
		pip install --quiet pytest && \
		pytest

.PHONY: test-all
test-all: test-rust test-go test-ts test-csharp test-python
	@echo ""
	@echo "==== test-all: PASS ===="

# --- CI alias ----------------------------------------------------------------

.PHONY: ci
ci: conformance test-all
	@echo ""
	@echo "==== ci: PASS ===="

# --- Cleanup -----------------------------------------------------------------

.PHONY: clean
clean:
	cargo clean --manifest-path implementations/rust/Cargo.toml
	cargo clean --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo clean --manifest-path tools/foundation-keygen/Cargo.toml
	rm -rf implementations/cpp/target
	rm -rf implementations/csharp/Provekit.*/bin implementations/csharp/Provekit.*/obj
	rm -rf node_modules
	cd implementations/go/provekit-self-contracts && rm -f mint-go-self-contracts
	rm -f implementations/*/blake3-512:*.proof
	rm -f blake3-512:*.proof
