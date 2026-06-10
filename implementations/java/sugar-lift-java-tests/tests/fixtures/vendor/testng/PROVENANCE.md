## Provenance: TestNG Assert.java

Source file vendored for VocabDeriver source-learning (Phase 4).

- **URL**: https://raw.githubusercontent.com/testng-team/testng/7.10.2/testng-asserts/src/main/java/org/testng/Assert.java
- **Tag**: 7.10.2
- **sha256**: 7d4dfe6017459e3035e2589b932aa84204e854c4dcbd1d17d79c1d01c06bbc19
- **License**: Apache License 2.0 (license header intact in the file)
- **Purpose**: The VocabDeriver parses this file via JavacTask.parse() to learn the TestNG assertion vocabulary. The key fact learned: TestNG assertEquals(actual, expected) places the actual value FIRST and the expected value SECOND — the reverse of JUnit. This is THE standing proof that assertion vocabulary must be learned per-framework from its own source, never hardcoded: hardcode JUnit order and TestNG lifts every assertion backwards. The VocabDeriver reads parameter NAMES from this source ("actual" is param[0], "expected" is param[1]) and records expectedArgIndex=1 for TestNG assertEquals.
