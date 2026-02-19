use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Schema for microarchitecture definitions and feature aliases.
#[derive(Debug, Deserialize)]
pub struct MicroarchitecturesSchema {
    pub microarchitectures: HashMap<String, Microarchitecture>,
    pub feature_aliases: HashMap<String, FeatureAlias>,
    pub conversions: Conversions,
}

impl MicroarchitecturesSchema {
    pub fn schema() -> &'static MicroarchitecturesSchema {
        static SCHEMA: OnceLock<MicroarchitecturesSchema> = OnceLock::new();
        SCHEMA.get_or_init(|| {
            serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/json/cpu/microarchitectures.json"
            )))
            .expect("Failed to load microarchitectures.json")
        })
    }
}

/// Defines the attributes and requirements of a microarchitecture.
#[derive(Debug, Deserialize)]
pub struct Microarchitecture {
    /// A list of the immediate microarchitectures that this one is considered
    /// to be derived from.
    #[serde(deserialize_with = "super::zero_one_many_string")]
    pub from: Vec<String>,

    /// Human-readable vendor name.
    pub vendor: String,

    /// The CPU features that are required to exist on the system for it to be
    /// compatible with this microarchitecture.
    pub features: Vec<String>,

    /// Optional information on how to tell different compilers how to optimize
    /// for this microarchitecture.
    pub compilers: Option<HashMap<String, CompilerSet>>,

    /// Generation of the microarchitecture, if relevant.
    pub generation: Option<usize>,

    /// CPU part identifier, used to distinguish microarchitectures with identical features
    /// (e.g. aarch64 neoverse_n2 vs neoverse_v2).
    pub cpupart: Option<String>,
}

/// Compiler optimization for a particular compiler, either one for all flavours
/// of the compiler or several indicating how to do it for particular version ranges.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CompilerSet {
    /// Multiple entries (Compiler change options across versions).
    Several(Vec<Compiler>),

    /// Single entry (Compiler didn't change options across versions).
    Single(Compiler),
}

/// Indicates how to tell a particular compiler flavour how to optimize
/// for an microarchitecture.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Compiler {
    /// Indicates the versions of the compiler this applies to.
    pub versions: String,

    /// Command line argument to pass to the compiler to optimize for this architecture.
    /// May contain `{name}` placeholders.
    pub flags: String,

    /// Architecture name, for use in the optimization flags.
    pub name: Option<String>,
}

/// Synthesised feature aliases derived from existing features or families.
#[derive(Debug, Clone, Deserialize)]
pub struct FeatureAlias {
    /// The reason for why this alias is defined.
    pub reason: Option<String>,

    /// The alias is valid if any of the items are a feature of the target.
    pub any_of: Option<Vec<String>>,

    /// The alias is valid if the family of the target is in this list.
    pub families: Option<Vec<String>>,
}

/// Conversions that map some platform specific value to canonical values.
#[derive(Debug, Deserialize)]
pub struct Conversions {
    pub description: String,

    /// Maps from ARM vendor hex-values to actual vendor names.
    pub arm_vendors: HashMap<String, String>,

    /// Maps from macOS feature flags to the expected feature names.
    pub darwin_flags: HashMap<String, String>,
}
