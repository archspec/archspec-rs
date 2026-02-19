#![allow(dead_code)]

use super::microarchitecture::{Microarchitecture, UnsupportedMicroarchitecture};
use crate::cpuid::{CpuId, CpuIdProvider, MachineCpuIdProvider};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Cursor},
    sync::Arc,
};

/// Returns the architecture as defined by the compiler.
const fn target_architecture_compiler() -> &'static str {
    // HACK: Cannot compare strings in const context, but we can compare bytes.
    match std::env::consts::ARCH.as_bytes() {
        b"powerpc64" if cfg!(target_endian = "little") => "ppc64le",
        b"powerpc64" => "ppc64",
        _ => std::env::consts::ARCH,
    }
}

/// Returns the architecture of the host machine by querying uname.
#[cfg(not(target_os = "windows"))]
pub(crate) fn target_architecture_uname() -> std::io::Result<String> {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;

    let mut utsname = MaybeUninit::zeroed();
    let r = unsafe { libc::uname(utsname.as_mut_ptr()) };
    if r != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let utsname = unsafe { utsname.assume_init() };
    let machine = unsafe { CStr::from_ptr(utsname.machine.as_ptr()) };

    Ok(machine.to_string_lossy().into_owned())
}

#[cfg(target_os = "windows")]
pub(crate) fn target_architecture_uname() -> std::io::Result<String> {
    Ok(target_architecture_compiler().to_string())
}

pub(crate) struct ProcCpuInfo {
    cpu_info: HashMap<String, String>,
}

impl ProcCpuInfo {
    pub fn from_str(contents: &str) -> Self {
        Self::from_reader(Cursor::new(contents.as_bytes()))
    }

    pub fn from_reader(reader: impl BufRead) -> Self {
        let mut cpu_info = std::collections::HashMap::new();
        for line in reader.lines() {
            let Ok(line) = line else {
                continue;
            };
            let Some((key, value)) = line.split_once(':') else {
                // If there is no seperator and info was already populated, break because we are on a
                // blank line seperating CPUs.
                if !cpu_info.is_empty() {
                    break;
                }
                continue;
            };
            cpu_info.insert(key.trim().to_string(), value.trim().to_string());
        }
        Self { cpu_info }
    }

    /// Read the contents from /proc/cpuinfo and parse it into a `ProcCpuInfo` struct.
    pub fn from_proc_info() -> std::io::Result<Self> {
        let file = std::fs::File::open("/proc/cpuinfo")?;
        Ok(Self::from_reader(BufReader::new(file)))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.cpu_info.get(key).map(String::as_str)
    }
}

/// Returns the micro architecture of a Windows machine with the specified properties.
pub(crate) fn detect_windows<C: CpuIdProvider>(
    arch: &str,
    cpuid: &C,
) -> Result<Microarchitecture, UnsupportedMicroarchitecture> {
    match arch {
        "x86_64" | "x86" => {
            let cpuid = CpuId::detect(cpuid);
            Ok(Microarchitecture {
                name: String::new(),
                parents: vec![],
                vendor: cpuid.vendor.clone(),
                features: cpuid.features.clone(),
                compilers: Default::default(),
                generation: 0,
                ancestors: Default::default(),
            })
        }
        target_arch @ ("ppc64" | "ppc64le" | "aarch64" | "riscv64") => {
            Ok(Microarchitecture::generic(target_arch))
        }
        _ => Err(UnsupportedMicroarchitecture),
    }
}

