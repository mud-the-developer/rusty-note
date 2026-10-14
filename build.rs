use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/rusty-note.rc");
    println!("cargo:rerun-if-changed=assets/rusty-note.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let sdk = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK is required"))
                .join("Windows Kits/10")
        });
    let compiler = fs::read_dir(sdk.join("bin"))
        .expect("Windows SDK bin directory is required")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("x64/rc.exe"))
        .filter(|path| path.is_file())
        .max()
        .expect("Windows SDK x64 resource compiler is required");
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("rusty-note.res");
    let status = Command::new(compiler)
        .args(["/nologo", "/fo"])
        .arg(&resource)
        .arg("assets/rusty-note.rc")
        .status()
        .expect("Could not start the Windows resource compiler");
    assert!(status.success(), "Could not compile the Rusty Note icon");
    println!("cargo:rustc-link-arg-bin=rusty-note={}", resource.display());
}
