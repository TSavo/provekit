# Behavioral versioning: what `sugar diff` is actually doing

`sugar diff` compares two minted proof sets and reports what changed in terms of
*meaning*, not text. This document is the argument for why that small tool
inverts something the whole ecosystem has had backwards.

## SemVer versions the shadow

Under SemVer, the unit of versioning is the code. A code change demands a version
bump. A change to a unit test cannot affect the version at all.

But nobody depends on your code. They depend on what your code *does*. SemVer
cannot see "what it does," so it uses "did the bytes change" as a stand-in, and
that proxy is wrong in both directions at once:

- **Too jumpy.** A rename, a reformat, a refactor that preserves behavior still
  changes the bytes, so it pressures a bump for a change no consumer can observe.
- **Stone blind.** It cannot tell a bugfix from a backdoor. Both are "a patch."
  The version number carries no information about whether the behavior moved,
  which is the one thing a consumer needs to know.

The entire dependency graph has been versioning the object's shadow, because the
shadow (code text) was the only thing it could see on the wall.

## Sugar versions the object

A contract is a statement about behavior: a precondition and a postcondition over
observable state. Lift any source and you get its contracts; content-address each
one and you get a CID that is the name-stripped identity of that behavior. The
`.proof` is the sealed bundle of those discharged contracts.

`sugar diff` inverts each proof set to `CID -> {names}` and classifies by
behavior, not by name:

| class     | meaning                                              |
|-----------|------------------------------------------------------|
| `held`    | a behavior present both sides under the same name(s) |
| `renamed` | a behavior present both sides, name(s) changed       |
| `new`     | a behavior only in AFTER (additive)                  |
| `lost`    | a behavior only in BEFORE (breaking)                 |

The verdict follows the CID set. A behavior that vanishes is `MAJOR`. A behavior
that appears with nothing lost is `minor`. A rename, a reformat, a refactor that
keeps every behavior-CID stable is `none`. Names are sugar; only a behavior
appearing or disappearing moves the needle.

## The test is the thing of record

The instant you can see behavior as a content-addressed contract, the hierarchy
flips. The test stops being the tax you pay to ship the code and becomes the
thing of record, because the test is the only artifact that ever stated what the
behavior was supposed to be. A test *is* a contract: an asserted behavior plus a
witness that it holds.

So:

- Change the code, the witness still passes. You changed nothing anyone can
  depend on. No version move.
- Change what the test asserts. You changed the promise. The pin moves.

"We pin on the changed unit test" is just that, read out loud. The assertion is
the spec, the spec is what you version, the code is downstream of it.

This is TDD all the way up. TDD says write the test first; the code exists to
satisfy it. SemVer throws that ordering away and versions the code anyway. Sugar
restores the ordering, but at the release and supply-chain layer instead of the
editor: contract primary, implementation a swappable satisfier, version tracking
the contract.

And it is not a metaphor that a `.proof` is a recorded passing test. The witness
axis discharges a contract by *recomputing* it and watching the result. The proof
and the test were never two different things. SemVer simply could not see the
test, so it pinned the code and filed the test under metadata.

## One primitive, three hats

The same comparison, pointed at different pairs, is three tools:

```
sugar diff <a> <b>                              behavior delta + trichotomy + bump
sugar diff --git HEAD~1 HEAD --path <proj>      behavioral version control
sugar diff --git <last-tag> HEAD --require minor   honest semver (pre-publish)
sugar diff --frozen <pinned> <installed>        supply-chain pin (install-time)
```

- **Behavioral VCS.** git tracks the bytes you typed; this tracks what your code
  does. A commit that renamed or reformatted reads as `held`/`renamed`. Only a
  commit where a behavior-CID appeared or vanished shows `new`/`lost`. You bisect
  a regression to the commit where behavior actually moved and skip the rest.

- **Honest semver.** `--require <bump>` fails unless the behavior delta fits the
  claimed bump. The version stops being a promise a human types and becomes a
  measurement: the required bump is computed from the proof delta. A release that
  calls itself `minor` while a behavior was lost is refused at publish time.

- **Supply-chain pin.** `--frozen` fails on any behavior delta. A pinned
  dependency must denote byte-identical behavior; `new`, `lost`, or `renamed` all
  mean it mutated under a fixed version. This is the defense against a malicious
  patch shipped under a benign version number: the version stayed `1.2.4`, but
  the behavior-CID moved, and `--frozen` sees the thing the version number hid.

The diff is a few hundred lines because the substrate already did the work; the
weight was paid at mint time, baked into the CID. By the time you are comparing,
the semantics already happened. You are just reading.