fn detect_linux(arch: &str, cpu_info: &ProcCpuInfo) -> Microarchitecture {
    match arch {
        "x86_64" => Microarchitecture {
            vendor: cpu_info.get("vendor_id").unwrap_or("generic").to_string(),
            features: cpu_info
                .get("flags")
                .unwrap_or_default()
                .split_ascii_whitespace()
                .map(|s| s.to_string())
                .collect(),
            ..Microarchitecture::generic("")
        },
        "aarch64" => {
            let vendor = if let Some(implementer) = cpu_info.get("CPU implementer") {
                // Mapping numeric codes to vendor (ARM). This list is a merge from
                // different sources:
                //
                // https://github.com/karelzak/util-linux/blob/master/sys-utils/lscpu-arm.c
                // https://developer.arm.com/docs/ddi0487/latest/arm-architecture-reference-manual-armv8-for-armv8-a-architecture-profile
                // https://github.com/gcc-mirror/gcc/blob/master/gcc/config/aarch64/aarch64-cores.def
                // https://patchwork.kernel.org/patch/10524949/
                crate::schema::MicroarchitecturesSchema::schema()
                    .conversions
                    .arm_vendors
                    .get(implementer)
                    .cloned()
                    .unwrap_or_else(|| "generic".to_string())
            } else {
                String::from("generic")
            };

            Microarchitecture {
                vendor,
                features: cpu_info
                    .get("Features")
                    .unwrap_or_default()
                    .split_ascii_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                ..Microarchitecture::generic("")
            }
        }
        "ppc64" | "ppc64le" => {
            let cpu = cpu_info.get("cpu").unwrap_or_default();
            let generation = cpu
                .strip_prefix("POWER")
                .map(|rest| {
                    rest.split_once(|c: char| !c.is_ascii_digit())
                        .map_or(rest, |(digits, _)| digits)
                })
                .and_then(|gen| gen.parse().ok())
                .unwrap_or(0);
            Microarchitecture {
                generation,
                ..Microarchitecture::generic("")
            }
        }
        "riscv64" => {
            let uarch = match cpu_info.get("uarch") {
                Some("sifive,u74-mc") => "u74mc",
                Some(uarch) => uarch,
                None => "riscv64",
            };
            Microarchitecture::generic(uarch)
        }
        _ => Microarchitecture::generic(arch),
    }
}

pub(crate) trait SysCtlProvider {
    fn sysctl(&self, name: &str) -> std::io::Result<String>;
}

#[derive(Default)]
pub(crate) struct MachineSysCtlProvider {}

impl SysCtlProvider for MachineSysCtlProvider {
    fn sysctl(&self, name: &str) -> std::io::Result<String> {
        cfg_if::cfg_if! {
            if #[cfg(target_os = "macos")] {
                use sysctl::Sysctl;
                sysctl::Ctl::new(name)
                    .and_then(|ctl| ctl.value())
                    .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
                    .map(|v| v.to_string())
            } else {
                unimplemented!("Sysctl is not implemented for this platform, requesting {name}")
            }
        }
    }
}

fn detect_macos<S: SysCtlProvider>(arch: &str, sysctl: &S) -> Microarchitecture {
    match arch {
        "x86_64" => {
            let cpu_features = sysctl
                .sysctl("machdep.cpu.features")
                .unwrap_or_default()
                .to_lowercase();
            let cpu_leaf7_features = sysctl
                .sysctl("machdep.cpu.leaf7_features")
                .unwrap_or_default()
                .to_lowercase();
            let vendor = sysctl.sysctl("machdep.cpu.vendor").unwrap_or_default();

            let mut features = cpu_features
                .split_whitespace()
                .chain(cpu_leaf7_features.split_whitespace())
                .map(|s| s.to_string())
                .collect::<HashSet<String>>();

            // Flags detected on Darwin turned to their linux counterpart.
            for (darwin_flag, linux_flag) in crate::schema::MicroarchitecturesSchema::schema()
                .conversions
                .darwin_flags
                .iter()
            {
                if features.contains(darwin_flag) {
                    features.extend(linux_flag.split_whitespace().map(|s| s.to_string()))
                }
            }

            Microarchitecture {
                vendor,
                features,
                ..Microarchitecture::generic("")
            }
        }
        _ => {
            let model = match sysctl
                .sysctl("machdep.cpu.brand_string")
                .map(|v| v.to_string().to_lowercase())
                .ok()
            {
                Some(model) if model.contains("m2") => String::from("m2"),
                Some(model) if model.contains("m1") => String::from("m1"),
                Some(model) if model.contains("apple") => String::from("m1"),
                _ => String::from("unknown"),
            };

            Microarchitecture {
                vendor: String::from("Apple"),
                ..Microarchitecture::generic(&model)
            }
        }
    }
}

