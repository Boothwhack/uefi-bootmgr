use adw::prelude::*;
use adw::gtk::{Align, Box, Label, ListBox, Orientation, SelectionMode};
use adw::{ActionRow, Clamp, HeaderBar, StatusPage, WindowTitle};
use adw::glib::MainContext;
use efivar::backend::{EFIVars, platform_backend};
use efivar::efiboot::ListBootEntriesExt;

pub fn main_window() -> Box {
    let container = Box::new(Orientation::Vertical, 0);
    container.append(&HeaderBar::builder()
        .title_widget(&WindowTitle::new("UEFI Boot Manager", ""))
        .build());

    let content = Box::new(Orientation::Vertical, 10);
    let clamp = Clamp::builder().maximum_size(320).child(&content).build();

    container.append(&clamp);

    {
        let content = content.clone();
        MainContext::default().spawn_local(async move {
            match platform_backend().await {
                Ok(efivars) => main_page(efivars, content).await,
                Err(err) => {
                    content.append(&StatusPage::builder()
                        .description(format!("<b>Failed to initialize EFI backend</b>\r\r{}", err))
                        .icon_name("dialog-warning-symbolic")
                        .build());
                }
            }
        });
    }

    container
}

async fn main_page(efivars: impl EFIVars, content: Box) {
    match efivars.list_boot_entries().await {
        Ok(entries) => {
            content.append(&Label::builder()
                .label("Boot Entries")
                .halign(Align::Start)
                .css_classes(["heading"])
                .margin_top(10)
                .build());
            let list = ListBox::builder()
                .selection_mode(SelectionMode::None)
                .css_classes(vec!["boxed-list"])
                .build();
            content.append(&list);

            for entry in entries.iter() {
                list.append(&ActionRow::builder()
                    .title(entry.description())
                    .subtitle(if entry.is_active() { "Active" } else { "Inactive" })
                    .build());
            }
        },
        Err(err) => {
            content.append(&StatusPage::builder()
                .description(format!("<b>Failed to list EFI boot entries</b>\r\r{}", err))
                .icon_name("dialog-warning-symbolic")
                .build());
        }
    }

    /*match efivars.list_variables().await {
        Ok(variables) => {
            content.append(&Label::builder()
                .label("Boot Entries")
                .halign(Align::Start)
                .css_classes(["heading"])
                .margin_top(10)
                .build());
            let list = ListBox::builder()
                .selection_mode(SelectionMode::None)
                .css_classes(vec!["boxed-list"])
                .build();
            content.append(&list);

            for entry in variables.into_iter().filter_map(Result::ok)
                .filter_map(|var| BootEntry::parse(&var))
                .filter_map(Result::ok) {
                list.append(&ActionRow::builder()
                    .title(entry.description())
                    .subtitle(if entry.is_active() { "Active" } else { "Inactive" })
                    .build());
            }
        }
        Err(err) => {
            content.append(&StatusPage::builder()
                .description(format!("<b>Failed to list EFI variables</b>\r\r{}", err))
                .icon_name("dialog-warning-symbolic")
                .build());
        }
    }*/
}
