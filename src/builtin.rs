//! Built-in tables from archspec-json. See `scripts/generate-tables.py`.
use crate::schema::*;
use std::collections::HashMap;
use std::sync::OnceLock;

mod cpuid;
mod microarchitectures;
mod strings;

use cpuid::{BITS, EXTENSION, EXTENSION_FLAGS, FLAGS, VENDOR};
use microarchitectures::{
    ALIASES, ARM_VENDORS, COMPILERS, DARWIN_FLAGS, DESCRIPTION, TARGETS, VERSIONS,
};
use strings::STRINGS;

// Generated string offsets point to NUL-terminated UTF-8 strings. The maximum
// offset marks an absent optional value; table ranges are end-exclusive.
fn string(offset: u16) -> &'static str {
    STRINGS[usize::from(offset)..]
        .split_once('\0')
        .expect("generated string is NUL-terminated")
        .0
}

fn optional(offset: u16) -> Option<&'static str> {
    (offset != u16::MAX).then(|| string(offset))
}

fn words(list: &str) -> Vec<String> {
    list.split_ascii_whitespace().map(str::to_owned).collect()
}

#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub(crate) name: &'static str,
    pub(crate) parents: &'static str,
    pub(crate) vendor: &'static str,
    pub(crate) features: &'static str,
    pub(crate) generation: Option<usize>,
    pub(crate) part: Option<&'static str>,
    pub(crate) compilers: Option<Compilers>,
}

/// Targets are ordered so every parent precedes its children.
pub(crate) fn targets() -> impl Iterator<Item = Target> {
    TARGETS.iter().map(
        |&[name, parents, vendor, features, generation, part, start, end]| Target {
            name: string(name),
            parents: string(parents),
            vendor: string(vendor),
            features: string(features),
            generation: (generation != u16::MAX).then_some(usize::from(generation)),
            part: optional(part),
            compilers: (start != u16::MAX).then_some(Compilers { start, end }),
        },
    )
}

impl MicroarchitecturesSchema {
    pub fn schema() -> &'static Self {
        static SCHEMA: OnceLock<MicroarchitecturesSchema> = OnceLock::new();
        SCHEMA.get_or_init(|| Self {
            microarchitectures: targets()
                .map(|target| {
                    (
                        target.name.to_owned(),
                        Microarchitecture {
                            from: words(target.parents),
                            vendor: target.vendor.to_owned(),
                            features: words(target.features),
                            generation: target.generation,
                            cpupart: target.part.map(str::to_owned),
                            compilers: target.compilers.map(|group| group.entries().collect()),
                        },
                    )
                })
                .collect(),
            feature_aliases: ALIASES
                .iter()
                .map(|&[name, reason, any_of, families]| {
                    (
                        string(name).to_owned(),
                        FeatureAlias {
                            reason: optional(reason).map(str::to_owned),
                            any_of: optional(any_of).map(words),
                            families: optional(families).map(words),
                        },
                    )
                })
                .collect(),
            conversions: Conversions {
                description: string(DESCRIPTION).to_owned(),
                arm_vendors: mapping(ARM_VENDORS),
                darwin_flags: mapping(DARWIN_FLAGS),
            },
        })
    }
}

fn pairs(rows: &'static [[u16; 2]]) -> impl Iterator<Item = (&'static str, &'static str)> {
    rows.iter()
        .map(|&[key, value]| (string(key), string(value)))
}

fn mapping(rows: &'static [[u16; 2]]) -> HashMap<String, String> {
    pairs(rows)
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

pub(crate) fn arm_vendor(implementer: &str) -> Option<&'static str> {
    pairs(ARM_VENDORS)
        .find(|(code, _)| *code == implementer)
        .map(|(_, vendor)| vendor)
}

pub(crate) fn darwin_flags() -> impl Iterator<Item = (&'static str, &'static str)> {
    pairs(DARWIN_FLAGS)
}

/// The range of compiler records for one target in the built-in catalog.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Compilers {
    start: u16,
    end: u16,
}

impl Compilers {
    pub(crate) fn is_empty(self) -> bool {
        self.start == self.end
    }

    pub(crate) fn entries(self) -> impl Iterator<Item = (String, CompilerSet)> {
        COMPILERS[usize::from(self.start)..usize::from(self.end)]
            .iter()
            .map(|&[name, single, start, end]| {
                let versions = &VERSIONS[usize::from(start)..usize::from(end)];
                let set = if single == 1 {
                    CompilerSet::Single(compiler(versions[0]))
                } else {
                    CompilerSet::Several(versions.iter().copied().map(compiler).collect())
                };
                (string(name).to_owned(), set)
            })
    }
}

fn compiler([versions, flags, name]: [u16; 3]) -> Compiler {
    Compiler {
        versions: string(versions).to_owned(),
        flags: string(flags).to_owned(),
        name: optional(name).map(str::to_owned),
    }
}

impl CpuIdSchema {
    pub fn schema() -> &'static Self {
        static SCHEMA: OnceLock<CpuIdSchema> = OnceLock::new();
        SCHEMA.get_or_init(|| Self {
            vendor: property(VENDOR),
            highest_extension_support: property(EXTENSION),
            flags: cpuid_flags(FLAGS),
            extension_flags: cpuid_flags(EXTENSION_FLAGS),
        })
    }
}

fn property([description, eax, ecx]: [u32; 3]) -> CpuIdProperty {
    CpuIdProperty {
        description: string(description as u16).to_owned(),
        input: CpuIdInput { eax, ecx },
    }
}

fn cpuid_flags(leaves: &[[u32; 5]]) -> Vec<CpuIdFlags> {
    leaves
        .iter()
        .map(|&[description, eax, ecx, start, end]| CpuIdFlags {
            description: string(description as u16).to_owned(),
            input: CpuIdInput { eax, ecx },
            bits: BITS[start as usize..end as usize]
                .iter()
                .map(|&[name, register, bit]| CpuIdBits {
                    name: string(name).to_owned(),
                    register: [
                        CpuRegister::Eax,
                        CpuRegister::Ebx,
                        CpuRegister::Ecx,
                        CpuRegister::Edx,
                    ][usize::from(register)],
                    bit: bit as u8,
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tables_match_json() {
        let json = include_str!("../json/cpu/microarchitectures.json");
        let expected: MicroarchitecturesSchema = serde_json::from_str(json).unwrap();
        assert_eq!(MicroarchitecturesSchema::schema(), &expected);

        let json = include_str!("../json/cpu/cpuid.json");
        let expected: CpuIdSchema = serde_json::from_str(json).unwrap();
        assert_eq!(CpuIdSchema::schema(), &expected);
    }
}
