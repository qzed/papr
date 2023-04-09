use gtk::{glib, prelude::ApplicationExtManual};

mod canvas;
mod pdf;
mod types;
mod ui;

fn main() -> glib::ExitCode {
    env_logger::init();

    let app = ui::app::App::new();
    app.run()
}
