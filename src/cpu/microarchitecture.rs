use crate::cpu::detect::target_architecture_uname;
use crate::schema::{Compiler, CompilerSet};
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::iter;
use std::sync::{Arc, OnceLock};

pub struct Microarchitecture {
    pub(crate) name: String,
    pub(crate) parents: Vec<Arc<Microarchitecture>>,
    pub(crate) vendor: String,
    pub(crate) features: HashSet<String>,
    pub(crate) compilers: HashMap<String, Vec<Compiler>>,
    pub(crate) generation: usize,
    pub(crate) cpu_part: Option<String>,

    // Not used in comparison
    pub(crate) ancestors: OnceLock<Vec<Arc<Microarchitecture>>>,
}

impl PartialEq<Self> for Microarchitecture {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.vendor == other.vendor
            && self.features == other.features
            && self.parents == other.parents
            && self.compilers == other.compilers
            && self.generation == other.generation
            && self.cpu_part == other.cpu_part
    }
}

impl Eq for Microarchitecture {}

impl Debug for Microarchitecture {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Microarchitecture")
            .field("name", &self.name)
            .field(
                "ancestors",
                &self
                    .ancestors()
                    .iter()
                    .map(|arch| arch.name.as_str())
                    .collect_vec(),
            )
            .field("vendor", &self.vendor)
            .field("features", &self.all_features())
            .field("compilers", &self.compilers)
            .field("generation", &self.generation)
            .field("cpu_part", &self.cpu_part)
            .finish()
    }
}

impl Microarchitecture {
    pub(crate) fn new(
        name: String,
        parents: Vec<Arc<Microarchitecture>>,
        vendor: String,
        features: HashSet<String>,
        compilers: HashMap<String, Vec<Compiler>>,
    ) -> Self {
        Microarchitecture::new_generation(name, parents, vendor, features, compilers, 0)
    }

    pub(crate) fn new_generation(
        name: String,
        parents: Vec<Arc<Microarchitecture>>,
        vendor: String,
        features: HashSet<String>,
        compilers: HashMap<String, Vec<Compiler>>,
        generation: usize,
    ) -> Self {
        Microarchitecture {
            name,
            parents,
            vendor,
            features,
            compilers,
            generation,
            cpu_part: None,
            ancestors: OnceLock::new(),
        }
    }

    /// Constructs a new generic micro architecture
    pub fn generic(name: &str) -> Microarchitecture {
        Microarchitecture::new(
            name.to_string(),
            vec![],
            "generic".to_string(),
            HashSet::new(),
            HashMap::new(),
        )
    }

    /// Returns the name of the micro architecture.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the vendor of the micro architecture
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// Returns the generation of the micro architecture.
    /// Examples of architectures with a generation are:
    /// - x86_64 = 0
    /// - x86_64_v3 = 3
    /// - power7 = 7
    /// - power10 = 10
    pub fn generation(&self) -> usize {
        self.generation
    }

