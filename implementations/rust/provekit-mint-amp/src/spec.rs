// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{JsonSpec, MintError, Result};

macro_rules! spec_type {
    ($name:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            pub(crate) raw: Value,
            pub(crate) source_dir: Option<PathBuf>,
        }

        impl $name {
            pub fn from_path(path: &Path) -> Result<Self> {
                let bytes = std::fs::read(path).map_err(|source| MintError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
                let raw: Value =
                    serde_json::from_slice(&bytes).map_err(|source| MintError::Json {
                        path: path.to_path_buf(),
                        source,
                    })?;
                Ok(Self {
                    raw,
                    source_dir: path.parent().map(Path::to_path_buf),
                })
            }

            pub fn from_json_str(input: &str) -> Result<Self> {
                let raw: Value = serde_json::from_str(input).map_err(|source| MintError::Json {
                    path: PathBuf::from("<inline>"),
                    source,
                })?;
                Ok(Self {
                    raw,
                    source_dir: None,
                })
            }

            pub fn from_value(raw: Value) -> Self {
                Self {
                    raw,
                    source_dir: None,
                }
            }

            pub fn raw(&self) -> &Value {
                &self.raw
            }
        }

        impl JsonSpec for $name {
            fn raw(&self) -> &Value {
                &self.raw
            }

            fn base_dir(&self) -> Option<&Path> {
                self.source_dir.as_deref()
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                self.raw.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let raw = Value::deserialize(deserializer)?;
                Ok(Self {
                    raw,
                    source_dir: None,
                })
            }
        }
    };
}

spec_type!(AlgorithmSpec);
spec_type!(BindingSpec);
spec_type!(SortSpec);
spec_type!(EquationSpec);
spec_type!(EffectSignatureSpec);
spec_type!(LanguageSignatureSpec);
spec_type!(LanguageMorphismSpec);
