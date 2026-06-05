# SPDX-License-Identifier: Apache-2.0

ROOT := ../..

.PHONY: all build test test-cpp-source clean

all: build

build:
	$(ROOT)/tools/build-cpp-lift.sh
	$(ROOT)/tools/build-cpp-source-lift.sh
	$(ROOT)/tools/build-cpp-lsp.sh

test: build test-cpp-source
	sh provekit-lsp-cpp/test_lsp.sh target/provekit-lsp-cpp

test-cpp-source:
	$(ROOT)/tools/test-cpp-source-lift.sh

clean:
	rm -rf target
