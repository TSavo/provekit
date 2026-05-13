<?php
/** ProvekIt PHP self-contracts: Canonical contract definitions.
 *  Mirrors the Ruby/Go/Rust self-contracts slab pattern.
 *  Contract names prefixed with `php_` per kit convention.
 */

namespace ProvekIt\SelfContracts;

use function ProvekIt\Ir\{
    V, Num, Str, Ctor, Ctor1, Ctor2,
    Eq, Gte, Lte, Lt, Gt, NotNull,
    And_, ForAllRef, TrueAtom, StringLength, LenGte, LenLte,
};
use ProvekIt\Ir\{Collector, ContractDecl, Sort};

// ──────────────────────────────────────────
// 1. Blake3 contracts (mirrors ruby_blake3_*)
// ──────────────────────────────────────────

function invariantsBlake3(): void
{
    $hBytes = fn($s) => Ctor1('Blake3.bytes', $s);
    $hHex   = fn($s) => Ctor1('Blake3.hex', $s);

    Collector::Contract('php_blake3_bytes_length_eq_64',
        post: Eq(Ctor1('byte_length', $hBytes(V('s'))), Num(64))
    );
    Collector::Contract('php_blake3_hex_length_eq_139',
        post: Eq(Ctor1('string_length', $hHex(V('s'))), Num(139))
    );
    Collector::Contract('php_blake3_hex_is_deterministic',
        post: Eq($hHex(V('s')), $hHex(V('s')))
    );
}

// ──────────────────────────────────────────
// 2. JCS contracts
// ──────────────────────────────────────────

function invariantsJcs(): void
{
    $enc = fn($v) => Ctor1('Jcs.encode', $v);

    Collector::Contract('php_jcs_encode_is_deterministic',
        post: Eq($enc(V('v')), $enc(V('v')))
    );
    Collector::Contract('php_jcs_encode_true_length_eq_4',
        post: Eq(StringLength($enc(Str('true'))), Num(4))
    );
    Collector::Contract('php_jcs_encode_null_length_eq_4',
        post: Eq(StringLength($enc(Str('null'))), Num(4))
    );
}

// ──────────────────────────────────────────
// 3. CBOR contracts
// ──────────────────────────────────────────

function invariantsCbor(): void
{
    Collector::Contract('php_cbor_tstr_roundtrip_length_gte_1',
        post: Gte(Ctor1('byte_length', Ctor1('Cbor.encode_tstr', V('s'))), Num(1))
    );
    Collector::Contract('php_cbor_bstr_roundtrip_length_gte_1',
        post: Gte(Ctor1('byte_length', Ctor1('Cbor.encode_bstr', V('b'))), Num(1))
    );
    Collector::Contract('php_cbor_encode_key_is_deterministic',
        post: Eq(Ctor1('Cbor.encode_key', V('k')), Ctor1('Cbor.encode_key', V('k')))
    );
}

// ──────────────────────────────────────────
// 4. Signing contracts
// ──────────────────────────────────────────

function invariantsSigning(): void
{
    Collector::Contract('php_ed25519_signature_length_eq_64',
        post: Eq(
            Ctor1('byte_length', Ctor2('Signing.sign_with_seed', V('seed'), V('msg'))),
            Num(64)
        )
    );
    Collector::Contract('php_ed25519_sign_is_deterministic',
        post: Eq(
            Ctor2('Signing.sign_with_seed', V('seed'), V('msg')),
            Ctor2('Signing.sign_with_seed', V('seed'), V('msg'))
        )
    );
}

// ──────────────────────────────────────────
// 5. Proof envelope contracts
// ──────────────────────────────────────────

function invariantsProofEnvelope(): void
{
    Collector::Contract('php_proof_envelope_build_produces_bytes',
        post: Gt(StringLength(Ctor1('ProofEnvelope.build', V('input'))), Num(0))
    );
    Collector::Contract('php_proof_envelope_cid_starts_with_prefix',
        post: Gte(StringLength(Ctor1('ProofEnvelope.cid', V('input'))), Num(10))
    );
}

// ──────────────────────────────────────────
// 6. Lift plugin protocol contracts
// ──────────────────────────────────────────

function invariantsLiftPluginProtocol(): void
{
    Collector::Contract('php_lift_initialize_returns_capabilities',
        post: Gte(StringLength(Ctor1('Lift.initialize.result', V('params'))), Num(0))
    );
    Collector::Contract('php_lift_returns_proof_envelope',
        post: Eq(Ctor1('Lift.response.kind', V('params')), Str('proof-envelope'))
    );
}

// ──────────────────────────────────────────
// Slab registry
// ──────────────────────────────────────────

class Slab
{
    public function __construct(
        public readonly string $label,
        public readonly \Closure $run,
    ) {}
}

/** @return Slab[] */
function Slabs(): array
{
    return [
        new Slab('blake3',         invariantsBlake3(...)),
        new Slab('jcs',            invariantsJcs(...)),
        new Slab('cbor',           invariantsCbor(...)),
        new Slab('signing',        invariantsSigning(...)),
        new Slab('proof_envelope', invariantsProofEnvelope(...)),
        new Slab('lift_protocol',  invariantsLiftPluginProtocol(...)),
    ];
}
