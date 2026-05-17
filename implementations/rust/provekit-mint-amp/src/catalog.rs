// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{canonical_jcs, MintError, Result, Signer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Algorithm,
    Binding,
    Exam,
    Sort,
    Equation,
    EffectSignature,
    LanguageSignature,
    LanguageMorphism,
}

impl Kind {
    pub fn directory(self) -> &'static str {
        match self {
            Kind::Algorithm => "algorithms",
            Kind::Binding => "bindings",
            Kind::Exam => "exams",
            Kind::Sort => "sorts",
            Kind::Equation => "equations",
            Kind::EffectSignature => "signatures",
            Kind::LanguageSignature => "signatures",
            Kind::LanguageMorphism => "morphisms",
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Kind::Algorithm => "algorithm",
            Kind::Binding => "binding",
            Kind::Exam => "exam",
            Kind::Sort => "sort",
            Kind::Equation => "equation",
            Kind::EffectSignature => "effect_signature",
            Kind::LanguageSignature => "language_signature",
            Kind::LanguageMorphism => "language_morphism",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub root: PathBuf,
    pub index: CatalogIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogIndex {
    pub schema_version: String,
    #[serde(default)]
    pub entries: BTreeMap<String, CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub cid: String,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
}

impl Default for CatalogIndex {
    fn default() -> Self {
        Self {
            schema_version: "provekit-algebraic-catalog-index/1".into(),
            entries: BTreeMap::new(),
        }
    }
}

impl Catalog {
    pub fn new(root: PathBuf) -> Result<Catalog> {
        std::fs::create_dir_all(&root).map_err(|source| MintError::Io {
            path: root.clone(),
            source,
        })?;
        for dir in [
            "algorithms",
            "bindings",
            "exams",
            "signatures",
            "morphisms",
            "sorts",
            "equations",
        ] {
            let path = root.join(dir);
            std::fs::create_dir_all(&path).map_err(|source| MintError::Io { path, source })?;
        }
        let index = load_index(&root)?;
        if !root.join("index.json").exists() {
            save_index(&root, &index)?;
        }
        Ok(Catalog { root, index })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_memento(
        &mut self,
        kind: Kind,
        name: &str,
        payload: &Value,
        cid: &str,
    ) -> Result<PathBuf> {
        let signature = Value::Null;
        let path = write_envelope(
            &self.root,
            &mut self.index,
            kind,
            name,
            payload,
            cid,
            signature,
        )?;
        save_index(&self.root, &self.index)?;
        Ok(path)
    }

    pub fn write_signed_memento(
        &self,
        kind: Kind,
        name: &str,
        payload: &Value,
        cid: &str,
        signer: &Signer,
    ) -> Result<PathBuf> {
        let canonical = canonical_jcs(payload)?;
        let signature = json!({
            "alg": "ed25519",
            "key_id": signer.key_id(),
            "sig_b64": signer.sign_b64(canonical.as_bytes()),
        });
        let mut index = load_index(&self.root)?;
        let path = write_envelope(&self.root, &mut index, kind, name, payload, cid, signature)?;
        save_index(&self.root, &index)?;
        Ok(path)
    }

    pub fn read_by_cid(&self, cid: &str) -> Result<Value> {
        let index = load_index(&self.root)?;
        let path = if let Some(entry) = index.entries.get(cid) {
            self.root.join(&entry.path)
        } else {
            find_cid_path(&self.root, cid)?.ok_or_else(|| {
                MintError::Catalog(format!("CID `{cid}` is not present in catalog index"))
            })?
        };
        read_envelope_payload(&path, cid)
    }
}

fn write_envelope(
    root: &Path,
    index: &mut CatalogIndex,
    kind: Kind,
    name: &str,
    payload: &Value,
    cid: &str,
    signature: Value,
) -> Result<PathBuf> {
    let relative = index
        .entries
        .get(cid)
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| {
            PathBuf::from(kind.directory()).join(format!("{}.{}.json", sanitize_name(name), cid))
        });
    let path = root.join(&relative);

    if path.exists() {
        let existing = read_envelope(&path)?;
        if existing.get("cid").and_then(Value::as_str) == Some(cid)
            && existing.get("memento") == Some(payload)
        {
            index
                .entries
                .entry(cid.to_string())
                .or_insert(CatalogEntry {
                    cid: cid.to_string(),
                    kind,
                    name: name.to_string(),
                    path: relative,
                });
            return Ok(path);
        }
        return Err(MintError::Catalog(format!(
            "CID collision for `{cid}` at {}",
            path.display()
        )));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| MintError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let envelope = json!({
        "memento": payload,
        "cid": cid,
        "signature": signature,
    });
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|source| MintError::Json {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|source| MintError::Io {
        path: path.clone(),
        source,
    })?;
    index.entries.insert(
        cid.to_string(),
        CatalogEntry {
            cid: cid.to_string(),
            kind,
            name: name.to_string(),
            path: relative,
        },
    );
    Ok(path)
}

fn read_envelope_payload(path: &Path, cid: &str) -> Result<Value> {
    let envelope = read_envelope(path)?;
    if envelope.get("cid").and_then(Value::as_str) != Some(cid) {
        return Err(MintError::Catalog(format!(
            "catalog entry {} does not contain CID `{cid}`",
            path.display()
        )));
    }
    envelope
        .get("memento")
        .cloned()
        .ok_or_else(|| MintError::Catalog(format!("missing memento in {}", path.display())))
}

fn read_envelope(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).map_err(|source| MintError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| MintError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn load_index(root: &Path) -> Result<CatalogIndex> {
    let path = root.join("index.json");
    if !path.exists() {
        return Ok(CatalogIndex::default());
    }
    let bytes = std::fs::read(&path).map_err(|source| MintError::Io {
        path: path.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| MintError::Json { path, source })
}

fn save_index(root: &Path, index: &CatalogIndex) -> Result<()> {
    let path = root.join("index.json");
    let bytes = serde_json::to_vec_pretty(index).map_err(|source| MintError::Json {
        path: path.clone(),
        source,
    })?;
    std::fs::write(&path, bytes).map_err(|source| MintError::Io { path, source })
}

fn find_cid_path(root: &Path, cid: &str) -> Result<Option<PathBuf>> {
    for dir in [
        "algorithms",
        "bindings",
        "signatures",
        "morphisms",
        "sorts",
        "equations",
    ] {
        let dir_path = root.join(dir);
        if !dir_path.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir_path).map_err(|source| MintError::Io {
            path: dir_path.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| MintError::Io {
                path: dir_path.clone(),
                source,
            })?;
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(cid))
                .unwrap_or(false)
            {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ' ' | '\t' | '\n' | '\r' => '_',
            _ => c,
        })
        .collect()
}
