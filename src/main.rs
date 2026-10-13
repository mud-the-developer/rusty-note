#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    use rusty_note::{
        native::{self, Options},
        store,
    };
    use std::path::PathBuf;
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut options = Options {
        notes_dir: store::default_notes_dir(),
        state_file: base.join("RustyNote/state.json"),
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--notes-dir" => {
                options.notes_dir = PathBuf::from(args.next().expect("--notes-dir needs a path"))
            }
            "--state-file" => {
                options.state_file = PathBuf::from(args.next().expect("--state-file needs a path"))
            }
            "--smoke-output" => {
                options.smoke_output = Some(PathBuf::from(
                    args.next().expect("--smoke-output needs a path"),
                ))
            }
            "--metrics-file" => {
                options.metrics_file = Some(PathBuf::from(
                    args.next().expect("--metrics-file needs a path"),
                ))
            }
            "--light" => options.light = true,
            "--reduced-motion" => options.reduced_motion = true,
            _ => {
                eprintln!("Unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    if let Err(error) = native::run(options) {
        eprintln!("Rusty Note: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("The native UI currently targets Windows. The core layout/tests are portable.");
}
