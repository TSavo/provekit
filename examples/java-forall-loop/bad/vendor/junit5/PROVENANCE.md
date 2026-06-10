# Provenance: JUnit5 Assertions.java and delegate classes

Source files vendored for VocabDeriver throw-locus derivation (Phase 4.5).
All files from tag r5.10.2 of junit-team/junit5.

## Assertions.java (public API entry point)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/Assertions.java
- **Tag**: r5.10.2
- **sha256**: 536e6f91e8b2d5123c5e4441bf680ec6dd9df2ca7a0b2cfa01b9f84f53cd06a6
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: Public API class that delegates to package-private helper classes. The throw-locus deriver inlines through these delegations.

## AssertEquals.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertEquals.java
- **Tag**: r5.10.2
- **sha256**: 092762d66b2bb516ffe9fc2ba262a5c63eeb4283d16d42f1fff040ef1f8b4e19
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: Contains the actual assertEquals bodies with guard: `if (!objectsAreEqual(expected, actual)) failNotEqual(...)`. The EQUALITY classification derives from this guard.

## AssertNotEquals.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertNotEquals.java
- **Tag**: r5.10.2
- **sha256**: 8bcd51c68cc110f3ea83665dbe3572988143ae8ebfdfa79fbe469ac882f5cd3b
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: INEQUALITY classification guard: `if (unexpected == actual) failEqual(...)` for primitives.

## AssertTrue.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertTrue.java
- **Tag**: r5.10.2
- **sha256**: a3c29607bdac3f9c33eefb61a10717cc048ee8f8d1b2d7c9ae7d86591513ff69
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: TRUTH classification guard: `if (!condition) failNotTrue(...)`.

## AssertFalse.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertFalse.java
- **Tag**: r5.10.2
- **sha256**: cf35ac85aef6bd49f5f3bc5a784ff09dcbca55992b8a672d18f55b51bdefd779
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: NEGATED_TRUTH classification guard: `if (condition) failNotFalse(...)`.

## AssertNull.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertNull.java
- **Tag**: r5.10.2
- **sha256**: a0d7b89c950cd5b0527a5d5a6bff03765b119eeefbc71410ca79353a552bc554
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: NULL classification guard: `if (actual != null) failNotNull(...)`.

## AssertNotNull.java (package-private delegate)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertNotNull.java
- **Tag**: r5.10.2
- **sha256**: c35c0541707bdd0f1c75d5338e6cfca5ffb7dd22e6b85c9bcd7d5230220cecaf
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: NOT_NULL classification guard: `if (actual == null) failNull(...)`.

## AssertionUtils.java (package-private utilities)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertionUtils.java
- **Tag**: r5.10.2
- **sha256**: d34aafc5bb52a18162661ac2418f5256115c9e5cd324c9ca4180430f90fd7c63
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: Contains `objectsAreEqual` helper. The throw-locus deriver recognizes calls to this as equality-predicate sentinels.

## AssertionFailureBuilder.java (failure builder)

- **URL**: https://raw.githubusercontent.com/junit-team/junit5/r5.10.2/junit-jupiter-api/src/main/java/org/junit/jupiter/api/AssertionFailureBuilder.java
- **Tag**: r5.10.2
- **sha256**: 0376bca823fe0cc933a2d650e84469a72612c3f61443ce1f310e80a59f861120
- **License**: Eclipse Public License v2.0 (license header intact in the file)
- **Purpose**: Contains `buildAndThrow()` which is a recognized throw-locus terminal in the JUnit5 chain.
