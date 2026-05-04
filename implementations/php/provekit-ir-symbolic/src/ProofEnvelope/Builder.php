<?php
/** ProvekIt — Deterministic CBOR catalog builder. Simplified for hand-rolled CBOR. */

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

    /**
     * Simplified deterministic CBOR catalog encoding.
     * Real implementation would use a proper CBOR library;
     * this hand-rolled version produces correct structure for verification.
     */
    private function encodeCatalog(string $name, string $version, array $members, string $declaredAt): string
    {
        // CBOR map with deterministic integer keys
        $map = [];

        // Key 0: kind = "catalog"
        $map[0] = $this->cborString('catalog');

        // Key 1: name
        $map[1] = $this->cborString($name . '@provekit/lift');

        // Key 2: signer
        $map[2] = $this->cborString(Blake3::cid($this->signer->pubKeyHex()));

        // Key 3: members (CBOR map)
        $membersCbor = chr(0xa0 | count($members)); // map header
        foreach ($members as $cid => $bytes) {
            $membersCbor .= $this->cborString($cid);
            $membersCbor .= $this->cborBytes($bytes);
        }
        $map[3] = $membersCbor;

        // Key 4: version
        $map[4] = $this->cborString($version);

        // Key 5: signature (sign canonical data)
        $map[5] = $this->cborString(
            'ed25519:' . $this->signer->signBase64($membersCbor)
        );

        // Key 6: declaredAt
        $map[6] = $this->cborString($declaredAt);

        // Deterministic: sort keys before encoding
        ksort($map);
        $buf = chr(0xa0 | count($map));
        foreach ($map as $k => $v) {
            $buf .= $this->cborInt($k) . $v;
        }

        return $buf;
    }

    private function cborInt(int $n): string
    {
        if ($n <= 23) return chr($n);
        if ($n <= 0xFF) return chr(0x18) . chr($n);
        if ($n <= 0xFFFF) return chr(0x19) . pack('n', $n);
        throw new \RuntimeException("int too large for hand-rolled CBOR");
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
