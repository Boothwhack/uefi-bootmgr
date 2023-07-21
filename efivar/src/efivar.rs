use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use enumflags2::BitFlags;
use thiserror::Error;
use uuid::Uuid;

const EFI_GLOBAL_VENDOR_GID: &'static str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";

fn efi_global_vendor_uuid() -> Uuid {
    Uuid::from_str(EFI_GLOBAL_VENDOR_GID).unwrap()
}

#[derive(Clone)]
pub struct VariableName {
    key: String,
    vendor: Uuid,
}

impl Debug for VariableName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VariableName")
            .field(&self.key)
            .field(&self.vendor)
            .finish()
    }
}

impl Display for VariableName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key)
    }
}

#[derive(Debug, Error)]
pub enum VariableNameFromStrError {
    #[error("input was not recognized as a variable name")]
    InvalidFormat,
    #[error("error parsing vendor uuid")]
    UuidError(#[from] uuid::Error),
}

impl FromStr for VariableName {
    type Err = VariableNameFromStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (key, vendor) = s.split_once('-').ok_or(VariableNameFromStrError::InvalidFormat)?;
        let vendor = Uuid::from_str(vendor)?;

        Ok(VariableName {
            key: key.to_owned(),
            vendor,
        })
    }
}

impl VariableName {
    pub fn new(key: String, vendor: Uuid) -> Self {
        Self {
            key,
            vendor,
        }
    }

    pub fn global_vendor_new(key: String) -> Self {
        Self::new(key, efi_global_vendor_uuid())
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn vendor(&self) -> &Uuid {
        &self.vendor
    }
}

#[derive(Clone, Debug)]
pub struct EFIVariable {
    name: VariableName,
    attributes: BitFlags<EFIVariableAttribute>,
    data: Vec<u8>,
}

impl EFIVariable {
    pub fn new(name: VariableName, attributes: BitFlags<EFIVariableAttribute>, data: Vec<u8>) -> Self {
        Self {
            name,
            attributes,
            data,
        }
    }

    pub fn name(&self) -> &VariableName {
        &self.name
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[enumflags2::bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum EFIVariableAttribute {
    NonVolatile = 0x0000000000000001,
    BootServiceAccess = 0x0000000000000002,
    RuntimeAccess = 0x0000000000000004,
    HardwareErrorRecord = 0x0000000000000008,
    AuthenticatedWriteAccess = 0x0000000000000010,
    TimeBasedAuthenticatedWriteAccess = 0x0000000000000020,
    AppendWrite = 0x0000000000000040,
}