fn compare_microarchitectures(a: &Microarchitecture, b: &Microarchitecture) -> Ordering {
    let ancestors_a = a.ancestors().len();
    let ancestors_b = b.ancestors().len();

    let features_a = a.features.len();
    let features_b = b.features.len();

    ancestors_a
        .cmp(&ancestors_b)
        .then(features_a.cmp(&features_b))
}

struct TargetDetector<S, C> {
    target_os: Option<String>,
    target_arch: Option<String>,
    cpu_info: Option<ProcCpuInfo>,
    cpuid_provider: C,
    sysctl_provider: S,
}

impl TargetDetector<MachineSysCtlProvider, MachineCpuIdProvider> {
    pub fn new() -> Self {
        Self {
            target_os: None,
            target_arch: None,
            cpu_info: None,
            cpuid_provider: MachineCpuIdProvider::default(),
            sysctl_provider: MachineSysCtlProvider::default(),
        }
    }
}

impl<S: SysCtlProvider, C: CpuIdProvider> TargetDetector<S, C> {
    pub fn with_sysctl_provider<O: SysCtlProvider>(
        self,
        sysctl_provider: O,
    ) -> TargetDetector<O, C> {
        TargetDetector {
            target_os: self.target_os,
            target_arch: self.target_arch,
            cpu_info: self.cpu_info,
            cpuid_provider: self.cpuid_provider,
            sysctl_provider,
        }
    }

    pub fn with_cpuid_provider<O: CpuIdProvider>(self, cpuid_provider: O) -> TargetDetector<S, O> {
        TargetDetector {
            target_os: self.target_os,
            target_arch: self.target_arch,
            cpu_info: self.cpu_info,
            cpuid_provider,
            sysctl_provider: self.sysctl_provider,
        }
    }

    pub fn with_target_os(self, target_os: &str) -> Self {
        Self {
            target_os: Some(target_os.to_string()),
            ..self
        }
    }

    pub fn with_target_arch(self, target_arch: &str) -> Self {
        Self {
            target_arch: Some(target_arch.to_string()),
            ..self
        }
    }

    pub fn with_proc_cpu_info(self, proc_cpu_info: ProcCpuInfo) -> Self {
        Self {
            cpu_info: Some(proc_cpu_info),
            ..self
        }
    }

    pub fn detect(self) -> Result<Arc<Microarchitecture>, UnsupportedMicroarchitecture> {
        let os = self.target_os.as_deref().unwrap_or(std::env::consts::OS);

        // Determine the architecture of the machine based on the operating system.
        let target_arch_uname;
        let target_arch = match (os, &self.target_arch) {
            ("linux", None) => {
                target_arch_uname =
                    target_architecture_uname().map_err(|_| UnsupportedMicroarchitecture)?;
                &target_arch_uname
            }
            ("macos", _) => {
                // On macOS, it might happen that we are on an M1 but running in Rosetta. In that
                // case uname will return "x86_64" so we need to fix that.
                if self
                    .sysctl_provider
                    .sysctl("machdep.cpu.brand_string")
                    .unwrap_or_default()
                    .contains("Apple")
                {
                    "aarch64"
                } else {
                    "x86_64"
                }
            }
            (_, Some(arch)) => arch.as_str(),
            (_, None) => target_architecture_compiler(),
        };

        // Detect the architecture based on the operating system.
        let detected_arch = match os {
            "linux" => {
                if let Some(cpu_info) = self.cpu_info.or_else(|| ProcCpuInfo::from_proc_info().ok())
                {
                    detect_linux(target_arch, &cpu_info)
                } else {
                    Microarchitecture::generic(target_arch)
                }
            }
            "macos" => detect_macos(target_arch, &self.sysctl_provider),
            "windows" => detect_windows(target_arch, &self.cpuid_provider)?,
            _ => {
                return Err(UnsupportedMicroarchitecture);
            }
        };

        // Determine compatible targets based on the architecture.
        let compatible_targets = match target_arch {
            "aarch64" => compatible_microarchitectures_for_aarch64(&detected_arch, os == "macos"),
            "ppc64" | "ppc64le" => {
                compatible_microarchitectures_for_ppc64(&detected_arch, target_arch == "ppc64le")
            }
            "riscv64" => compatible_microarchitectures_for_riscv64(&detected_arch),
            "x86_64" | "x86" => compatible_microarchitectures_for_x86_64(&detected_arch),
            _ => vec![Microarchitecture::known_targets()
                .get(target_arch)
                .ok_or(UnsupportedMicroarchitecture)?
                .clone()],
        };

        // Find the best generic candidates
        let Some(best_generic_candidate) = compatible_targets
            .iter()
            .filter(|target| target.vendor == "generic")
            .sorted_by(|a, b| compare_microarchitectures(a, b))
            .last()
        else {
            // If there is no matching generic candidate then
            return Err(UnsupportedMicroarchitecture);
        };

        // Filter the candidates to be descendant of the best generic candidate. This is to avoid that
        // the lack of a niche feature that can be disabled from e.g. BIOS prevents detection of a
        // reasonably performant architecture
        let best_candidates = compatible_targets
            .iter()
            .filter(|target| target.is_strict_superset(best_generic_candidate))
            .collect_vec();

        // Resort the matching candidates and fall back to the best generic candidate if there is no
        // matching non-generic candidate.
        Ok(best_candidates
            .into_iter()
            .sorted_by(|a, b| compare_microarchitectures(a, b))
            .last()
            .unwrap_or(best_generic_candidate)
            .clone())
    }
}

