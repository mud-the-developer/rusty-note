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
        state_file: base.join("RustyNote/sticker-state.json"),
        inline_edit: true,
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    let mut explicit_mode: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--notes-dir" => {
                options.notes_dir = PathBuf::from(args.next().expect("--notes-dir needs a path"));
                options.explicit_notes_dir = true;
            }
            "--state-file" => {
                options.state_file = PathBuf::from(args.next().expect("--state-file needs a path"))
            }
            "--smoke-output" => {
                options.smoke_output = Some(PathBuf::from(
                    args.next().expect("--smoke-output needs a path"),
                ))
            }
            "--smoke-drop-file" => {
                options.smoke_drop_file = Some(PathBuf::from(
                    args.next().expect("--smoke-drop-file needs a path"),
                ))
            }
            "--metrics-file" => {
                options.metrics_file = Some(PathBuf::from(
                    args.next().expect("--metrics-file needs a path"),
                ))
            }
            mode @ ("--editor" | "--inline-edit" | "--read-only") => {
                if explicit_mode.as_deref().is_some_and(|old| old != mode) {
                    eprintln!("Choose only one editor/view mode");
                    std::process::exit(2);
                }
                explicit_mode = Some(mode.to_owned());
                options.editor_mode = mode == "--editor";
                options.inline_edit = mode == "--inline-edit";
            }
            "--light" => options.light = true,
            "--reduced-motion" => options.reduced_motion = true,
            _ if !arg.starts_with('-') && options.file.is_none() => {
                options.file = Some(PathBuf::from(arg))
            }
            _ => {
                eprintln!("Unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    if options.state_file == base.join("RustyNote/sticker-state.json") {
        if options.editor_mode {
            options.state_file = base.join("RustyNote/state.json");
        } else if options.inline_edit {
            options.state_file = base.join("RustyNote/inline-state.json");
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
