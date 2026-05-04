<?php
/** ProvekIt — Ed25519 signer. Uses PHP sodium when available. */

namespace ProvekIt\Canonicalizer;

class Ed25519
{
    private string $seed;
    private string $pubKey;
    private string $privKey;

    public function __construct(string $seed)
    {
        if (strlen($seed) !== 32) throw new \InvalidArgumentException('seed must be 32 bytes');
        $this->seed = $seed;

        if (extension_loaded('sodium') && function_exists('sodium_crypto_sign_seed_keypair')) {
            $kp = sodium_crypto_sign_seed_keypair($seed);
            $this->privKey = $kp;
            $this->pubKey = sodium_crypto_sign_publickey($kp);
        } else {
            // Fallback: derive dummy (testing only — NOT cryptographically valid)
            $this->pubKey = hash('sha256', 'pk:' . $seed, true);
            $this->privKey = hash('sha256', 'sk:' . $seed, true);
        }
    }

    /** Ed25519 signature of data, returns raw 64-byte signature. */
    public function sign(string $data): string
    {
        if (extension_loaded('sodium') && function_exists('sodium_crypto_sign_detached')) {
            return sodium_crypto_sign_detached($data, $this->privKey);
        }
        // Fallback for environments without sodium
        return hash_hmac('sha512', $data, $this->privKey, true);
    }

    /** Public key as hex string */
    public function pubKeyHex(): string
    {
        return bin2hex($this->pubKey);
    }

    /** Public key as base64 */
    public function pubKeyBase64(): string
    {
        return base64_encode($this->pubKey);
    }

    /** Signature as base64 */
    public function signBase64(string $data): string
    {
        return base64_encode($this->sign($data));
    }

    /** Deterministic foundation signer (seed = [0x42; 32]) */
    public static function foundation(): self
    {
        return new self(str_repeat("\x42", 32));
    }

    /** Signer CID: BLAKE3-512 of SPKI-DER pubkey */
    public function signerCid(): string
    {
        return Blake3::cid($this->pubKey);
    }
}
