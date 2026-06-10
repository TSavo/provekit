# Vendored source provenance

These files are unmodified copies of Apache commons-codec source, pinned to a
release tag. License headers (Apache License 2.0, ASF) are intact in every file.
They are vendored so the AST walk runs against a content-addressed snapshot, not
a moving HEAD.

- **Repository:** apache/commons-codec
- **Tag:** `rel/commons-codec-1.16.1`
- **Fetched:** 2026-06-10 via `curl` from `raw.githubusercontent.com`

| File | Source path (in repo) | sha256 |
|------|-----------------------|--------|
| `Base64.java` | `src/main/java/org/apache/commons/codec/binary/Base64.java` | `d6e02dcc3b277f5f366724b1b2d74fda3cff1db37ca8ca709db60cd3adee0fdf` |
| `BaseNCodec.java` | `src/main/java/org/apache/commons/codec/binary/BaseNCodec.java` | `930594ae7da6cb20595c4af0f69c7be938a20d089d265bae9d983da496a84e35` |
| `Base64Test.java` | `src/test/java/org/apache/commons/codec/binary/Base64Test.java` | `ef97352ff2460ff416ae5850dfbb38fc36064c7ac1f16fba6f14fe224ebb1604` |

## Exact fetch URLs

```
https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/Base64.java
https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/main/java/org/apache/commons/codec/binary/BaseNCodec.java
https://raw.githubusercontent.com/apache/commons-codec/rel/commons-codec-1.16.1/src/test/java/org/apache/commons/codec/binary/Base64Test.java
```

## Re-fetch / verify

```sh
TAG="rel/commons-codec-1.16.1"
BASE="https://raw.githubusercontent.com/apache/commons-codec/${TAG}/src"
curl -fsSL "${BASE}/main/java/org/apache/commons/codec/binary/Base64.java"     -o Base64.java
curl -fsSL "${BASE}/main/java/org/apache/commons/codec/binary/BaseNCodec.java" -o BaseNCodec.java
curl -fsSL "${BASE}/test/java/org/apache/commons/codec/binary/Base64Test.java" -o Base64Test.java
shasum -a 256 Base64.java BaseNCodec.java Base64Test.java   # must match the table above
```

## The vendor vector used by the experiment

`Base64Test.java`, line 878 (RFC 4648 §10 test vector "foo"):

```java
assertEquals("Zm9v", Base64.encodeBase64String(StringUtils.getBytesUtf8("foo")));
```

This is the single point sample whose constraints the walk generalizes and the
solver re-derives.
