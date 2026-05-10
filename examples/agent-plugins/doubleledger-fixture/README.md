# Doubleledger fixture: the headline `provekit must` demo

This is the marketing demo: an English phrase ("not lose money") on a
small TypeScript double-entry ledger gets translated by the configured
agent into a verified ProvekIt contract memento.

```
$ provekit must src/doubleledger.ts "not lose money" --agent stub --json
{
  "ok": true,
  "minted_cid": "blake3-512:...",
  "name": "doubleledger_conservation",
  "rejected": 0,
  "agent_calls": 1,
  ...
}
```

The stub agent recognises the phrase "not lose money" and returns the
canonical conservation contract:

> forall txn. sumDebits(txn) == sumCredits(txn)

The validation gate parses the IR-JSON, mints a signed memento with
BLAKE3-512 + ed25519, and prints the CID.

## What this proves

The headline UX works end-to-end without any user-authored IR:

1. User points at a TypeScript file.
2. User says English.
3. Agent (stub here; real agents wire identically) translates.
4. ProvekIt validates the IR-JSON shape, mints, signs.

The acceptance test for this fixture lives at
`implementations/rust/provekit-agent/tests/doubleledger.rs`.

## Real agents

Replace `--agent stub` with `--agent claude-code` (after configuring
ANTHROPIC_API_KEY and the plugin manifest) or `--agent openai`. The
trait + JSON-RPC seam is identical; the stub is the canonical
deterministic CI path.
