# Java Source Language Signature

Draft source-language operation algebra for `provekit-lift-java-source`.

The operation names are all `java:`-namespaced and describe Java source method
bodies lifted with the JDK compiler tree API. Version `0.1.0-draft` is an
honest partial slice: unsupported Java syntax is refused by the lifter rather
than mapped to an unknown operation.

The lifter emits a lossless `java:source-unit(bytes, operational_term)` wrapper
per source file. Loop bodies are marked with `opaque_loop` effects keyed by the
BLAKE3-512 CID of the lifted loop sub-term until a loop invariant memento is
available.
