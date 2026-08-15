use account_utils::write_file_via_temporary;
use builder_definition::{BuilderDefinition, ValidationError, validate_builders};
use serde::{Deserialize, Serialize};
use std::fs::{File, create_dir_all};
use std::io;
use std::path::{Path, PathBuf};

/// The file name for the serialized `BuilderConfigFile` struct.
pub const BUILDERS_FILENAME: &str = "builder_definitions.yml";
/// The temporary file name for the serialized `BuilderConfigFile` struct.
///
/// This is used to achieve an atomic update of the contents on disk, without truncation.
pub const BUILDERS_TEMP_FILENAME: &str = ".builder_definitions.yml.tmp";

#[derive(Debug)]
pub enum Error {
    /// The config file could not be opened.
    UnableToOpenFile(io::Error),
    /// The config file could not be parsed as YAML.
    UnableToParseFile(yaml_serde::Error),
    /// The builders file could not be serialized as YAML.
    UnableToEncodeFile(yaml_serde::Error),
    /// The builders file or temp file could not be written to the filesystem.
    UnableToWriteFile(filesystem::Error),
    /// The validator directory could not be created.
    UnableToCreateValidatorDir(PathBuf),
    /// The list of builders failed validation — see builder_definition::ValidationError
    Validation(ValidationError),
}

impl From<ValidationError> for Error {
    fn from(e: ValidationError) -> Self {
        Error::Validation(e)
    }
}

fn default_builder_boost_factor() -> u64 {
    100
}

/// The validator client's builder configuration file.
///
/// Holds the global bid-policy defaults plus the list of builders to request bids from directly. It
/// resolves into the wire `BuilderConfig` at block-production time: the globals govern p2p bids and
/// fill in any builder that omits `min_bid`/`builder_boost_factor`.
#[derive(Clone, Serialize, Deserialize)]
pub struct BuilderConfigFile {
    /// Global minimum total payment (gwei). Applies to p2p bids and is inherited by any builder that
    /// omits its own `min_bid`.
    #[serde(default)]
    pub min_bid: u64,
    /// Global boost factor. Applies to p2p bids and is inherited by any builder that omits its own
    /// `builder_boost_factor`.
    #[serde(default = "default_builder_boost_factor")]
    pub builder_boost_factor: u64,
    /// The builders to request bids from directly.
    #[serde(default)]
    pub builders: Vec<BuilderDefinition>,
}

impl Default for BuilderConfigFile {
    fn default() -> Self {
        Self {
            min_bid: 0,
            builder_boost_factor: default_builder_boost_factor(),
            builders: Vec::new(),
        }
    }
}

impl BuilderConfigFile {
    /// Open an existing file or create a new, empty one if it does not exist.
    pub fn open_or_create<P: AsRef<Path>>(validators_dir: P) -> Result<Self, Error> {
        create_dir_all(validators_dir.as_ref()).map_err(|_| {
            Error::UnableToCreateValidatorDir(PathBuf::from(validators_dir.as_ref()))
        })?;
        let builders_file_path = validators_dir.as_ref().join(BUILDERS_FILENAME);
        if !builders_file_path.exists() {
            let this = Self::default();
            this.save(&validators_dir)?;
        }
        Self::open(validators_dir)
    }

    /// Open an existing file, returning an error if the file does not exist.
    pub fn open<P: AsRef<Path>>(validators_dir: P) -> Result<Self, Error> {
        let config_path = validators_dir.as_ref().join(BUILDERS_FILENAME);
        let file = File::options()
            .write(true)
            .read(true)
            .create_new(false)
            .open(config_path)
            .map_err(Error::UnableToOpenFile)?;
        let config: Self = yaml_serde::from_reader(file).map_err(Error::UnableToParseFile)?;
        config.validate()?;
        Ok(config)
    }

    /// Encodes `self` as a YAML string and atomically writes it to the `CONFIG_FILENAME` file in
    /// the `validators_dir` directory.
    ///
    /// Will create a new file if it does not exist or overwrite any existing file.
    pub fn save<P: AsRef<Path>>(&self, validators_dir: P) -> Result<(), Error> {
        let config_path = validators_dir.as_ref().join(BUILDERS_FILENAME);
        let temp_path = validators_dir.as_ref().join(BUILDERS_TEMP_FILENAME);
        let mut bytes = vec![];
        yaml_serde::to_writer(&mut bytes, self).map_err(Error::UnableToEncodeFile)?;

        write_file_via_temporary(&config_path, &temp_path, &bytes)
            .map_err(Error::UnableToWriteFile)?;

        Ok(())
    }

    pub fn as_slice(&self) -> &[BuilderDefinition] {
        &self.builders
    }

    pub fn push(&mut self, definition: BuilderDefinition) {
        self.builders.push(definition);
    }

    pub fn validate(&self) -> Result<(), Error> {
        validate_builders(&self.builders).map_err(Error::from)
    }
}

impl<'a> IntoIterator for &'a BuilderConfigFile {
    type Item = &'a BuilderDefinition;
    type IntoIter = std::slice::Iter<'a, BuilderDefinition>;

    fn into_iter(self) -> Self::IntoIter {
        self.builders.iter()
    }
}
