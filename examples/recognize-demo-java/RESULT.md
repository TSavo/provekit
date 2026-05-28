# recognize-demo-java - Java recognize consumer

This Maven project is a small Java consumer for the recognize verb. It
uses user-authored JDBC helper bodies in `Persist.java` and asks
`provekit recognize` to tag those bodies against the sqlite-jdbc shim's
published sugar templates.

- SQL vendor: `sqlite-jdbc`
- User modules scanned by recognize: `Persist.java`, `Report.java`
- Recognized SQL boundaries: 3
- Second concept domain in user code: Jackson JSON deserialization in
  `Report.java`

Only the sqlite-jdbc Java SQL shim is present for this run. The demo does
not manufacture a second SQL vendor. `Report.java` uses Jackson to
deserialize the JSON payload column, but the recognize command below only
loads the sqlite-jdbc binding proof, so no JSON tags are expected or
emitted.

## Green path

The Maven tests exercise the helper methods, the JSON report seam, and
the full in-memory SQLite round trip:

```
$ MAVEN_OPTS='-Djansi.force=false' mvn -q test
SLF4J: Failed to load class "org.slf4j.impl.StaticLoggerBinder".
SLF4J: Defaulting to no-operation (NOP) logger implementation
SLF4J: See http://www.slf4j.org/codes.html#StaticLoggerBinder for further details.
WARNING: A restricted method in java.lang.System has been called
WARNING: java.lang.System::load has been called by org.sqlite.SQLiteJDBCLoader in an unnamed module (file:/Users/tsavo/.m2/repository/org/xerial/sqlite-jdbc/3.45.3.0/sqlite-jdbc-3.45.3.0.jar)
WARNING: Use --enable-native-access=ALL-UNNAMED to avoid a warning for callers in this module
WARNING: Restricted methods will be blocked in a future release unless native access is enabled
```

Surefire reports:

```
PersistTest: Tests run: 3, Failures: 0, Errors: 0, Skipped: 0
ReportTest:  Tests run: 1, Failures: 0, Errors: 0, Skipped: 0
E2ETest:     Tests run: 1, Failures: 0, Errors: 0, Skipped: 0
```

## Recognizer pilot

The checked-in sqlite-jdbc shim proof at
`examples/provekit-shim-java-sqlite-jdbc/blake3-512:506722bb5a51a9f7d0ef1eea0dab88d1a462a0623485577eda377ffebe734d8b38ed87327ffc9c61ce9724aa242abd676de61d96938fcb4b0ffcd5135d844267.proof`
does not contain `body_source.ast_template`, so `provekit recognize`
loads it as `bindings=0`. To keep the demo source-only on the kit side,
the demo vendors a freshly minted content-addressed binding proof from
the existing sqlite-jdbc shim source at:

```
examples/recognize-demo-java/.provekit/bindings/sqlite-jdbc/blake3-512:e263bdea525c0cf6664984c15eea0da47ef642bcbbb4082298cd60655c74a8617c806afc34cd4345797910a84ab099694dfb3763d7082e6ca609e5260d5be5d4.proof
```

The Java recognize surface is `java-bind`; the CLI default is still
`rust-bind`, so the command must pass `--surface java-bind --target java`.

```
$ provekit recognize \
    --surface java-bind \
    --target java \
    --project /Users/tsavo/provekit-demo-java/examples/recognize-demo-java \
    --source src/main/java/com/provekit/demo/recognize/Persist.java \
    --source src/main/java/com/provekit/demo/recognize/Report.java \
    --binding /Users/tsavo/provekit-demo-java/examples/recognize-demo-java/.provekit/bindings/sqlite-jdbc/blake3-512:e263bdea525c0cf6664984c15eea0da47ef642bcbbb4082298cd60655c74a8617c806afc34cd4345797910a84ab099694dfb3763d7082e6ca609e5260d5be5d4.proof

dispatch: surface=`java-bind` bindings=45 sources=2
recognize: 3 tag(s) emitted
  [0] concept:sql-connection-open @ src/main/java/com/provekit/demo/recognize/Persist.java:14 (fn=openConnection, exact)
  [1] concept:sql-execute @ src/main/java/com/provekit/demo/recognize/Persist.java:18 (fn=executeStatement, exact)
  [2] concept:sql-query-iterate @ src/main/java/com/provekit/demo/recognize/Persist.java:22 (fn=queryRows, exact)
```

