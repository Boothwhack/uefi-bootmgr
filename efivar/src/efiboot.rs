use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::io::Cursor;
use std::sync::OnceLock;
use async_trait::async_trait;
use bytemuck::cast_slice;
use futures::{stream, StreamExt, TryStreamExt};
use log::debug;
use regex::Regex;
use thiserror::Error;
use crate::backend::EFIVars;
use crate::efiloadoption::{EFILoadOption, LoadOptionAttributeFlag, LoadOptionParseError};
use crate::efivar::{EFIVariable, VariableName};

static BOOT_KEY_REGEX: OnceLock<Regex> = OnceLock::new();

fn boot_key_regex() -> &'static Regex {
    BOOT_KEY_REGEX.get_or_init(|| Regex::new(r"^Boot([0-9A-F]{4})$").unwrap())
}

#[derive(Debug, Error)]
#[error("error parsing Boot{id:04X}: {source}")]
pub struct BootEntryParseError {
    id: u16,
    source: LoadOptionParseError,
}

impl BootEntryParseError {
    pub fn new(id: u16, source: LoadOptionParseError) -> Self {
        Self { id, source }
    }
}

#[derive(Clone)]
pub struct BootEntry {
    id: u16,
    load_option: EFILoadOption,
}

impl Debug for BootEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Boot{:04X}", self.id))
            .field("load_option", &self.load_option)
            .finish()
    }
}

impl BootEntry {
    pub fn description(&self) -> &str {
        self.load_option.description()
    }

    pub fn is_active(&self) -> bool {
        self.load_option.attributes().flags().contains(LoadOptionAttributeFlag::Active)
    }
}

pub struct BootOrder {
    order: Vec<u16>,
}

impl Debug for BootOrder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.order.iter().map(|id| format!("{:04X}", id)))
            .finish()
    }
}

impl BootOrder {
    pub fn iter(&self) -> impl Iterator<Item=&u16> {
        self.order.iter()
    }
}

pub struct OrderedBootEntries {
    entries: HashMap<u16, BootEntry>,
    order: BootOrder,
}

impl OrderedBootEntries {
    pub fn iter(&self) -> impl Iterator<Item=&BootEntry> {
        self.order.iter().filter_map(move |id| self.entries.get(id))
    }
}

#[derive(Debug, Error)]
pub enum ListBootEntriesError<E: EFIVars> {
    #[error("error listing efi variables: {0}")]
    ListVariablesError(#[source] E::ListError),
    #[error("error reading efi boot entry: {0}")]
    ReadBootEntryError(#[from] ReadBootEntryError<E>),
    #[error("failed to locate BootOrder variable")]
    NoBootOrderVariableError,
    #[error("error reading BootOrder variable: {0}")]
    ReadBootOrderVariableError(#[source] E::ReadError),
}

#[derive(Debug, Error)]
pub enum ReadBootEntryError<E: EFIVars> {
    #[error("error reading efi boot entry variable: {0}")]
    ReadVariableError(#[source] E::ReadError),
    #[error(transparent)]
    ParseError(#[from] BootEntryParseError),
}

#[async_trait(? Send)]
pub trait ListBootEntriesExt: EFIVars + Sized {
    async fn read_boot_entry(&self, name: &VariableName) -> Option<Result<BootEntry, ReadBootEntryError<Self>>>;

    async fn list_boot_entries(&self) -> Result<OrderedBootEntries, ListBootEntriesError<Self>>;
}

#[async_trait(? Send)]
impl<E> ListBootEntriesExt for E
    where E: EFIVars {
    async fn read_boot_entry(&self, name: &VariableName) -> Option<Result<BootEntry, ReadBootEntryError<E>>> {
        use ReadBootEntryError::*;

        let id = boot_key_regex()
            .captures(name.key())?
            .get(1)?
            .as_str();
        let id = u16::from_str_radix(id, 16).ok()?;

        debug!("Reading Boot{:04X} variable...", id);

        fn parse_entry<E: EFIVars>(id: u16, variable: EFIVariable) -> Result<BootEntry, ReadBootEntryError<E>> {
            let mut read = Cursor::new(variable.data());

            EFILoadOption::parse(&mut read)
                .map(|load_option| BootEntry { id, load_option })
                .map_err(|err| BootEntryParseError::new(id, err).into())
        }

        Some(match self.read_variable(name).await? {
            Ok(variable) => parse_entry(id, variable),
            Err(err) => Err(ReadVariableError::<E>(err))
        })
    }

    async fn list_boot_entries(&self) -> Result<OrderedBootEntries, ListBootEntriesError<Self>> {
        use ListBootEntriesError::*;

        let order = self.read_variable(&VariableName::global_vendor_new("BootOrder".to_owned())).await
            .ok_or(NoBootOrderVariableError)?.map_err(ReadBootOrderVariableError)?;
        let order = BootOrder { order: cast_slice(order.data()).to_vec() };

        debug!("Read boot order: {:?}", order);

        let variables = self.enumerate_variables().await.map_err(ListVariablesError)?;
        let entries = stream::iter(variables).filter_map(|name| async move { self.read_boot_entry(&name).await })
            .try_collect::<Vec<_>>().await?;

        let entries = entries.into_iter().map(|entry| (entry.id, entry)).collect();

        Ok(OrderedBootEntries { order, entries })
    }
}
