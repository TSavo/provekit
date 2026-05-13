#![allow(dead_code)]

use std::path::{Path, PathBuf};

use provekit_mint_amp::{Catalog, Result, Signer};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn test_catalog(prefix: &str) -> Result<(tempfile::TempDir, Catalog)> {
    let dir = tempfile::Builder::new()
        .prefix(&format!("provekit-minter-test-{prefix}-"))
        .tempdir()
        .expect("tempdir");
    let catalog = Catalog::new(dir.path().join("catalog-test"))?;
    Ok((dir, catalog))
}

pub fn signer() -> Signer {
    Signer::from_test_seed([0x34; 32])
}
