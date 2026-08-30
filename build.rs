use std::env;
use std::path::Path;

/// Embeds `app.manifest` into the executable.
///
/// The MSVC linker can embed a manifest directly, which avoids depending on a resource-compiler
/// crate for what is two linker arguments.
fn main() {
    println!("cargo::rerun-if-changed=app.manifest");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let manifest = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("app.manifest");
    println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}
