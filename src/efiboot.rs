use std::fmt::{Debug, Formatter};
use std::io::Cursor;
use std::str::FromStr;
use std::sync::OnceLock;
use adw::glib::Priority;
use byteorder::{LittleEndian, ReadBytesExt};
use enumflags2::BitFlags;
use gio::File;
use gio::prelude::*;
use log::debug;
use regex::Regex;
use uuid::Uuid;
use crate::efiloadoption::{EFILoadOption, LoadOptionAttributeFlag, LoadOptionParseError};
use crate::efivars::{EFIVariableAttribute, EFIVariableInfo};

const EFI_VENDOR_GID: &'static str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";

fn efi_vendor_uuid() -> Uuid {
    Uuid::from_str(EFI_VENDOR_GID).unwrap()
}

static BOOT_EFIVAR_REGEX: OnceLock<Regex> = OnceLock::new();

fn boot_efivar_regex() -> &'static Regex {
    BOOT_EFIVAR_REGEX.get_or_init(|| Regex::new(r"^Boot([0-9A-F]{4})$").unwrap())
}

#[derive(Clone)]
pub struct BootEntry {
    id: u16,
    attributes: BitFlags<EFIVariableAttribute>,
    load_option: EFILoadOption,
}

impl Debug for BootEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootEntry")
            .field("id", &format!("{:04X}", self.id))
            .field("attributes", &self.attributes)
            .field("load_option", &self.load_option)
            .finish()
    }
}

impl BootEntry {
    pub async fn parse(variable: &EFIVariableInfo, dir: &File) -> Option<Result<BootEntry, LoadOptionParseError>> {
        let id = boot_efivar_regex()
            .captures(variable.key())?
            .get(1)?
            .as_str();
        let id = u16::from_str_radix(id, 16).ok()?;

        let file = dir.resolve_relative_path(variable.name());
        let input_stream = file.read_future(Priority::default()).await.ok()?;

        let buffer = {
            let mut buffer = vec![];
            buffer.resize(variable.file_info().size() as _, 0u8);
            buffer
        };

        let buffer = match input_stream.read_all_future(buffer, Priority::default()).await {
            Err((_, err)) => return Some(Err(err.into())),
            Ok((_, _, Some(err))) => return Some(Err(err.into())),
            Ok((mut buffer, bytes_read, None)) => {
                buffer.resize(bytes_read, 0u8);
                buffer
            }
        };
        let mut read = Cursor::new(buffer);
        let attributes = match read.read_u32::<LittleEndian>() {
            Ok(v) => v,
            Err(err) => return Some(Err(err.into())),
        };
        let attributes = BitFlags::from_bits_truncate(attributes);

        debug!("Boot{:04X} attributes: {:?}", id, attributes);

        let load_option = match EFILoadOption::parse(&mut read) {
            Ok(value) => value,
            Err(err) => return Some(Err(err.into())),
        };

        Some(Ok(BootEntry { id, attributes, load_option }))
    }

    pub fn description(&self) -> &str {
        self.load_option.description()
    }

    pub fn is_active(&self) -> bool {
        self.load_option.attributes().flags().contains(LoadOptionAttributeFlag::Active)
    }
}
