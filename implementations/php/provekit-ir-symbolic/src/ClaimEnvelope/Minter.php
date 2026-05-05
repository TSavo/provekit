<?php
/** ProvekIt — Claim envelope minter. */

namespace ProvekIt\ClaimEnvelope;

use ProvekIt\Canonicalizer\{Blake3, Ed25519, Jcs};
use ProvekIt\Ir\{ContractDecl, BridgeDecl, Sort, Collector};

class Minter
{
    private Ed25519 $signer;

    public function __construct(Ed25519 $signer)
    {
        $this->signer = $signer;
    }

    /** Mint a contract declaration into a signed claim envelope. */
    public function mintContract(
        ContractDecl $decl,
        string $producedBy,
        string $producedAt,
    ): array {
        // Compute property hash: BLAKE3(JCS({pre?, post?, inv?, outBinding}))
        $propObj = ['outBinding' => $decl->outBinding];
        if ($decl->pre !== null)  $propObj['pre'] = $decl->pre;
        if ($decl->post !== null) $propObj['post'] = $decl->post;
        if ($decl->inv !== null)  $propObj['inv'] = $decl->inv;
        $propHash = Blake3::cid(Jcs::encode($propObj));

        // Compute binding hash: BLAKE3(JCS({name, outBinding, pre?, post?, inv?}))
        $bindObj = ['name' => $decl->name, 'outBinding' => $decl->outBinding];
        if ($decl->pre !== null)  $bindObj['pre'] = $decl->pre;
        if ($decl->post !== null) $bindObj['post'] = $decl->post;
        if ($decl->inv !== null)  $bindObj['inv'] = $decl->inv;
        $bindHash = Blake3::cid(Jcs::encode($bindObj));

        // Content CID: BLAKE3(JCS(name + outBinding + pre/post/inv))
        $contentObj = ['name' => $decl->name, 'outBinding' => $decl->outBinding];
        if ($decl->pre !== null)  $contentObj['pre'] = $decl->pre;
        if ($decl->post !== null) $contentObj['post'] = $decl->post;
        if ($decl->inv !== null)  $contentObj['inv'] = $decl->inv;
        $contentCid = Blake3::cid(Jcs::encode($contentObj));

        // Build header
        $header = [
            'kind' => 'contract',
            'schema' => 'blake3-512:' . str_repeat('0', 128 + 2), // placeholder
            'cid' => $contentCid,
            'name' => $decl->name,
            'outBinding' => $decl->outBinding,
            'post' => $decl->post?->jsonSerialize(),
            'postHash' => Blake3::cid(Jcs::encode($decl->post?->jsonSerialize())),
            'bindingHash' => $bindHash,
            'propertyHash' => $propHash,
            'inputCids' => [],
            'verdict' => 'holds',
            'schemaVersion' => '1',
        ];
        if ($decl->pre !== null) {
            $header['pre'] = $decl->pre->jsonSerialize();
            $header['preHash'] = Blake3::cid(Jcs::encode($decl->pre->jsonSerialize()));
        }

        // Build envelope
        $env = [
            'signer' => 'ed25519:' . $this->signer->pubKeyBase64(),
            'declaredAt' => $producedAt,
        ];
        $sigPayload = Jcs::encode($env) . Jcs::encode($header);
        $env['signature'] = 'ed25519:' . $this->signer->signBase64($sigPayload);

        // Full memento
        $memento = [
            'envelope' => $env,
            'header' => $header,
            'evidence' => [
                'kind' => 'contract',
                'body' => [
                    'contractName' => $decl->name,
                    'outBinding' => $decl->outBinding,
                    'post' => $decl->post?->jsonSerialize(),
                    'postHash' => $header['postHash'],
                    'producerKind' => 'lift',
                    'lifter' => $producedBy,
                    'evidence' => 'types',
                ],
            ],
        ];
        if ($decl->pre !== null) {
            $memento['evidence']['body']['pre'] = $decl->pre->jsonSerialize();
            $memento['evidence']['body']['preHash'] = $header['preHash'];
        }

        $canonical = Jcs::encode($memento);

        return [
            'cid' => $contentCid,
            'envelopeCid' => Blake3::cid($canonical),
            'canonicalBytes' => $canonical,
        ];
    }

