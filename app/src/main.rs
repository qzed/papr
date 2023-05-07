#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use gtk::{glib, prelude::ApplicationExtManual};

mod core;
mod types;
mod ui;

fn main() -> glib::ExitCode {
    // set up logging
    tracing_subscriber::fmt::init();

    // run application
    let app = ui::app::App::new();
    app.run()
}
