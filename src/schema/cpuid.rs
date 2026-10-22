use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CpuIdSchema {
    pub vendor: CpuIdProperty,
    pub highest_extension_support: CpuIdProperty,
    pub flags: Vec<CpuIdFlags>,
    #[serde(rename = "extension-flags")]
    pub extension_flags: Vec<CpuIdFlags>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CpuIdProperty {
    pub description: String,
    pub input: CpuIdInput,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CpuIdFlags {
    pub description: String,
    pub input: CpuIdInput,
    pub bits: Vec<CpuIdBits>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CpuIdBits {
    pub name: String,
    pub register: CpuRegister,
    pub bit: u8,
}

#[derive(Debug, Deserialize, Copy, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CpuRegister {
    Eax,
    Ebx,
    Ecx,
    Edx,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct CpuIdInput {
    pub eax: u32,
    pub ecx: u32,
}