    /** Mint bridge declarations similarly */
    public function mintBridge(BridgeDecl $decl): array
    {
        $contentObj = [
            'name' => $decl->name,
            'sourceSymbol' => $decl->sourceSymbol,
            'sourceLayer' => $decl->sourceLayer,
            'sourceContractCid' => $decl->sourceContractCid,
            'targetContractCid' => $decl->targetContractCid,
            'targetProofCid' => $decl->targetProofCid,
            'targetLayer' => $decl->targetLayer,
        ];
        $cid = Blake3::cid(Jcs::encode($contentObj));

        return [
            'cid' => $cid,
            'canonicalBytes' => Jcs::encode($decl),
        ];
    }

    /**
     * Mint a v1.4 BridgeDeclaration (layered envelope/header/body,
     * tagged-union target). Per spec bridge-target-dimensionality.md §1.R1-R6.
     *
     * Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
     *
     * @param array $args with keys: name, sourceSymbol, sourceLayer,
     *   sourceContractCid, target (['kind'=>'contract'|'contractSet','cid'=>'...']),
     *   declaredAt, plus optional metadata fields.
     */
    public function mintBridgeV14(array $args): array
    {
        // Validate and normalize tagged-union target before signing (P1 #19).
        // Extra keys or missing kind/cid would produce a malformed header that
        // downstream verifiers reject. Reject early with a clear error.
        $target = $args['target'] ?? null;
        if (!is_array($target)) {
            throw new \InvalidArgumentException("mintBridgeV14: 'target' must be an array");
        }
        $validKinds = ['contract', 'contractSet'];
        $targetKind = $target['kind'] ?? null;
        if (!in_array($targetKind, $validKinds, true)) {
            throw new \InvalidArgumentException(
                "mintBridgeV14: target.kind must be one of [" . implode(', ', $validKinds) . "], got: " . json_encode($targetKind)
            );
        }
        if (empty($target['cid']) || !is_string($target['cid'])) {
            throw new \InvalidArgumentException("mintBridgeV14: target.cid must be a non-empty string");
        }
        // Emit only the canonical two fields — strip any extra keys.
        $normalizedTarget = ['cid' => $target['cid'], 'kind' => $targetKind];

        // Build header (7 canonical fields per §1.R3)
        $header = [
            'schemaVersion' => '1',
            'kind' => 'bridge',
            'name' => $args['name'],
            'sourceSymbol' => $args['sourceSymbol'],
            'sourceLayer' => $args['sourceLayer'],
            'sourceContractCid' => $args['sourceContractCid'],
            'target' => $normalizedTarget,
        ];

        // Build metadata (omit missing fields per §1.R2)
        $metaKeys = ['targetWitnessCid','targetBinaryCid','targetLayer',
                      'targetContractSetCid','producedBy','producedAt'];
        $meta = [];
        foreach ($metaKeys as $k) {
            if (!empty($args[$k])) $meta[$k] = $args[$k];
        }

        // Sign: JCS({header, metadata})
        $sigPayload = ['header' => $header, 'metadata' => $meta];
        $sigPayloadJcs = Jcs::encode($sigPayload);
        $sig = $this->signer->signBase64($sigPayloadJcs);

        // Build envelope
        $env = [
            'signer' => 'ed25519:' . $this->signer->pubKeyBase64(),
            'declaredAt' => $args['declaredAt'],
            'signature' => 'ed25519:' . $sig,
        ];

        // Full memento: {envelope, header, metadata}
        $memento = ['envelope' => $env, 'header' => $header, 'metadata' => $meta];
        $canonical = Jcs::encode($memento);

        return [
            'cid' => Blake3::cid($canonical),
            'canonicalBytes' => $canonical,
        ];
    }
}
