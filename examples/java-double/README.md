# Java end-to-end example

This fixture registers Java lift and emit surfaces through project config only.
The CLI dispatches through `.sugar/config.toml` and per-surface manifests;
the Java kits own Java parsing, JUnit/TestNG syntax, and Maven checks.

`App.java` is the production proof fixture: `twice` has the real body and
`check` contains the plain Java assertion the Java lifter harvests.
