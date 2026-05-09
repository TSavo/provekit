# SPDX-License-Identifier: Apache-2.0

.PHONY: all test clean

all: test

test:
	$(MAKE) -C ../provekit-lift-c-sparse all
	$(MAKE) -C ../provekit-lift-c-assertions all
	@chmod +x tests/integration.sh
	@tests/integration.sh

clean:
	$(MAKE) -C ../provekit-lift-c-sparse clean
	$(MAKE) -C ../provekit-lift-c-assertions clean
