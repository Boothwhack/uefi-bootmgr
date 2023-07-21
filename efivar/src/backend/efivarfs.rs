use std::io;
use std::io::Cursor;
use std::str::FromStr;
use async_trait::async_trait;
use byteorder::{LittleEndian, ReadBytesExt};
use enumflags2::BitFlags;
use futures::{stream, StreamExt, TryStreamExt};
use gio::{Cancellable, File, FileQueryInfoFlags, glib, MountMountFlags, MountOperation};
use gio::glib::Priority;
use thiserror::Error;
use crate::backend::EFIVars;
use crate::efivar::{EFIVariable, VariableName, VariableNameFromStrError};
use gio::prelude::*;

pub struct EFIVarFS {
    root: File,
}

impl EFIVarFS {
    pub async fn new_gvfs_admin() -> Result<Self, glib::Error> {
        let root = File::for_uri("admin:///sys/firmware/efi/efivars");
        root.mount_enclosing_volume_future(MountMountFlags::empty(), None::<&MountOperation>).await?;
        Ok(Self { root })
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
pub enum ReadVariableError {
    #[error("glib produced an error while reading efi variable")]
    GLibError(#[from] glib::Error),
    #[error("error reading efi variable attributes")]
    IoError(#[from] io::Error),
}

#[async_trait(? Send)]
impl EFIVars for EFIVarFS {
    type ListError = ListVariablesError;
    type ReadError = ReadVariableError;

    async fn enumerate_variables(&self) -> Result<Vec<VariableName>, Self::ListError> {
        self.root
            .enumerate_children_future("standard::name", FileQueryInfoFlags::empty(), Priority::default())
            .await?
            .into_stream(10, Priority::default())
            .map_ok(|files| stream::iter(files).map(Ok::<_, glib::Error>))
            .try_flatten()
            .map(|file| {
                let name = file?.name();
                let name = name.to_str().ok_or(ListVariablesError::from(VariableNameFromStrError::InvalidFormat))?;
                Ok(VariableName::from_str(name)?)
            })
            .try_collect::<Vec<_>>()
            .await
    }

    async fn read_variable(&self, name: &VariableName) -> Option<Result<EFIVariable, Self::ReadError>> {
        let file = self.root.resolve_relative_path(format!("{}-{:x}", name.key(), name.vendor()).as_str());
        if !file.query_exists(None::<&Cancellable>) {
            return None;
        }

        async fn read_existing_variable(file: File, name: &VariableName) -> Result<EFIVariable, ReadVariableError> {
            let size = file.query_info_future("standard::size", FileQueryInfoFlags::empty(), Priority::default())
                .await?
                .size() as usize;
            let buffer = vec![0u8; size];
            match file.read_future(Priority::default()).await?
                .read_all_future(buffer, Priority::default()).await.map_err(|(_, err)| err)? {
                (_, _, Some(err)) => Err(err.into()),
                (buffer, _, None) => {
                    let attributes = Cursor::new(&buffer[0..4]).read_u32::<LittleEndian>()?;
                    let attributes = BitFlags::from_bits_truncate(attributes);
                    Ok(EFIVariable::new(name.clone(), attributes, buffer[4..].to_vec()))
                }
            }
        }

        Some(read_existing_variable(file, name).await)
    }
}
