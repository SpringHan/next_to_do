//! Next To Do: an always-on-top todo panel for Wayland compositors such as Niri.

mod config;
mod model;
mod ui;

use std::process::ExitCode;

use chrono::Local;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("waybar") => {
            println!("{}", waybar_text());
            ExitCode::SUCCESS
        }
        Some("-h" | "--help" | "help") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("next_to_do: unknown command `{other}`\n");
            print_help();
            ExitCode::FAILURE
        }
        None => {
            ui::run();
            ExitCode::SUCCESS
        }
    }
}

/// `order-bounded:time-bounded` counts of today's unfinished items.
fn waybar_text() -> String {
    let data = model::Data::load(&config::data_path());
    let (order, time) = data.counts(Local::now().date_naive());
    format!("{order}:{time}")
}

fn print_help() {
    println!(
        "next_to_do {version}

USAGE:
    next_to_do            open the todo panel
    next_to_do waybar     print `order-bounded:time-bounded` for today
    next_to_do help       show this text

FILES:
    {data}
    {config}

The panel is driven by single, configurable keys (see `keys` in the config
file). Press `Escape` to hide it and `q` to quit.",
        version = env!("CARGO_PKG_VERSION"),
        data = config::data_path().display(),
        config = config::config_path().display(),
    );
}
