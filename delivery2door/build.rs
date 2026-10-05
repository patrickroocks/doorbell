use chrono::Local;

fn main() {
    // Get the build timestamp for AppInfo.
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", Local::now().to_rfc3339());

    // Always re-run the build script when any src file changes.
    println!("cargo:rerun-if-changed=src");
}
