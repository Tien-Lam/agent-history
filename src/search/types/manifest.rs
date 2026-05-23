use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub(in crate::search) struct Manifest {
    pub sessions: std::collections::HashMap<String, FileFingerprint>,
    /// Note id -> `updated_at` snapshot from the metadata sidecar. `#[serde(default)]`
    /// keeps older manifests deserializable; on schema-mismatch wipes the whole
    /// manifest is recreated from scratch anyway.
    #[serde(default)]
    pub notes: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::search) struct FileFingerprint {
    pub len: u64,
    pub modified_nanos: u64,
    pub sha256: String,
}

impl<'de> Deserialize<'de> for FileFingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Compat {
            Current {
                len: u64,
                modified_nanos: u64,
                sha256: String,
            },
            LegacySeconds(u64),
        }

        match Compat::deserialize(deserializer)? {
            Compat::Current {
                len,
                modified_nanos,
                sha256,
            } => Ok(Self {
                len,
                modified_nanos,
                sha256,
            }),
            Compat::LegacySeconds(seconds) => Ok(Self {
                len: 0,
                modified_nanos: seconds.saturating_mul(1_000_000_000),
                sha256: String::new(),
            }),
        }
    }
}
