use gtk::{glib, prelude::ApplicationExtManual};

mod canvas;
mod pdf;
mod types;
mod ui;

fn main() -> glib::ExitCode {
    let app = ui::app::App::new();
    app.run()
}