    /// Returns all the known micro architectures.
    pub fn known_targets() -> &'static HashMap<String, Arc<Microarchitecture>> {
        static KNOWN_TARGETS: std::sync::OnceLock<HashMap<String, Arc<Microarchitecture>>> =
            std::sync::OnceLock::new();
        KNOWN_TARGETS.get_or_init(known_microarchitectures)
    }

    /// Returns all the ancestors of this micro architecture.
    pub fn ancestors(&self) -> &[Arc<Microarchitecture>] {
        self.ancestors.get_or_init(|| {
            let mut v = self.parents.clone();
            for parent in &self.parents {
                let new_ancestors = parent
                    .ancestors()
                    .iter()
                    .filter(|a| !v.contains(a))
                    .cloned()
                    .collect_vec();
                v.extend(new_ancestors);
            }
            v
        })
    }

    /// Returns true if the given micro architecture is an ancestor of this micro architecture.
    pub fn decendent_of(&self, parent: &Microarchitecture) -> bool {
        for p in self.parents.iter() {
            if p.as_ref() == parent || p.decendent_of(parent) {
                return true;
            }
        }
        false
    }

    /// Returns true if this micro architecture is a strict superset of the other.
    ///
    /// If a micro architecture is a strict superset of another, it means that it has all the
    /// features of the other micro architecture, and more.
    pub fn is_strict_superset(&self, other: &Microarchitecture) -> bool {
        self.is_superset(other) && self.name != other.name
    }

    /// Returns true if this micro architecture is a superset of the other.
    ///
    /// This means that the current micro architecture has at least all the features of the other
    /// micro architecture.
    fn is_superset(&self, other: &Microarchitecture) -> bool {
        let a = self.node_set();
        let b = other.node_set();
        a.is_superset(&b)
    }

    /// Returns the names of all the ancestors, including the current micro architecture name.
    ///
    /// This effectively returns all the nodes in the graph of micro architectures that are
    /// reachable from the current node. This is useful for comparing two micro architectures.
    ///
    /// See also [`Self::is_strict_superset`].
    fn node_set(&self) -> HashSet<&str> {
        iter::once(self.name.as_str())
            .chain(self.ancestors().iter().map(|a| a.name.as_str()))
            .collect()
    }

    /// Returns the architecture root, the first parent architecture that does not have a
    /// defined parent.
    ///
    /// It is assumed that all architectures have a single root.
    pub fn family(&self) -> &Self {
        match self.parents.first() {
            Some(parent) => parent.family(),
            None => self,
        }
    }

    /// Returns all features supported by this architecture.
    pub fn all_features(&self) -> HashSet<String> {
        let mut features = self.features.clone();
        for parent in &self.parents {
            features.extend(parent.all_features());
        }
        features
    }
}

#[derive(Debug)]
pub struct UnsupportedMicroarchitecture;

fn known_microarchitectures() -> HashMap<String, Arc<Microarchitecture>> {
    let mut known_targets: HashMap<String, Arc<Microarchitecture>> = HashMap::new();
    let schema = crate::schema::MicroarchitecturesSchema::schema();

    fn fill_target_from_map(
        name: &str,
        schema: &crate::schema::MicroarchitecturesSchema,
        targets: &mut HashMap<String, Arc<Microarchitecture>>,
    ) {
        let data = &schema.microarchitectures;
        let values = &data[name];
        let parent_names = &values.from;
        for parent in parent_names {
            if !targets.contains_key(parent) {
                fill_target_from_map(parent, schema, targets);
            }
        }
        let parents = parent_names
            .iter()
            .map(|parent| targets[parent].clone())
            .collect::<Vec<Arc<Microarchitecture>>>();

        let vendor = values.vendor.clone();
        let features: HashSet<String> = values.features.iter().cloned().collect();
        let compilers: HashMap<String, Vec<Compiler>> = values
            .compilers
            .as_ref()
            .map(|compilers| {
                compilers
                    .iter()
                    .map(|(vendor, set)| {
                        (
                            vendor.clone(),
                            // normalize to a sequence of compiler definitions
                            match set {
                                CompilerSet::Several(cs) => cs.clone(),
                                CompilerSet::Single(c) => vec![c.clone()],
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let generation = values.generation.unwrap_or(0);
        let cpu_part = values.cpupart.clone();

        targets.insert(
            name.to_string(),
            Arc::new(Microarchitecture {
                name: name.to_string(),
                parents,
                vendor,
                features,
                compilers,
                generation,
                cpu_part,
                ancestors: OnceLock::new(),
            }),
        );
    }

    for name in schema.microarchitectures.keys() {
        if !known_targets.contains_key(name) {
            fill_target_from_map(name, schema, &mut known_targets);
        }
    }

    if let Ok(host_platform) = target_architecture_uname() {
        known_targets
            .entry(host_platform.to_string())
            .or_insert_with(|| Arc::new(Microarchitecture::generic(&host_platform)));
    }

    known_targets
}
