use std::error::Error;
use async_trait::async_trait;
use futures::{stream, StreamExt};
use crate::backend::efivarfs::EFIVarFS;
use crate::efivar::{EFIVariable, VariableName};

pub mod efivarfs;

#[async_trait(? Send)]
pub trait EFIVars {
    type ListError: 'static + Error;
    type ReadError: 'static + Error;

    async fn enumerate_variables(&self) -> Result<Vec<VariableName>, Self::ListError>;

    async fn read_variable(&self, name: &VariableName) -> Option<Result<EFIVariable, Self::ReadError>>;

    async fn list_variables(&self) -> Result<Vec<Result<EFIVariable, (VariableName, Self::ReadError)>>, Self::ListError> {
        let names = self.enumerate_variables().await?;

        let variables = stream::iter(names.into_iter())
            .filter_map(|name| async move {
                self.read_variable(&name).await
                    .map(|result| result.map_err(|err| (name, err)))
            })
            .collect::<Vec<_>>().await;

        Ok(variables)
    }
}

#[cfg(target_os = "linux")]
pub async fn platform_backend() -> Result<EFIVarFS, gio::glib::Error> {
    EFIVarFS::new_gvfs_admin().await
}
