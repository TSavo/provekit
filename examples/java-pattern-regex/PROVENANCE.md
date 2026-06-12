# Vendored source provenance — java-pattern-regex (Door 3: the @Pattern regex universe)

The kit walks a JSR-380 `@Pattern(regexp="…")` annotation literal from the
vendor's source AST into z3's native regular-language theory, then refutes a
consumer's validity claim about an input the pattern rejects. Every constraint
traces to an AST node; nothing is hand-authored.

## Bean Validation (JSR-380) — `jakarta.validation:jakarta.validation-api`, tag `3.0.2`

Source: https://github.com/jakartaee/validation, tag `3.0.2`. License: Apache-2.0.

The `@Pattern` constraint annotation declaration is vendored as a stub carrying
only the `regexp` element the kit reads:

| File (under `good/vendor/jakarta/` and `bad/vendor/jakarta/`) | Upstream path |
|---|---|
| `validation/constraints/Pattern.java` | `src/main/java/jakarta/validation/constraints/Pattern.java` |

Upstream raw URL:
- https://github.com/jakartaee/validation/blob/3.0.2/src/main/java/jakarta/validation/constraints/Pattern.java

The annotation is **parsed, never executed** — the kit reads the
`@Pattern(regexp="…")` string literal from the annotation's `AnnotationTree`
AST node (`AssignmentTree("regexp") → LiteralTree<String>`). The stub carries
the same `regexp()` element signature as upstream; the `message`/`groups`/
`payload`/`flags` elements are immaterial to the walk.

## The walked annotation (the vendor's oath)

`good/vendor/account/UserHandle.java` and `bad/vendor/account/UserHandle.java`
carry the `@Pattern`-annotated, String-returning validating accessor:

```java
@Pattern(regexp = "^[a-z][a-z0-9_]{2,15}$")
public static String accept(String handle) { return handle; }
```

The regex literal `^[a-z][a-z0-9_]{2,15}$` is walked **verbatim** from this
annotation's AST. The author's stated intent (in the source comment) is "a
lowercase handle: starts with a letter, then 2 to 15 of letter / digit /
underscore." The walk lowers the literal to the regular language it actually
denotes — and z3 decides membership over THAT language, not the author's prose.

## The regex walked, and the FOL it lowers to

`^[a-z][a-z0-9_]{2,15}$` lowers (Rust `regex_regln`) to z3 RegLan:

- `^` / `$` anchors → identity (z3 `str.in_re` is already whole-string membership)
- `[a-z]` → `(re.range "a" "z")`
- `[a-z0-9_]` → `(re.union (re.range "a" "z") (re.union (re.range "0" "9") (str.to_re "_")))`
- `{2,15}` → `((_ re.loop 2 15) …)`
- concatenation → `(re.++ …)`

emitted as `(str.in_re <accept-callresult> (re.++ (re.range "a" "z") ((_ re.loop 2 15) …)))`.

## GOOD / BAD (the gate)

- **GOOD** (`PatternRegexGoodTest`): `assertEquals("alice_01", accept("alice_01"))`.
  `"alice_01"` ∈ L(@Pattern) — an 'a'-led 8-char handle of `[a-z0-9_]`. The
  equality and the `str.in-regex` row conjoin under the same `#euf#` name:
  SAT → **discharged**.
- **BAD** (`PatternRegexBadTest`): `assertEquals("Alice!", accept("Alice!"))`.
  `"Alice!"` ∉ L(@Pattern) — it leads with uppercase `A` (the language requires
  a lowercase letter) and ends with `!` (not in the `[a-z0-9_]` body class). The
  `str.in-regex` row conjoins with the equality: UNSAT → **unsatisfied**. The
  refutation is **membership-driven** (the walked regular language), NOT a
  within-test contradiction: the equality `=(accept("Alice!"), "Alice!")` is
  satisfiable in isolation; the regex membership is the sole source of UNSAT.

## THE SPOTLIGHT — validation's false confidence, named

`@Pattern` is the densest false-confidence dark: "the framework validates it"
feels safe while unaccounted behaviour sits right next to it. Two concrete
darks this universe makes mechanical:

1. **The unescaped-dot dark.** An author who writes `@Pattern(regexp="user.admin")`
   believing it pins the literal `"user.admin"` has written a language that ALSO
   accepts `"userXadmin"`, `"user'admin"`, `"user@admin"` — because `.` is the
   any-char metacharacter, not a literal dot. The walked language is strictly
   wider than the intuition; a claim that `"user'admin"` is valid is SAT, and
   that SAT is the proof the pattern is more permissive than its author thinks.
   (The Rust unit test `in_regex_spotlight_accepts_more_than_intuited` demonstrates
   the sibling permissive-email dark mechanically.)

2. **The "alphanumerics only" dark.** The handle pattern here
   `^[a-z][a-z0-9_]{2,15}$` is commonly read as "safe identifier characters." It
   silently ADMITS a leading reserved word and arbitrary underscore runs
   (`admin_root`, `a__________x`), and silently REJECTS any uppercase or
   punctuation — including the `"Alice!"` the BAD suite refutes. The membrane
   between "what the regex accepts" and "what the author believes it accepts" is
   exactly what the walked universe pins.

## Honest scope — which regex features render, which REFUSE BY NAME

The walk renders only the **regular** subset; every other feature is **refused by
name** (named diagnostic, no row emitted — the language is never approximated):

| Rendered (regular) | Lowering |
|---|---|
| literals, escaped metachars `\. \+ …` | `str.to_re` |
| char classes `[a-z] [abc] [^…]` | `re.range` / `re.union` / `re.comp` |
| dot `.` (non-newline) | `(re.comp (str.to_re "\n"))` |
| `* + ?` | `re.* re.+ re.opt` |
| `{n} {n,} {n,m}` | `(_ re.loop …)` |
| alternation `a\|b` | `re.union` |
| groups `(…) (?:…)` | grouping |
| anchors `^ $` | identity (whole-string membership) |
| predefined `\d \D \w \W \s \S` | `re.range` / `re.union` / `re.comp` |

| REFUSED BY NAME (not a regular language) |
|---|
| backreferences `\1 … \k<name>` |
| lookahead / lookbehind `(?= (?! (?<= (?<!` |
| atomic group `(?>…)`, possessive `*+`, reluctant `*?` |
| named-capture `(?<name>…) (?P…)`, inline flags `(?i)…` |
| word/input anchors `\b \B \A \Z \z \G` |

A refused pattern leaves the weak floor standing: no `str.in-regex` row, a named
diagnostic, silent=0 structural.

## JUnit framework source (assertion vocabulary)

`good/vendor/junit5/` and `bad/vendor/junit5/` are copied from
`examples/java-assertion-consistency/*/vendor/junit5/` — see the `PROVENANCE.md`
inside those directories (junit5 tag `r5.10.2`).
