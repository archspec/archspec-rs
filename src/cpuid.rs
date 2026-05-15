#![allow(dead_code)]

use crate::schema::{CpuIdSchema, CpuRegister};
use itertools::chain;
use std::collections::HashSet;
use std::ffi::CStr;

pub(crate) trait CpuIdProvider {
    fn cpuid(&self, leaf: u32, sub_leaf: u32) -> CpuIdRegisters;
}

#[derive(Debug, Clone)]
pub(crate) struct CpuIdRegisters {
    /// EAX register.
    pub eax: u32,
    /// EBX register.
    pub ebx: u32,
    /// ECX register.
    pub ecx: u32,
    /// EDX register.
    pub edx: u32,
}

#[cfg(target_arch = "x86_64")]
impl From<std::arch::x86_64::CpuidResult> for CpuIdRegisters {
    fn from(value: std::arch::x86_64::CpuidResult) -> Self {
        Self {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }
}

#[cfg(target_arch = "x86")]
impl From<std::arch::x86::CpuidResult> for CpuIdRegisters {
    fn from(value: std::arch::x86::CpuidResult) -> Self {
        Self {
            eax: value.eax,
            ebx: value.ebx,
            ecx: value.ecx,
            edx: value.edx,
        }
    }
}

/// Default implementation of the `CpuidProvider` trait. This implementation uses the
/// [`__cpuid_count`] intrinsic to read actual CPUID information.
///
/// This implementation is only available on x86 and x86_64 architectures.
#[derive(Default)]
pub(crate) struct MachineCpuIdProvider {}

impl CpuIdProvider for MachineCpuIdProvider {
    fn cpuid(&self, leaf: u32, sub_leaf: u32) -> CpuIdRegisters {
        // `__cpuid_count` is `unsafe` on MSRV but became safe in a later Rust release; allow
        // the resulting `unused_unsafe` warning so the same code builds on both.
        #[allow(unused_unsafe)]
        {
            cfg_if::cfg_if! {
                if #[cfg(target_arch = "x86_64")] {
                    unsafe { std::arch::x86_64::__cpuid_count(leaf, sub_leaf).into() }
                } else if #[cfg(target_arch = "x86")] {
                    unsafe { std::arch::x86::__cpuid_count(leaf, sub_leaf).into() }
                } else {
                    unimplemented!("Unsupported architecture for CPUID instruction ({leaf} {sub_leaf})")
                }
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct CpuId {
    /// The vendor name of the CPU
    pub vendor: String,

    /// Optional brand name of the CPU
    pub brand: Option<String>,

    /// The supported features of the CPU.
    pub features: HashSet<String>,
}

impl CpuId {
    pub fn detect<P: CpuIdProvider>(provider: &P) -> Self {
        let schema = CpuIdSchema::schema();

        // Read the vendor information
        let registers = provider.cpuid(schema.vendor.input.eax, schema.vendor.input.ecx);
        let highest_basic_support = registers.eax;
        let vendor_bytes: Vec<_> = chain!(
            registers.ebx.to_le_bytes(),
            registers.edx.to_le_bytes(),
            registers.ecx.to_le_bytes()
        )
        .collect();
        let vendor = String::from_utf8_lossy(&vendor_bytes).into_owned();

        // Read the highest_extension_support
        let registers = provider.cpuid(
            schema.highest_extension_support.input.eax,
            schema.highest_extension_support.input.ecx,
        );
        let highest_extension_support = registers.eax;

        // Read feature flags
        let mut features = HashSet::new();
        let supported_flags = schema
            .flags
            .iter()
            .filter(|flags| flags.input.eax <= highest_basic_support);
        let supported_extensions = schema
            .extension_flags
            .iter()
            .filter(|flags| flags.input.eax <= highest_extension_support);
        for flags in supported_flags.chain(supported_extensions) {
            let registers = provider.cpuid(flags.input.eax, flags.input.ecx);
            for bits in &flags.bits {
                let register = match bits.register {
                    CpuRegister::Eax => registers.eax,
                    CpuRegister::Ebx => registers.ebx,
                    CpuRegister::Ecx => registers.ecx,
                    CpuRegister::Edx => registers.edx,
                };
                if register & (1 << bits.bit) != 0 {
                    features.insert(bits.name.clone());
                }
            }
        }

        // Read brand name if supported.
        let brand = if highest_extension_support >= 0x80000004 {
            let registers = (
                provider.cpuid(0x80000002, 0),
                provider.cpuid(0x80000003, 0),
                provider.cpuid(0x80000004, 0),
            );

            let vendor_bytes: Vec<_> = chain!(
                registers.0.eax.to_le_bytes(),
                registers.0.ebx.to_le_bytes(),
                registers.0.ecx.to_le_bytes(),
                registers.0.edx.to_le_bytes(),
                registers.1.eax.to_le_bytes(),
                registers.1.ebx.to_le_bytes(),
                registers.1.ecx.to_le_bytes(),
                registers.1.edx.to_le_bytes(),
                registers.2.eax.to_le_bytes(),
                registers.2.ebx.to_le_bytes(),
                registers.2.ecx.to_le_bytes(),
                registers.2.edx.to_le_bytes(),
            )
            .collect();
            let brand_string = match CStr::from_bytes_until_nul(&vendor_bytes) {
                Ok(cstr) => cstr.to_string_lossy(),
                Err(_) => String::from_utf8_lossy(&vendor_bytes),
            };
            Some(brand_string.trim().to_string())
        } else {
            None
        };

        Self {
            vendor,
            features,
            brand,
        }
    }
}
