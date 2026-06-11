# signup-service

A small user-signup intake. **It is a deliberately ordinary Maven project** —
the point is that it looks like a hundred thousand others, not like a demo.

```
src/main/java/com/example/signup/SignupRequest.java   @NotBlank / @Min payload
src/main/java/com/example/signup/SignupService.java    parse → validate → token → escape → audit
src/test/java/com/example/signup/SignupServiceTest.java  6 ordinary JUnit 5 tests
pom.xml                                                 the dependencies
prove.sh                                                mvn → sugar, prove everything
```

Every dependency is a real, recognizable library at a pinned version — gson,
hibernate-validator, commons-codec, commons-text, commons-lang3, commons-io,
junit. Run the tests the normal way:

```
mvn test       # Tests run: 6, Failures: 0
```

## Then: prove the whole supply chain

```
./prove.sh
```

It is Maven-driven, period. One goal — `mvn dependency:copy-dependencies
-Dclassifier=sources` — hands over **every** dependency's source the pom
resolves to (this project's 7 declared deps fan out to **19** transitive
artifacts). The loop mints one `.proof` per artifact, from that library's
*own* source and *own* tests. The vendor never did anything but publish; that
was the consent.

Two columns come out:

- `PROOF <artifact>` — a content-addressed proof of that dependency's sworn
  behavior, re-verifiable by anyone.
- `GAP <artifact> -> []` — the dependency lifts to the **empty set**: nothing in
  it was ever sworn to. That line is not a failure. It is the product: the
  signed shape of what no vendor ever warranted, twenty layers under your app.

The proven set is the ground you can stand on. The GAP lines are the crime
scene waiting to happen.