`Persist.openConnection(String jdbcUrl)` alpha-matches the shim's
`open(String url)` body:

```
return DriverManager.getConnection(param1);
```

`Persist.executeStatement(PreparedStatement stmt)` matches the shim's
`stmtExecute(PreparedStatement stmt)` body:

```
return stmt.executeUpdate();
```

The connection-level shim method `execute(Connection conn, String sql)`
also exists, but its current Java Phase A template collapses
try-with-resources bodies to the same `TryStmt` shape as several other
sqlite helpers. Using the prepared-statement body avoids that duplicate
template collision and yields the intended `concept:sql-execute` tag.

`Persist.queryRows(Connection conn, String sql)` alpha-matches the shim's
`queryRow(Connection conn, String sql)` body:

```
Statement stmt = conn.createStatement();
return stmt.executeQuery(sql);
```

## Write path

`recognize --write` mints the demo-side bridges and implication
contracts into `.provekit/recognize/`:

```
$ provekit recognize \
    --write \
    --surface java-bind \
    --target java \
    --project /Users/tsavo/provekit-demo-java/examples/recognize-demo-java \
    --source src/main/java/com/provekit/demo/recognize/Persist.java \
    --source src/main/java/com/provekit/demo/recognize/Report.java \
    --binding /Users/tsavo/provekit-demo-java/examples/recognize-demo-java/.provekit/bindings/sqlite-jdbc/blake3-512:e263bdea525c0cf6664984c15eea0da47ef642bcbbb4082298cd60655c74a8617c806afc34cd4345797910a84ab099694dfb3763d7082e6ca609e5260d5be5d4.proof

dispatch: surface=`java-bind` bindings=45 sources=2
recognize: 3 tag(s) emitted
  [0] concept:sql-connection-open @ src/main/java/com/provekit/demo/recognize/Persist.java:14 (fn=openConnection, exact)
  [1] concept:sql-execute @ src/main/java/com/provekit/demo/recognize/Persist.java:18 (fn=executeStatement, exact)
  [2] concept:sql-query-iterate @ src/main/java/com/provekit/demo/recognize/Persist.java:22 (fn=queryRows, exact)
write: minted 3 bridge(s) + 3 implication contract(s) into /Users/tsavo/provekit-demo-java/examples/recognize-demo-java/.provekit/recognize/blake3-512:ac6cb928b55db7e63c17259607da7d6409ee0e4237fa182ec26bc529fee6b18b402ddfd0b9f6cf4f4c533ae0dead0c9c5a621c1f5715eec176da8466a7e9a5a2.proof
```

## Prove path

`provekit prove` sees the recognize-emitted contract anchors and bridges.
The sqlite binding proof used here publishes sugar templates but no shim
contract mementos, so the bridge target CID stays unset and the verifier
uses the back-compat post-only path. That is why the discharge reason is
vacuous rather than a shim-contract precondition proof.

```
$ provekit prove /Users/tsavo/provekit-demo-java/examples/recognize-demo-java
dependency proof resolver ["/usr/bin/env", "JAVA_HOME=/usr/local/opt/openjdk", "java", "-jar", "implementations/java/provekit-lift-java-core/target/provekit-lsp-java.jar", "--rpc"] does not implement provekit.plugin.resolve_dependency_proofs
warning: bridge openConnection has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge executeStatement has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
warning: bridge queryRows has no targetProofCid; ConsequentBundlePinned not enforced (back-compat path)
ProvekIt verifier report
  total callsites : 3
  discharged      : 3
  violations      : 0
  load errors     : 0

  [discharged] queryRows  (java -> sqlite-jdbc)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] openConnection  (java -> sqlite-jdbc)
      reason: vacuous: no precondition on target (publisher post-only)
  [discharged] executeStatement  (java -> sqlite-jdbc)
      reason: vacuous: no precondition on target (publisher post-only)
```
