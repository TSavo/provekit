# ref-email-format-v1

The canonical reference contract for RFC 5322 email format validation.

> **Status:** stub. Canonical IR not yet committed to `protocol/reference-contracts/`.

## CID

`blake3-512:bafy...ref-email-format-v1` *(placeholder; computed from committed canonical IR)*

## What it claims

> A string `s` matches the email format predicate if and only if `s` matches the simplified RFC 5322 email pattern: a local part, an `@` symbol, and a domain part with at least one dot.

The canonical regex (used in the canonical IR's `matches_pattern` predicate):

```
^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$
```

This is a conservative simplification. Full RFC 5322 (and RFC 5321 for the SMTP envelope, RFC 6531 for internationalized email) is much more permissive but rarely actually used in practice. The reference captures "what most validators want."

## Canonical IR (sketch)

```
{
  "kind": "contract",
  "name": "ref-email-format-v1",
  "version": 1,
  "params": [
    {"name": "s", "sort": {"kind": "primitive", "name": "String"}}
  ],
  "post": {
    "kind": "atomic",
    "predicate": "matches_pattern",
    "args": [
      {"kind": "var", "name": "s"},
      {"kind": "const", "type": "String", "value": "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"}
    ]
  }
}
```

## Implementations that bridge to this reference

- [TypeScript] zod's `z.string().email()`: *bridge planned in `provekit-lift-zod` v0.4*
- [Python] pydantic's `EmailStr`: *bridge planned in `provekit-lift-pydantic` v0.5*
- [Java] Bean Validation's `@Email`: *bridge planned in `provekit-lift-java-bean-validation`*
- [C#] `[EmailAddress]` data annotation: *bridge planned in `Provekit.Lift.DataAnnotations`*
- [Ruby] active_model `validates :email, format: { with: URI::MailTo::EMAIL_REGEXP }`: *bridge planned in `provekit-lift-active_model`*

When all five bridges ship, an email validation in any of these languages discharges at Tier 1 against an email validation in any other.

## Limitations

- **Conservative.** The simplified regex rejects valid RFC 5322 addresses (e.g., quoted local parts, IP-literal domains). Implementations that accept these need a separate reference.
- **Internationalization.** No EAI (RFC 6531) support. Domains with non-ASCII characters require punycode encoding before validation against this reference.
- **Display name handling.** This is the address-only validation. Full email headers like `"User Name" <user@example.com>` require a separate reference (`ref-email-rfc5322-v1`, proposed).
- **Length.** No specific length bound. Implementations may impose their own (typically 254 chars per RFC 5321).

For the most precise contract for a specific implementation, use a per-implementation contract memento alongside the bridge.

## Why this reference exists

Email validation is one of the most common patterns in any input-validation surface. Every web framework, every form library, every schema library has it. The reference captures the most-common case: "looks like an email." More precise references for specific use cases (full RFC compliance, locale, IDN) can be added separately.

## Related references

- `ref-email-rfc5322-v1` (proposed): full RFC 5322 with display name support.
- `ref-email-eai-v1` (proposed): internationalized email (RFC 6531).
- `ref-url-format-v1` (proposed): URL validation.

## Read next

- [`README.md`](README.md).
- [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md).
- [`../contributing/writing-a-lift-adapter/03-emit-canonical-IR.md`](../contributing/writing-a-lift-adapter/03-emit-canonical-IR.md): how an adapter produces the bridge to this reference.
