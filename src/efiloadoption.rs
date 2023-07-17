use std::io::{Cursor, Read, Write};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::debug;
use std::fmt::{Debug, Formatter};
use enumflags2::BitFlags;
use thiserror::Error;
use std::io;
use std::iter::once;
use std::string::FromUtf16Error;
use std::ops::Range;
use bytemuck::{bytes_of, cast_slice};
use gio::glib;
use crate::efidevicepath::{DevicePathProtocolParseError, EFIDevicePathProtocol};

/// Modelled after [https://uefi.org/specs/UEFI/2.10/03_Boot_Manager.html#load-options](https://uefi.org/specs/UEFI/2.10/03_Boot_Manager.html#load-options)
#[derive(Clone, Debug, PartialEq)]
pub struct EFILoadOption {
    attributes: LoadOptionAttributes,
    file_path_list: Vec<EFIDevicePathProtocol>,
    description: String,
    optional_data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum LoadOptionParseError {
    #[error(transparent)]
    ReadError(#[from] glib::Error),
    #[error(transparent)]
    ParseError(#[from] io::Error),
    #[error(transparent)]
    FromUtf16Error(#[from] FromUtf16Error),
    #[error(transparent)]
    DevicePathProtocolParseError(#[from] DevicePathProtocolParseError),
}

impl EFILoadOption {
    pub fn parse(read: &mut impl Read) -> Result<EFILoadOption, LoadOptionParseError> {
        debug!("Beginning to parse EFILoadOption...");

        let attributes = LoadOptionAttributes::from(read.read_u32::<LittleEndian>()?);

        debug!("Parsed attributes: {:?}", attributes);

        let file_path_list_length = read.read_u16::<LittleEndian>()?;
        debug!("Parsed file path list length: {:?}", file_path_list_length);

        let description = {
            let mut description = vec![];
            loop {
                let char = read.read_u16::<LittleEndian>()?;
                if char == 0x0000 {
                    break;
                }
                description.push(char);
            }
            String::from_utf16(&description)?
        };
        debug!("Parsed description: {}", description);
        let file_path_list = {
            let mut buffer = vec![];
            buffer.resize(file_path_list_length as _, 0u8);
            read.read_exact(&mut buffer)?;

            let mut list = vec![];
            let mut read = Cursor::new(buffer);
            loop {
                let device_path = EFIDevicePathProtocol::parse(&mut read)?;
                debug!("Parsed device path protocol: {device_path:?}");
                if matches!(device_path, EFIDevicePathProtocol::End(_)) {
                    break;
                }
                list.push(device_path);
            }
            list
        };


        let optional_data = {
            let mut buf = vec![];
            read.read_to_end(&mut buf)?;
            buf
        };

        Ok(EFILoadOption { attributes, description, file_path_list, optional_data })
    }

    pub fn write(&self, write: &mut impl Write) -> io::Result<()> {
        write.write_u32::<LittleEndian>(self.attributes.bits())?;

        // concat end device path entry
        let end = EFIDevicePathProtocol::new_end_entire();
        let file_path_list_with_end = self.file_path_list.iter().chain([&end]);

        // file path list length
        write.write_u16::<LittleEndian>(file_path_list_with_end.clone().sum())?;

        {
            let description = self.description.encode_utf16().chain(once(0x0000)).collect::<Vec<_>>();
            write.write_all(cast_slice(description.as_slice()))?;
        }

        for device_path in file_path_list_with_end {
            device_path.write(write)?;
        }

        write.write_all(self.optional_data.as_slice())?;

        Ok(())
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn attributes(&self) -> &LoadOptionAttributes {
        &self.attributes
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq)]
pub struct LoadOptionAttributes(u32);

impl Debug for LoadOptionAttributes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadOptionAttributes")
            .field("flags", &self.flags())
            .field("category", &self.category())
            .finish()
    }
}

impl LoadOptionAttributes {
    const CATEGORY_MASK: u32 = 0x00001F00;

    pub fn new(flags: BitFlags<LoadOptionAttributeFlag>, category: LoadOptionCategory) -> Self {
        LoadOptionAttributes(flags.bits() | category.bits())
    }

    fn bits(&self) -> u32 {
        self.0
    }

    fn category_bits(&self) -> u32 {
        self.0 & Self::CATEGORY_MASK
    }

    pub fn category(&self) -> LoadOptionCategory {
        LoadOptionCategory(self.category_bits())
    }

    fn flag_bits(&self) -> u32 {
        self.0 & (Self::CATEGORY_MASK ^ u32::MAX)
    }

    pub fn flags(&self) -> BitFlags<LoadOptionAttributeFlag> {
        BitFlags::from_bits_truncate(self.flag_bits())
    }

    pub fn set_flags(&mut self, flags: BitFlags<LoadOptionAttributeFlag>) {
        self.0 = flags.bits() | self.category_bits();
    }

    pub fn set_category(&mut self, category: LoadOptionCategory) {
        self.0 = category.bits() | self.flag_bits()
    }
}

impl From<u32> for LoadOptionAttributes {
    fn from(value: u32) -> Self {
        LoadOptionAttributes(value)
    }
}

#[enumflags2::bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LoadOptionAttributeFlag {
    Active = 0x00000001,
    ForceReconnect = 0x00000002,
    Hidden = 0x00000008,
}

#[derive(Copy, Clone, PartialEq)]
pub struct LoadOptionCategory(u32);

impl Debug for LoadOptionCategory {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            _ if self.is_boot() => "Boot",
            _ if self.is_app() => "App",
            _ => "Other",
        })
    }
}

