# python/ — Python principle partition

TBD: language-specific axioms for Python go here.

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"), Python's dynamic
typing means a moderately large per-language axiom set. Examples that
may belong here when a Python SAST is built:

- `AttributeError` on possibly-None reference
- `KeyError` on dict access without `.get()` or `in` check
- `IndexError` on list access
- mutable default argument
- shadowed builtin (e.g. `list = ...`)
- bare `except:` swallowing system-exit signals

These are NOT shipped today because the SAST extractor is TypeScript-only.
This directory is the structural slot they will land in.
