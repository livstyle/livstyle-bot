//! The runtime ALSA and Pulse libraries are installed, but the unversioned
//! linker names (`libasound.so`, `libpulse.so`) come from the -dev packages.
//! Point the link at the sonames so the workbench builds without those packages.

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    for (name, soname) in [
        ("libasound.so", "libasound.so.2"),
        ("libpulse.so", "libpulse.so.0"),
    ] {
        let from = format!("/usr/lib/x86_64-linux-gnu/{soname}");
        let to = format!("{out}/{name}");
        let _ = std::fs::remove_file(&to);
        if std::path::Path::new(&from).exists() {
            std::os::unix::fs::symlink(&from, &to).ok();
        }
    }
    println!("cargo:rustc-link-search=native={out}");
}
