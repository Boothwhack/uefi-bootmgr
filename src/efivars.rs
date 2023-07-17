use std::path::PathBuf;
use std::str::FromStr;
use futures::prelude::*;
use gio::{File, FileInfo, FileQueryInfoFlags, glib, MountMountFlags, MountOperation, MountUnmountFlags};
use gio::glib::Priority;
use gio::prelude::*;
use thiserror::Error;
use uuid::Uuid;
use crate::efiboot::BootEntry;
use crate::efiloadoption::LoadOptionParseError;

#[allow(non_upper_case_globals)]
const NoneMountOperation: Option<&MountOperation> = None;

pub struct EFIVars {
    dir: File,
}

impl EFIVars {
    pub async fn new(dir: File) -> EFIVars {
        if let Err(err) = dir.mount_enclosing_volume_future(MountMountFlags::NONE, NoneMountOperation).await {
            eprintln!("mounting admin volume: {}", err);
        }

        EFIVars { dir }
    }

    pub async fn finish(self) {
        println!("Unmounting...");
        let _ignored_error = self.dir
            .unmount_mountable_with_operation_future(MountUnmountFlags::NONE, NoneMountOperation)
            .await;
    }

    pub async fn variables(&self) -> impl Stream<Item=Result<EFIVariableInfo, ListVariablesError>> {
        // enumerate children in chunks of 10
        self.dir.enumerate_children_future("*", FileQueryInfoFlags::NONE, Priority::default())
            .await
            .expect("enumerate children")
            .into_stream(10, Priority::default())
            // flatten each vec into a stream
            .map_ok(|files| stream::iter(files).map(Ok::<_, glib::Error>))
            .try_flatten()
            // parse file info into efi variables
            .map(|file| EFIVariableInfo::try_from(file?).map_err(ListVariablesError::from))
    }

    pub fn directory(&self) -> &File {
        &self.dir
    }

    pub async fn boot_entries(&self) -> Result<Vec<BootEntry>, ListBootEntriesError> {
        self.variables().await
            .map_err(|err| err.into())
            .try_filter_map(|var| async move {
                match BootEntry::parse(&var, &self.dir).await {
                    Some(Ok(entry)) => Ok(Some(entry)),
                    Some(Err(err)) => Err(err.into()),
                    None => Ok(None),
                }
            })
            .try_collect::<Vec<_>>().await
    }
}

#[derive(Debug, Error)]
pub enum ListVariablesError {
    #[error("glib produced an error while enumerating efivars directory")]
    GLibError(#[from] glib::Error),
    #[error("error while parsing efi variable name")]
    NameError(#[from] VariableNameFromStrError),
}

#[derive(Debug, Error)]
pub enum ListBootEntriesError {
    #[error(transparent)]
    ListVariablesError(#[from] ListVariablesError),
    #[error("failed to parse boot entry variable")]
    LoadOptionParseError(#[from] LoadOptionParseError),
}

pub struct EFIVariableInfo {
    name: VariableName,
    file_info: FileInfo,
}

impl TryFrom<FileInfo> for EFIVariableInfo {
    type Error = VariableNameFromStrError;

    fn try_from(file_info: FileInfo) -> Result<Self, Self::Error> {
        let name = file_info.name();
        let name = name.to_str().ok_or(VariableNameFromStrError::InvalidFormat)?;
        let name = VariableName::from_str(name)?;
        Ok(EFIVariableInfo { name, file_info })
    }
}

impl EFIVariableInfo {
    pub fn key(&self) -> &str {
        self.name.key()
    }

    pub fn name(&self) -> PathBuf {
        self.file_info.name()
    }

    pub fn file_info(&self) -> &FileInfo {
        &self.file_info
    }
}

pub struct VariableName {
    key: String,
    vendor: Uuid,
}

impl VariableName {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn vendor(&self) -> &Uuid {
        &self.vendor
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