impl LoadOptionCategory {
    const BOOT: LoadOptionCategory = LoadOptionCategory(0x00000000);
    const APP: LoadOptionCategory = LoadOptionCategory(0x00000100);
    const RESERVED_RANGE: Range<u32> = 0x00000200..0x00002000;

    pub fn is_boot(&self) -> bool {
        *self == Self::BOOT
    }

    pub fn is_app(&self) -> bool {
        *self == Self::APP
    }

    pub fn is_reserved(&self) -> bool {
        Self::RESERVED_RANGE.contains(&self.0)
    }

    fn bits(&self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::str::FromStr;
    use byteorder::{LittleEndian, ReadBytesExt};
    use gio::glib::MainContext;
    use uuid::Uuid;
    use crate::efidevicepath::{EFIDevicePathProtocol, HardDriveDevicePath, MediaDevicePath, PartitionTableType, Signature};
    use crate::efiloadoption::{EFILoadOption, LoadOptionAttributeFlag, LoadOptionAttributes, LoadOptionCategory};

    fn equivalent_load_option() -> EFILoadOption {
        EFILoadOption {
            attributes: LoadOptionAttributes::new(LoadOptionAttributeFlag::Active.into(), LoadOptionCategory::BOOT),
            file_path_list: vec![
                EFIDevicePathProtocol::new_hard_drive_gpt(
                    1,
                    0x800,
                    0x1F4000,
                    Uuid::from_str("eba9a856-dfdd-42eb-be76-31760ae90f55").unwrap(),
                ),
                EFIDevicePathProtocol::new_file_path("EFI\\Linux\\arch-linux.efi"),
            ],
            description: "Arch Linux".to_string(),
            optional_data: vec![],
        }
    }

    #[test]
    fn test_efi_load_option_parse() {
        // copied from my personal laptop
        let boot_entry_bytes = include_bytes!("test/Boot0001-8be4df61-93ca-11d2-aa0d-00e098032b8c");
        let mut read = Cursor::new(boot_entry_bytes);
        let _efivarfs_attrs = read.read_u32::<LittleEndian>();

        let parsed = EFILoadOption::parse(&mut read).unwrap();

        let expected = equivalent_load_option();

        assert_eq!(expected, parsed);
    }

    #[test]
    fn test_efi_load_option_write() {
        let source = equivalent_load_option();
        let expected = include_bytes!("test/Boot0001-8be4df61-93ca-11d2-aa0d-00e098032b8c");

        let buffer = vec![];
        let mut write = Cursor::new(buffer);
        source.write(&mut write).unwrap();

        let buffer = write.into_inner();
        assert_eq!(&expected[4..], buffer.as_slice());
    }
}