/// Detects the host micro-architecture and returns it.
pub fn host() -> Result<Arc<Microarchitecture>, UnsupportedMicroarchitecture> {
    TargetDetector::new().detect()
}

#[allow(unused)]
fn compatible_microarchitectures_for_aarch64(
    detected_info: &Microarchitecture,
    is_macos: bool,
) -> Vec<Arc<Microarchitecture>> {
    let targets = Microarchitecture::known_targets();

    // Get the root micro-architecture for aarch64.
    let Some(arch_root) = targets.get("aarch64") else {
        return vec![];
    };

    // On macOS it seems impossible to get all the CPU features with sysctl info, but for
    // ARM we can get the exact model
    let macos_model = if is_macos {
        match targets.get(&detected_info.name) {
            None => return vec![],
            model => model,
        }
    } else {
        None
    };

    // Find all targets that are decendants of the root architecture and are compatibile with the
    // detected micro-architecture.
    targets
        .values()
        .filter(|target| {
            // At the moment, it's not clear how to detect compatibility with a specific version of
            // the architecture.
            if target.vendor == "generic" && target.name != "aarch64" {
                return false;
            }

            // Must share the same architecture family and vendor.
            if arch_root.as_ref() != target.family()
                || !(target.vendor == "generic" || target.vendor == detected_info.vendor)
            {
                return false;
            }

            if let Some(macos_model) = macos_model {
                target.as_ref() == macos_model.as_ref() || macos_model.decendent_of(target)
            } else {
                target.features.is_subset(&detected_info.features)
            }
        })
        .cloned()
        .collect()
}

#[allow(unused)]
fn compatible_microarchitectures_for_ppc64(
    detected_info: &Microarchitecture,
    little_endian: bool,
) -> Vec<Arc<Microarchitecture>> {
    let targets = Microarchitecture::known_targets();

    let root_arch = if little_endian { "ppc64le" } else { "ppc64" };

    // Get the root micro-architecture.
    let Some(arch_root) = targets.get(root_arch) else {
        return vec![];
    };

    // Find all targets that are decendants of the root architecture and are compatibile with the
    // detected micro-architecture.
    targets
        .values()
        .filter(|target| {
            (target.as_ref() == arch_root.as_ref() || target.decendent_of(arch_root))
                && target.generation <= detected_info.generation
        })
        .cloned()
        .collect()
}

#[allow(unused)]
fn compatible_microarchitectures_for_x86_64(
    detected_info: &Microarchitecture,
) -> Vec<Arc<Microarchitecture>> {
    let targets = Microarchitecture::known_targets();

    // Get the root micro-architecture for x86_64.
    let Some(arch_root) = targets.get("x86_64") else {
        return vec![];
    };

    // Find all targets that are decendants of the root architecture and are compatibile with the
    // detected micro-architecture.
    targets
        .values()
        .filter(|target| {
            (target.as_ref() == arch_root.as_ref() || target.decendent_of(arch_root))
                && (target.vendor == detected_info.vendor || target.vendor == "generic")
                && target.features.is_subset(&detected_info.features)
        })
        .cloned()
        .collect()
}

