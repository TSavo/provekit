# provekit-emit-python-unittest

PEP 1.7.0 Python emitter kit for native `unittest` assertions.

The kit accepts normalized proof predicate data over the ProveKit plugin RPC
protocol and emits a standalone Python `unittest.TestCase` module. Predicate
spelling is Python framework knowledge and stays in this Python package; the
Rust CLI only dispatches through `.provekit/config.toml` and the emit manifest.

