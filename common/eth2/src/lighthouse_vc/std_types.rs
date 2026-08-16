use bls::PublicKeyBytes;
use eth2_keystore::Keystore;
use serde::{Deserialize, Serialize};
use types::{Address, Graffiti};
use zeroize::Zeroizing;

pub use eip_3076::Interchange;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct GetFeeRecipientResponse {
    pub pubkey: PublicKeyBytes,
    #[serde(with = "serde_utils::address_hex")]
    pub ethaddress: Address,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct GetGasLimitResponse {
    pub pubkey: PublicKeyBytes,
    #[serde(with = "serde_utils::quoted_u64")]
    pub gas_limit: u64,
}

/// The keymanager-APIs#88 `BuilderConfig` shape — used for both GET response and POST request.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BuilderConfigOverride {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "opt_quoted_u64"
    )]
    pub min_bid: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "opt_quoted_u64"
    )]
    pub builder_boost_factor: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builders: Option<Vec<BuilderConfigOverrideEntry>>,
}

/// One entry in `BuilderConfigOverride.builders`, per keymanager-APIs#88's `BuilderEntry`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BuilderConfigOverrideEntry {
    pub url: String,
    /// Opaque `0x`-prefixed hex; omittable, derives from `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_data: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub builder_pubkeys: Vec<PublicKeyBytes>,
    /// Omittable on POST; inherits from a matching global `builder_definitions.yml` entry.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "opt_quoted_u64"
    )]
    pub max_execution_payment: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "opt_quoted_u64"
    )]
    pub min_bid: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "opt_quoted_u64"
    )]
    pub builder_boost_factor: Option<u64>,
}

/// Serde helper: `Option<u64>` as an optional quoted-decimal string, matching
/// keymanager-APIs#88's `Uint64` schema, omitting the field entirely when `None`.
mod opt_quoted_u64 {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => serializer.serialize_some(&v.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        let Some(s) = Option::<String>::deserialize(deserializer)? else {
            return Ok(None);
        };
        s.parse::<u64>().map(Some).map_err(de::Error::custom)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AuthResponse {
    pub token_path: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ListKeystoresResponse {
    pub data: Vec<SingleKeystoreResponse>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SingleKeystoreResponse {
    pub validating_pubkey: PublicKeyBytes,
    pub derivation_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportKeystoresRequest {
    pub keystores: Vec<KeystoreJsonStr>,
    pub passwords: Vec<Zeroizing<String>>,
    pub slashing_protection: Option<InterchangeJsonStr>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct KeystoreJsonStr(#[serde(with = "serde_utils::json_str")] pub Keystore);

impl std::ops::Deref for KeystoreJsonStr {
    type Target = Keystore;
    fn deref(&self) -> &Keystore {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct InterchangeJsonStr(#[serde(with = "serde_utils::json_str")] pub Interchange);

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportKeystoresResponse {
    pub data: Vec<Status<ImportKeystoreStatus>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Status<T> {
    pub status: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> Status<T> {
    pub fn ok(status: T) -> Self {
        Self {
            status,
            message: None,
        }
    }

    pub fn error(status: T, message: String) -> Self {
        Self {
            status,
            message: Some(message),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportKeystoreStatus {
    Imported,
    Duplicate,
    Error,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteKeystoresRequest {
    pub pubkeys: Vec<PublicKeyBytes>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteKeystoresResponse {
    pub data: Vec<Status<DeleteKeystoreStatus>>,
    #[serde(with = "serde_utils::json_str")]
    pub slashing_protection: Interchange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteKeystoreStatus {
    Deleted,
    NotActive,
    NotFound,
    Error,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ListRemotekeysResponse {
    pub data: Vec<SingleListRemotekeysResponse>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct SingleListRemotekeysResponse {
    pub pubkey: PublicKeyBytes,
    pub url: String,
    pub readonly: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRemotekeysRequest {
    pub remote_keys: Vec<SingleImportRemotekeysRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SingleImportRemotekeysRequest {
    pub pubkey: PublicKeyBytes,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportRemotekeyStatus {
    Imported,
    Duplicate,
    Error,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportRemotekeysResponse {
    pub data: Vec<Status<ImportRemotekeyStatus>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteRemotekeysRequest {
    pub pubkeys: Vec<PublicKeyBytes>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteRemotekeyStatus {
    Deleted,
    NotFound,
    Error,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteRemotekeysResponse {
    pub data: Vec<Status<DeleteRemotekeyStatus>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetGraffitiResponse {
    pub pubkey: PublicKeyBytes,
    pub graffiti: Graffiti,
}