#[allow(unused)]
fn compatible_microarchitectures_for_riscv64(
    detected_info: &Microarchitecture,
) -> Vec<Arc<Microarchitecture>> {
    let targets = Microarchitecture::known_targets();

    // Get the root micro-architecture for riscv64.
    let Some(arch_root) = targets.get("riscv64") else {
        return vec![];
    };

    // Find all targets that are descendants of the root architecture and are compatible with the
    // detected micro-architecture.
    targets
        .values()
        .filter(|target| {
            (target.as_ref() == arch_root.as_ref() || target.decendent_of(arch_root))
                && (target.name == detected_info.name || target.vendor == "generic")
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::cpu::detect::{ProcCpuInfo, SysCtlProvider};
    use crate::cpu::Microarchitecture;
    use crate::cpuid::{CpuIdProvider, CpuIdRegisters};
    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn check_host() {
        let host = super::host();
        eprintln!("{:#?}", &host);
        host.expect("host() should return something");
    }

    #[rstest]
    fn test_expected_target(#[files("json/tests/targets/*")] path: PathBuf) {
        // Determine the type of target test.
        let filename = path.file_name().unwrap().to_string_lossy();
        let (platform, _operating_system, target) = filename.split('-').collect_tuple().unwrap();

        let expected_target = Microarchitecture::known_targets()
            .get(target)
            .expect("missing target");

        let architecture_family = match platform {
            "darwin" => "x86_64",
            "windows" => "x86_64",
            _ => expected_target.family().name.as_str(),
        };

        // Read the contents of the file.
        let contents = std::fs::read_to_string(&path).unwrap();

        let detector = super::TargetDetector::new().with_target_arch(architecture_family);
        let detected_target = match platform {
            "linux" | "bgq" => detector
                .with_target_os("linux")
                .with_proc_cpu_info(ProcCpuInfo::from_str(&contents))
                .detect(),
            "darwin" => detector
                .with_target_os("macos")
                .with_sysctl_provider(MemorySysCtlProvider::from_str(&contents))
                .detect(),
            "windows" => detector
                .with_target_os("windows")
                .with_cpuid_provider(MockCpuIdProvider::from_str(&contents))
                .detect(),
            _ => panic!("Unsupported platform: {}", platform),
        };

        let detected_target = detected_target.expect("Failed to detect target");
        assert_eq!(detected_target.as_ref(), expected_target.as_ref());
    }

    struct MemorySysCtlProvider {
        contents: HashMap<String, String>,
    }

    impl MemorySysCtlProvider {
        pub fn from_str(data: &str) -> Self {
            let mut contents = HashMap::new();
            for line in data.lines() {
                let (key, value) = line.split_once(':').unwrap();
                contents.insert(key.trim().to_string(), value.trim().to_string());
            }
            Self { contents }
        }
    }

    impl SysCtlProvider for MemorySysCtlProvider {
        fn sysctl(&self, name: &str) -> std::io::Result<String> {
            self.contents
                .get(name)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        }
    }

    struct MockCpuIdProvider {
        contents: HashMap<(u32, u32), CpuIdRegisters>,
    }

    impl MockCpuIdProvider {
        pub fn from_str(data: &str) -> Self {
            let mut contents = HashMap::new();
            for line in data.lines() {
                let (leaf, subleaf, eax, ebx, ecx, edx) = line
                    .split(", ")
                    .map(|d| d.parse().unwrap())
                    .collect_tuple()
                    .unwrap();
                contents.insert((leaf, subleaf), CpuIdRegisters { eax, ebx, ecx, edx });
            }
            Self { contents }
        }
    }

    impl CpuIdProvider for MockCpuIdProvider {
        fn cpuid(&self, leaf: u32, subleaf: u32) -> CpuIdRegisters {
            self.contents.get(&(leaf, subleaf)).cloned().unwrap()
        }
    }
}
