# go/: Go principle partition

TBD: language-specific axioms for Go go here.

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"), Go's per-language
set is small-to-moderate. Examples that may belong here when a Go SAST
is built:

- `error` returned but not checked (`_, err := ...; ignored err`)
- nil-map write panic
- nil-pointer dereference on possibly-nil receiver
- goroutine leak (no termination path)
- `defer` in loop without scoping helper

These are NOT shipped today because the SAST extractor is TypeScript-only.
This directory is the structural slot they will land in.
