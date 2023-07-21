mod ui;

use adw::ApplicationWindow;
use adw::gtk::Application;
use adw::prelude::*;

fn run(app: &Application) {
    let container = ui::app::main_window();

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
