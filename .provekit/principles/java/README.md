# java/ — Java principle partition

TBD: language-specific axioms for Java go here.

Per the CDD spec
(`docs/specs/2026-04-27-constraint-driven-development.md`,
"Language partitioning: principles per language"), Java sits in the
middle of the partition-size spectrum: stronger compile-time safety
than C/C++, weaker than Rust. Examples that may belong here when a
Java SAST is built:

- `NullPointerException` on possibly-null reference
- unchecked-cast `ClassCastException`
- `ConcurrentModificationException` on iterator while collection mutates
- resource not closed (try-with-resources missing)
- raw-type / generics-erasure pitfalls

These are NOT shipped today because the SAST extractor is TypeScript-only.
This directory is the structural slot they will land in.
