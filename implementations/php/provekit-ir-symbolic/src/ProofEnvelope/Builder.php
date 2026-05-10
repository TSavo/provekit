<?php
/** ProvekIt: Deterministic CBOR catalog builder. Simplified for hand-rolled CBOR. */

namespace ProvekIt\ProofEnvelope;

use ProvekIt\Canonicalizer\{Blake3, Ed25519, Jcs};

class Builder
{
    private Ed25519 $signer;

    public function __construct(Ed25519 $signer)
    {
        $this->signer = $signer;
    }

    /**
     * Build a proof envelope from a map of CID -> canonical bytes.
     * Returns {bytes: string, cid: string}
     */
    public function build(string $name, string $version, array $members, string $declaredAt): array
    {
        // Sort members by CID
        ksort($members, SORT_STRING);

        // Build minimal CBOR catalog
        $cbor = $this->encodeCatalog($name, $version, $members, $declaredAt);

        // Compute filename CID
        $fileCid = Blake3::cid($cbor);

        return [
            'bytes' => $cbor,
            'cid' => $fileCid,
        ];
    }

    private function encodeCatalog(string $name, string $version, array $members, string $declaredAt): string
    {
        ksort($members, SORT_STRING);
        $memberPairs = [];
        foreach ($members as $cid => $bytes) {
            $memberPairs[$cid] = $this->cborBytes($bytes);
        }
        $membersCbor = $this->cborMap($memberPairs);

        $signer = 'ed25519:' . $this->signer->pubKeyBase64();
        $signerCid = Blake3::cid($signer);

        $unsignedPairs = [
            'kind' => $this->cborString('catalog'),
            'name' => $this->cborString($name),
            'version' => $this->cborString($version),
            'members' => $membersCbor,
            'signer' => $this->cborString($signerCid),
            'declaredAt' => $this->cborString($declaredAt),
        ];
        $unsigned = $this->cborMap($unsignedPairs);
        $signature = $this->signer->sign($unsigned);

        $signedPairs = $unsignedPairs;
        $signedPairs['signature'] = $this->cborBytes($signature);
        return $this->cborMap($signedPairs);
    }

    private function cborMap(array $pairs): string
    {
        $encoded = [];
        foreach ($pairs as $key => $valueCbor) {
            $encoded[$this->cborString((string)$key)] = $valueCbor;
        }
        ksort($encoded, SORT_STRING);

        $buf = $this->cborHead(5, count($encoded));
        foreach ($encoded as $keyCbor => $valueCbor) {
            $buf .= $keyCbor . $valueCbor;
        }
        return $buf;
    }

    private function cborString(string $s): string
    {
        $len = strlen($s);
        return $this->cborHead(3, $len) . $s;
    }

    private function cborBytes(string $b): string
    {
        $len = strlen($b);
        return $this->cborHead(2, $len) . $b;
    }

    private function cborHead(int $major, int $n): string
    {
        if ($n <= 23) return chr(($major << 5) | $n);
        if ($n <= 0xFF) return chr(($major << 5) | 24) . chr($n);
        if ($n <= 0xFFFF) return chr(($major << 5) | 25) . pack('n', $n);
        return chr(($major << 5) | 26) . pack('N', $n);
    }
}
