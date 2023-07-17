mod efivars;
mod efiboot;
mod efidevicepath;
mod efiloadoption;

use adw::{ActionRow, ApplicationWindow, Clamp, HeaderBar, WindowTitle};
use adw::glib::MainContext;
use adw::gtk::{Application, Box, Label, ListBox, Orientation, SelectionMode};
use adw::prelude::*;
use gio::File;
use crate::efivars::EFIVars;

fn run(app: &Application) {
    let container = Box::new(Orientation::Vertical, 0);
    container.append(&HeaderBar::builder()
        .title_widget(&WindowTitle::new("UEFI Boot Manager", ""))
        .build());

    let content = Box::new(Orientation::Vertical, 10);
    let clamp = Clamp::builder().maximum_size(320).child(&content).build();

    container.append(&clamp);

    content.append(&Label::builder().label("Boot Entries").css_classes(["heading"]).build());

    {
        let content = content.clone();
        MainContext::default().spawn_local(async move {
            let directory = File::for_uri("admin:///sys/firmware/efi/efivars");
            let vars = EFIVars::new(directory.clone()).await;

            match vars.boot_entries().await {
                Ok(entries) => {
                    let list = ListBox::builder().selection_mode(SelectionMode::None).css_classes(["boxed-list"]).build();
                    content.append(&list);

                    for entry in entries {
                        list.append(&ActionRow::builder()
                            .title(entry.description())
                            .subtitle(if entry.is_active() { "Active" } else { "Inactive" })
                            .build());
                    }
                }
                Err(_) => {}
            }

            vars.finish().await;
        });
    }

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(500)
        .default_height(400)
        .name("UEFI Boot Manager")
        .content(&container)
        .build();

    window.show();
}

fn main() {
    env_logger::builder().target(env_logger::Target::Stdout).init();

    let application = Application::builder()
        .application_id("net.boothwhack.UEFIBootMgr")
        .build();

    application.connect_startup(|_| {
        adw::init().expect("initialize libadwaita");
    });
    application.connect_activate(run);

    application.run();
}
