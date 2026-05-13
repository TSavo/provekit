# JVM Bytecode Language Signature

This menagerie entry names the JVM bytecode primitive surface used by
`provekit-lift-jvm-bytecode`.

The signature is bytecode-domain first: instruction streams are positional,
branch labels are identifiers, local slots are address-like identifiers, and no
Java source syntax appears in the primitive vocabulary. Java, Kotlin, Scala,
and other JVM languages can target this same surface by compiling to bytecode
and linking by path.

The initial executable slice covers the deterministic Jasmin subset emitted by
`provekit-ir-compiler-jvm-bytecode`: integer locals, integer constants,
integer arithmetic, conditional branches, `goto`, `ireturn`, static memory
helpers, and `invokestatic` call edges.
