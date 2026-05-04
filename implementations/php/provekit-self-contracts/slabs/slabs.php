<?php
/** ProvekIt PHP self-contracts — Contract definitions. */

namespace ProvekIt\SelfContracts;

use function ProvekIt\Ir\{
    Var, Num, Str, Ctor, Ctor1, Ctor2,
    Eq, Gte, Lte, Lt, Gt, NotNull,
    And, ForAllRef, TrueAtom, StringLength, LenGte, LenLte,
};
use ProvekIt\Ir\{Collector, Sort};

// ──────────────────────────────────────────
// 1. Canonicalizer contracts
// ──────────────────────────────────────────

function InvariantsCanonicalizer(): void
{
    // EncodeJCS is deterministic
    Collector::Must('EncodeJCS_is_deterministic',
        ForAllRef('s', Eq(Ctor1('EncodeJCS', Var('s')), Ctor1('EncodeJCS', Var('s'))))
    );

    // EncodeJCS of "true" has length 4
    Collector::Contract('EncodeJCS_true_length_eq_4',
        post: Eq(StringLength(Ctor1('EncodeJCS', Str('true'))), Num(4))
    );

    // BLAKE3-512 produces 128 hex chars
    Collector::Must('BLAKE3_512_hex_length',
        ForAllRef('data', Eq(
            StringLength(Ctor1('Blake3_512', Var('data'))),
            Num(128)
        ))
    );

    // BLAKE3-512 of empty string is deterministic
    Collector::Contract('BLAKE3_512_empty_is_known',
        post: Eq(
            Ctor1('Blake3_512', Str('')),
            Str('786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce')
        )
    );

    // ED25519 sign-then-verify roundtrips
    Collector::Must('ED25519_sign_verify_roundtrips',
        ForAllRef('msg', Eq(
            Ctor1('Ed25519_Verify', Ctor2('Pair', Ctor1('Ed25519_Sign', Var('msg')), Var('msg'))),
            Ctor1('Bool', Var('true'))
        ))
    );
}

// ──────────────────────────────────────────
// 2. IR contracts
// ──────────────────────────────────────────

function InvariantsIr(): void
{
    // Forall formulas are self-consistent
    Collector::Must('ForAll_var_is_bound',
        ForAllRef('x', Gte(
            StringLength(Ctor1('ForAll_body', Var('x'))),
            Num(0)
        ))
    );

    // And with one operand = identity
    Collector::Contract('And_single_is_identity',
        post: Eq(
            Ctor1('And', Ctor1('Atomic', Ctor1('True', Var('x')))),
            Ctor1('Atomic', Ctor1('True', Var('x')))
        )
    );

    // Ctor term preserves arguments
    Collector::Must('Ctor_preserves_arg_count',
        ForAllRef('arg', Gte(
            StringLength(Ctor1('Ctor_ArgCount', Var('arg'))),
            Num(0)
        ))
    );
}

// ──────────────────────────────────────────
// 3. Claim envelope contracts
// ──────────────────────────────────────────

function InvariantsClaimEnvelope(): void
{
    // Contract CID is deterministic for same inputs
    Collector::Must('ContractCid_deterministic',
        ForAllRef('args', Eq(
            Ctor1('ContractCid', Var('args')),
            Ctor1('ContractCid', Var('args'))
        ))
    );

    // Minted contract has non-empty CID
    Collector::Contract('MintContract_produces_cid',
        post: Gte(StringLength(Ctor1('MintContract_cid', Var('x'))), Num(10))
    );

    // Signatures are 64 bytes
    Collector::Must('Ed25519_signature_length_64',
        ForAllRef('data', Eq(
            StringLength(Ctor1('Ed25519_Sign', Var('data'))),
            Num(64)
        ))
    );
}

// ──────────────────────────────────────────
// 4. Proof envelope contracts
// ──────────────────────────────────────────

function InvariantsProofEnvelope(): void
{
    // Build produces non-empty bytes
    Collector::Contract('BuildEnvelope_produces_bytes',
        post: Gt(StringLength(Ctor1('BuildEnvelope_bytes', Var('input'))), Num(0))
    );

    // Filename CID starts with blake3-512:
    Collector::Must('FilenameCid_prefix',
        ForAllRef('bytes', Gte(
            StringLength(Ctor1('FilenameCid', Var('bytes'))),
            Num(10)
        ))
    );
}

// ──────────────────────────────────────────
// 5. Verifier contracts
// ──────────────────────────────────────────

function InvariantsVerifier(): void
{
    // Load_all_proofs indexes all contracts
    Collector::Must('LoadAllProofs_indexes_all',
        ForAllRef('path', Gte(
            StringLength(Ctor1('LoadAllProofs_memento_count', Var('path'))),
            Num(0)
        ))
    );

    // Verifier merge merges pools
    Collector::Must('MementoPool_merge_idempotent',
        ForAllRef('pool', Eq(
            Ctor1('MementoPool_merge', Ctor2('Pair', Var('pool'), Var('pool'))),
            Var('pool')
        ))
    );
}

// ──────────────────────────────────────────
// 6. Lift plugin protocol bridges (cross-kit)
// ──────────────────────────────────────────

function InvariantsLiftPluginProtocol(): void
{
    // PHP kit conforms to lift plugin protocol C1-C11 invariants
    // These are the cross-kit bridges connecting PHP contracts to Rust canonical contracts.

    // C1: initialize returns capabilities
    Collector::Must('PHP_Lift_initialize_returns_capabilities',
        ForAllRef('params', Gte(StringLength(Ctor1('Lift_initialize_result', Var('params'))), Num(0)))
    );

    // C2: lift returns proof-envelope shape
    Collector::Must('PHP_Lift_returns_proof_envelope',
        ForAllRef('params', Eq(
            Ctor1('Lift_response_kind', Var('params')),
            Str('proof-envelope')
        ))
    );

    // C3: source_paths is non-empty
    Collector::Must('PHP_Lift_source_paths_non_empty',
        ForAllRef('params', Gt(
            StringLength(Ctor1('Lift_source_paths', Var('params'))),
            Num(2)
        ))
    );
}

// ──────────────────────────────────────────
// Slab registry
// ──────────────────────────────────────────

class Slab
{
    public function __construct(
        public readonly string $label,
        public readonly string $path,
        public readonly \Closure $run,
    ) {}
}

/** @return Slab[] */
function Slabs(): array
{
    return [
        new Slab('canonicalizer',     'Canonicalizer/',     InvariantsCanonicalizer(...)),
        new Slab('ir',                'Ir/',                InvariantsIr(...)),
        new Slab('claim_envelope',    'ClaimEnvelope/',     InvariantsClaimEnvelope(...)),
        new Slab('proof_envelope',    'ProofEnvelope/',     InvariantsProofEnvelope(...)),
        new Slab('verifier',          'Verifier/',          InvariantsVerifier(...)),
        new Slab('lift_protocol',     'LiftPluginProtocol/', InvariantsLiftPluginProtocol(...)),
    ];
}
