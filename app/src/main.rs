use gtk::{glib, prelude::ApplicationExtManual};

mod types;
mod ui;

fn main() -> glib::ExitCode {
    let app = ui::app::PaperApp::new();
    app.run()
}
