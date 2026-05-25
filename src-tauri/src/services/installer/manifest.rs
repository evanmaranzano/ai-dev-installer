use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppError;

const HASH_BUFFER_SIZE: usize = 64 * 1024;
const BUNDLED_MANIFEST_JSON: &str = include_str!("../../../resources/third_party/manifest.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BundledResource {
    pub component_id: String,
    pub version: String,
    pub file_name: String,
    pub sha256: String,
    pub install_command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstallerManifest {
    pub resources: Vec<BundledResource>,
}

impl InstallerManifest {
    pub fn resource(&self, component_id: &str) -> Option<&BundledResource> {
        self.resources
            .iter()
            .find(|item| item.component_id == component_id)
    }

    pub fn from_json_str(json: &str) -> Result<Self, AppError> {
        serde_json::from_str(json).map_err(|error| AppError {
            code: "installer_manifest_parse_failed".into(),
            message: "Failed to parse installer manifest".into(),
            details: Some(error.to_string()),
        })
    }

    pub fn bundled() -> Result<Self, AppError> {
        Self::from_json_str(BUNDLED_MANIFEST_JSON)
    }
}

pub fn verify_sha256(path: &Path, expected_sha256: &str) -> Result<bool, AppError> {
    let mut file = File::open(path).map_err(|error| AppError {
        code: "installer_manifest_read_failed".into(),
        message: "Failed to read bundled installer resource".into(),
        details: Some(error.to_string()),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|error| AppError {
            code: "installer_manifest_read_failed".into(),
            message: "Failed to read bundled installer resource".into(),
            details: Some(error.to_string()),
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let digest = hasher.finalize();
    let actual = hex::encode(digest);
    Ok(actual.eq_ignore_ascii_case(expected_sha256))
}
