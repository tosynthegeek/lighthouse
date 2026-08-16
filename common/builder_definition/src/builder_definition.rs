use std::collections::HashSet;

use bls::PublicKeyBytes;
use builder_types::{BuilderUrl, MAX_BUILDER_ENTRIES, RequestAuthData};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum ValidationError {
    /// A builder with the given URL and auth data already exists.
    DuplicateBuilderAuth(BuilderUrl),
    /// A builder URL could not be parsed as a URL.
    InvalidBuilderUrl(BuilderUrl),
    /// A builder URL does not use an `http`/`https` scheme.
    UnsupportedUrlScheme(BuilderUrl),
    /// More than `MAX_BUILDER_ENTRIES` builders are enabled, exceeding what fits in a
    /// `BuilderConfig`.
    TooManyEnabledBuilders { enabled: usize, max: usize },
}

/// A single builder in the config file: a direct bid request, with optional per-builder overrides
/// of the global bid policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuilderDefinition {
    /// Indicates whether this definition is enabled or disabled.
    pub enabled: bool,
    /// The URL the beacon node uses to contact this builder. Routing metadata; never signed.
    pub url: BuilderUrl,
    /// Opaque authentication data signed into `RequestAuth.data`, agreed with the builder out of
    /// band, as a `0x`-prefixed hex string. When unset, it defaults to the UTF-8 bytes of `url`
    /// (the builder-specs #165 default). Must be non-empty when set: a zero-length `data` is
    /// invalid on the wire.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_option_auth_data"
    )]
    pub auth_data: Option<RequestAuthData>,
    /// The builder BLS public keys this builder's bids may be signed by, hex-encoded. Empty (or
    /// omitted) accepts any builder; otherwise a bid not signed by one of them is rejected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub builder_pubkeys: Vec<PublicKeyBytes>,
    /// The maximum execution payment, in gwei, that we're willing to accept from this builder.
    pub max_execution_payment: u64,
    /// Per-builder override of the global minimum total payment (gwei). Inherits the global when
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_bid: Option<u64>,
    /// Per-builder override of the global boost factor. Inherits the global when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_boost_factor: Option<u64>,
}

/// A validator public key's builder configuration, as stored on `ValidatorDefinition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuilderOverride {
    /// This key's default `min_bid` (gwei). Falls back to the VC's global `min_bid` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_bid: Option<u64>,
    /// This key's default `builder_boost_factor`. Falls back to the VC's global value when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_boost_factor: Option<u64>,
    /// The builders this key sources bids from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builders: Option<Vec<BuilderDefinition>>,
}

// The enabled builders must fit in a `BuilderConfig`'s bounded list, so
// `BuilderStore::builder_config` cannot overflow when constructing it.
pub fn validate_builders(builders: &[BuilderDefinition]) -> Result<(), ValidationError> {
    let enabled = builders.iter().filter(|d| d.enabled).count();
    if enabled > MAX_BUILDER_ENTRIES {
        return Err(ValidationError::TooManyEnabledBuilders {
            enabled,
            max: MAX_BUILDER_ENTRIES,
        });
    }
    let mut seen_auth_urls = HashSet::new();
    for definition in builders {
        if !definition.enabled {
            continue;
        }
        let url = &definition.url;
        let sensitive_url = url
            .to_sensitive_url()
            .map_err(|_| ValidationError::InvalidBuilderUrl(url.clone()))?;
        if !matches!(sensitive_url.expose_full().scheme(), "http" | "https") {
            return Err(ValidationError::UnsupportedUrlScheme(url.clone()));
        }
        let auth = definition
            .auth_data
            .clone()
            .unwrap_or_else(|| url.to_default_auth_data());
        if !seen_auth_urls.insert((url.clone(), auth)) {
            return Err(ValidationError::DuplicateBuilderAuth(url.clone()));
        }
    }
    Ok(())
}

/// Serde helper: represent `Option<RequestAuthData>` as a `0x`-prefixed hex string in the config
/// file (matching how other byte fields are encoded), omitting it entirely when `None`.
mod serde_option_auth_data {
    use super::RequestAuthData;
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S: Serializer>(
        value: &Option<RequestAuthData>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(data) => serializer.serialize_some(&format!("0x{}", hex::encode(&data[..]))),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<RequestAuthData>, D::Error> {
        let Some(s) = Option::<String>::deserialize(deserializer)? else {
            return Ok(None);
        };
        let stripped = s.strip_prefix("0x").unwrap_or(&s);
        let bytes = hex::decode(stripped).map_err(de::Error::custom)?;
        let data = RequestAuthData::new(bytes)
            .map_err(|_| de::Error::custom("auth_data exceeds the maximum size"))?;
        Ok(Some(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_data_round_trips_as_hex() {
        let definition = BuilderDefinition {
            enabled: true,
            url: "http://builder.example.com".parse().unwrap(),
            auth_data: Some(RequestAuthData::new(b"hello".to_vec()).unwrap()),
            builder_pubkeys: vec![],
            max_execution_payment: 1,
            min_bid: None,
            builder_boost_factor: None,
        };

        let yaml = yaml_serde::to_string(&definition).unwrap();
        // "hello" is 0x68656c6c6f, a hex string — not a YAML sequence of byte values.
        assert!(
            yaml.contains("0x68656c6c6f"),
            "auth_data not hex-encoded:\n{yaml}"
        );

        let decoded: BuilderDefinition = yaml_serde::from_str(&yaml).unwrap();
        assert_eq!(decoded, definition);
    }

    #[test]
    fn omits_none_optional_fields() {
        let definition = BuilderDefinition {
            enabled: true,
            url: "http://builder.example.com".parse().unwrap(),
            auth_data: None,
            builder_pubkeys: vec![],
            max_execution_payment: 1,
            min_bid: None,
            builder_boost_factor: None,
        };
        let yaml = yaml_serde::to_string(&definition).unwrap();
        for field in [
            "auth_data",
            "builder_pubkeys",
            "min_bid",
            "builder_boost_factor",
        ] {
            assert!(
                !yaml.contains(field),
                "unset `{field}` should be omitted:\n{yaml}"
            );
        }
    }
}
