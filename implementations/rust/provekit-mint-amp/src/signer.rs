// SPDX-License-Identifier: Apache-2.0

use std::path::{Component, Path};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};

use crate::{MintError, Result};

#[derive(Debug, Clone)]
pub struct Signer {
    key: SigningKey,
    key_id: String,
    unsigned_test_only: bool,
}

impl Signer {
    pub fn from_pem_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| MintError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let private = parse_private_key_text(&text)?;
        let key = signing_key_from_private_bytes(&private)?;
        Ok(Self::from_key(key, false, None))
    }

    pub fn from_unsigned_test_only() -> Self {
        Self::from_key(
            SigningKey::from_bytes(&[0xFA; 32]),
            true,
            Some("UNSIGNED_DEV_ONLY".into()),
        )
    }

    pub fn from_test_seed(seed: [u8; 32]) -> Self {
        Self::from_key(SigningKey::from_bytes(&seed), false, None)
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.key.verifying_key()
    }

    pub fn sign_b64(&self, message: &[u8]) -> String {
        let signature: Signature = self.key.sign(message);
        B64.encode(signature.to_bytes())
    }

    pub fn ensure_catalog_allowed(&self, root: &Path) -> Result<()> {
        if !self.unsigned_test_only {
            return Ok(());
        }
        if is_explicit_test_catalog(root) {
            Ok(())
        } else {
            Err(MintError::Signer(format!(
                "unsigned test signer refuses catalog root {}",
                root.display()
            )))
        }
    }

    fn from_key(key: SigningKey, unsigned_test_only: bool, key_id: Option<String>) -> Self {
        let key_id = key_id.unwrap_or_else(|| {
            let public = key.verifying_key().to_bytes();
            format!("ed25519:{}", B64.encode(public))
        });
        Self {
            key,
            key_id,
            unsigned_test_only,
        }
    }
}

fn parse_private_key_text(text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim();
    if let Some(b64) = trimmed.strip_prefix("ed25519:") {
        return B64
            .decode(b64.trim())
            .map_err(|e| MintError::Signer(format!("decode ed25519 private key: {e}")));
    }
    if looks_like_hex(trimmed) {
        return hex::decode(trimmed)
            .map_err(|e| MintError::Signer(format!("decode hex private key: {e}")));
    }
    if trimmed.contains("-----BEGIN") {
        let mut body = String::new();
        for line in trimmed.lines() {
            let line = line.trim();
            if line.starts_with("-----") || line.contains(':') || line.is_empty() {
                continue;
            }
            body.push_str(line);
        }
        return B64
            .decode(body)
            .map_err(|e| MintError::Signer(format!("decode PEM private key: {e}")));
    }
    B64.decode(trimmed)
        .map_err(|e| MintError::Signer(format!("decode private key: {e}")))
}

fn signing_key_from_private_bytes(bytes: &[u8]) -> Result<SigningKey> {
    let seed: [u8; 32] = if bytes.len() == 32 {
        bytes
            .try_into()
            .map_err(|_| MintError::Signer("invalid 32 byte Ed25519 seed".into()))?
    } else if bytes.len() == 64 {
        bytes[0..32]
            .try_into()
            .map_err(|_| MintError::Signer("invalid 64 byte Ed25519 private key".into()))?
    } else if bytes.len() >= 48 && bytes.windows(3).any(|w| w == [0x2b, 0x65, 0x70]) {
        bytes[bytes.len() - 32..]
            .try_into()
            .map_err(|_| MintError::Signer("invalid PKCS8 Ed25519 private key".into()))?
    } else {
        return Err(MintError::Signer(format!(
            "unsupported Ed25519 private key length {}",
            bytes.len()
        )));
    };
    Ok(SigningKey::from_bytes(&seed))
}

fn looks_like_hex(value: &str) -> bool {
    (value.len() == 64 || value.len() == 128) && value.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_explicit_test_catalog(root: &Path) -> bool {
    let path_text = root.to_string_lossy().to_ascii_lowercase();
    if path_text.contains("protocol/algorithm-catalog")
        || path_text.contains("protocol/language-catalog")
        || path_text.contains("production")
    {
        return false;
    }
    root.components().any(|component| match component {
        Component::Normal(part) => {
            let part = part.to_string_lossy().to_ascii_lowercase();
            part.contains("test") || part.contains("dev")
        }
        _ => false,
    })
}
