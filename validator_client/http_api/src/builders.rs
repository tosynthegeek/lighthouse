use builder_definition::{BuilderDefinition, BuilderOverride};
use builder_store::GlobalBuilderConfig;
use builder_types::{BuilderUrl, RequestAuthData};
use eth2::lighthouse_vc::std_types::{BuilderConfigOverride, BuilderConfigOverrideEntry};

#[derive(Debug)]
pub enum ConversionError {
    /// The entry's `url` could not be parsed.
    InvalidUrl(String),
    /// The entry's `auth_data` is not valid hex.
    InvalidAuthData(String),
    /// `max_execution_payment` was omitted and no global entry with the same `(url, auth_data)`
    /// exists to inherit it from.
    NoMatchingGlobalBuilder(String),
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionError::InvalidUrl(url) => write!(f, "invalid builder url: {url}"),
            ConversionError::InvalidAuthData(url) => {
                write!(f, "invalid auth_data for builder {url}")
            }
            ConversionError::NoMatchingGlobalBuilder(url) => write!(
                f,
                "max_execution_payment omitted for {url}, and no matching builder in \
                 builder_definitions.yml to inherit it from"
            ),
        }
    }
}

/// Convert a `POST .../builders` request body into the internal `BuilderOverride`, resolving each
/// entry's omitted `max_execution_payment` against `global.builders`.
pub fn builder_override_from_wire(
    dto: BuilderConfigOverride,
    global: &GlobalBuilderConfig,
) -> Result<BuilderOverride, ConversionError> {
    let builders = dto
        .builders
        .map(|entries| {
            entries
                .into_iter()
                .map(|entry| builder_definition_from_wire(entry, &global.builders))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    Ok(BuilderOverride {
        min_bid: dto.min_bid,
        builder_boost_factor: dto.builder_boost_factor,
        builders,
    })
}

fn builder_definition_from_wire(
    entry: BuilderConfigOverrideEntry,
    global_builders: &[BuilderDefinition],
) -> Result<BuilderDefinition, ConversionError> {
    let url: BuilderUrl = entry
        .url
        .parse()
        .map_err(|_| ConversionError::InvalidUrl(entry.url.clone()))?;

    let auth_data = entry
        .auth_data
        .as_deref()
        .map(parse_auth_data_hex)
        .transpose()
        .map_err(|_| ConversionError::InvalidAuthData(entry.url.clone()))?;

    let max_execution_payment = match entry.max_execution_payment {
        Some(value) => value,
        None => {
            let resolved_auth = auth_data
                .clone()
                .unwrap_or_else(|| url.to_default_auth_data());
            global_builders
                .iter()
                .find(|d| {
                    d.url == url
                        && d.auth_data
                            .clone()
                            .unwrap_or_else(|| d.url.to_default_auth_data())
                            == resolved_auth
                })
                .map(|d| d.max_execution_payment)
                .ok_or_else(|| ConversionError::NoMatchingGlobalBuilder(entry.url.clone()))?
        }
    };

    Ok(BuilderDefinition {
        // Keymanager POST always implies an active entry — the API has no `enabled` toggle.
        enabled: true,
        url,
        auth_data,
        builder_pubkeys: entry.builder_pubkeys,
        max_execution_payment,
        min_bid: entry.min_bid,
        builder_boost_factor: entry.builder_boost_factor,
    })
}

fn parse_auth_data_hex(s: &str) -> Result<RequestAuthData, ()> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(stripped).map_err(|_| ())?;
    RequestAuthData::new(bytes).map_err(|_| ())
}

/// Convert a stored `BuilderOverride` (or `None`, meaning no override) into the fully-resolved
/// `GET .../builders` response — every field always populated, mirroring the exact resolution
/// chain `BuilderStore::builder_config()` uses when signing.
pub fn builder_override_to_wire(
    stored: Option<&BuilderOverride>,
    global: &GlobalBuilderConfig,
) -> BuilderConfigOverride {
    let min_bid = stored.and_then(|o| o.min_bid).unwrap_or(global.min_bid);
    let builder_boost_factor = stored
        .and_then(|o| o.builder_boost_factor)
        .unwrap_or(global.builder_boost_factor);

    let builders_source: &[BuilderDefinition] = match stored.and_then(|o| o.builders.as_ref()) {
        Some(builders) => builders,
        None => &global.builders,
    };

    let builders = builders_source
        .iter()
        .filter(|d| d.enabled)
        .map(|d| builder_definition_to_wire(d, min_bid, builder_boost_factor))
        .collect();

    BuilderConfigOverride {
        min_bid: Some(min_bid),
        builder_boost_factor: Some(builder_boost_factor),
        builders: Some(builders),
    }
}

fn builder_definition_to_wire(
    d: &BuilderDefinition,
    min_bid: u64,
    builder_boost_factor: u64,
) -> BuilderConfigOverrideEntry {
    let resolved_auth = d
        .auth_data
        .clone()
        .unwrap_or_else(|| d.url.to_default_auth_data());

    BuilderConfigOverrideEntry {
        url: d.url.to_string(),
        auth_data: Some(format!("0x{}", hex::encode(&resolved_auth[..]))),
        builder_pubkeys: d.builder_pubkeys.clone(),
        max_execution_payment: Some(d.max_execution_payment),
        min_bid: Some(d.min_bid.unwrap_or(min_bid)),
        builder_boost_factor: Some(d.builder_boost_factor.unwrap_or(builder_boost_factor)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn global_with_one_builder(max_execution_payment: u64) -> GlobalBuilderConfig {
        GlobalBuilderConfig {
            min_bid: 5,
            builder_boost_factor: 100,
            builders: vec![BuilderDefinition {
                enabled: true,
                url: "http://builder-a.example.com".parse().unwrap(),
                auth_data: None,
                builder_pubkeys: vec![],
                max_execution_payment,
                min_bid: None,
                builder_boost_factor: None,
            }],
        }
    }

    #[test]
    fn max_execution_payment_omitted_inherits_from_matching_global_entry() {
        let global = global_with_one_builder(250_000_000);
        let dto = BuilderConfigOverride {
            min_bid: None,
            builder_boost_factor: None,
            builders: Some(vec![BuilderConfigOverrideEntry {
                url: "http://builder-a.example.com".to_string(),
                auth_data: None,
                builder_pubkeys: vec![],
                max_execution_payment: None,
                min_bid: None,
                builder_boost_factor: None,
            }]),
        };

        let result = builder_override_from_wire(dto, &global).unwrap();
        let builders = result.builders.unwrap();
        assert_eq!(builders[0].max_execution_payment, 250_000_000);
    }

    #[test]
    fn max_execution_payment_omitted_with_no_matching_global_entry_errors() {
        let global = global_with_one_builder(250_000_000);
        let dto = BuilderConfigOverride {
            min_bid: None,
            builder_boost_factor: None,
            builders: Some(vec![BuilderConfigOverrideEntry {
                url: "http://builder-b.example.com".to_string(), // no matching global entry
                auth_data: None,
                builder_pubkeys: vec![],
                max_execution_payment: None,
                min_bid: None,
                builder_boost_factor: None,
            }]),
        };

        assert!(matches!(
            builder_override_from_wire(dto, &global),
            Err(ConversionError::NoMatchingGlobalBuilder(_))
        ));
    }

    #[test]
    fn builder_override_to_wire_with_no_override_renders_global_list() {
        let global = global_with_one_builder(250_000_000);
        let wire = builder_override_to_wire(None, &global);

        assert_eq!(wire.min_bid, Some(5));
        assert_eq!(wire.builder_boost_factor, Some(100));
        let builders = wire.builders.unwrap();
        assert_eq!(builders.len(), 1);
        assert_eq!(builders[0].max_execution_payment, Some(250_000_000));
        // Per-entry min_bid/builder_boost_factor resolve to the global values, since the entry
        // itself sets neither.
        assert_eq!(builders[0].min_bid, Some(5));
        assert_eq!(builders[0].builder_boost_factor, Some(100));
    }

    #[test]
    fn builder_override_to_wire_with_empty_override_renders_no_builders() {
        let global = global_with_one_builder(250_000_000);
        let stored = BuilderOverride {
            min_bid: None,
            builder_boost_factor: None,
            builders: Some(vec![]),
        };
        let wire = builder_override_to_wire(Some(&stored), &global);

        assert_eq!(wire.builders, Some(vec![]));
    }

    #[test]
    fn builder_override_to_wire_resolves_auth_data_even_when_derived() {
        let global = GlobalBuilderConfig {
            min_bid: 0,
            builder_boost_factor: 100,
            builders: vec![],
        };
        let stored = BuilderOverride {
            min_bid: None,
            builder_boost_factor: None,
            builders: Some(vec![BuilderDefinition {
                enabled: true,
                url: "http://builder.example.com".parse().unwrap(),
                auth_data: None, // derives from url
                builder_pubkeys: vec![],
                max_execution_payment: 1,
                min_bid: None,
                builder_boost_factor: None,
            }]),
        };
        let wire = builder_override_to_wire(Some(&stored), &global);

        // auth_data is Some(...) on the wire even though it was None internally.
        assert!(wire.builders.unwrap()[0].auth_data.is_some());
    }
}
