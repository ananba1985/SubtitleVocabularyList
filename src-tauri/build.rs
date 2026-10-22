fn main() {
    #[cfg(feature = "desktop")]
    tauri_build::build();
    // IPC-enabled native checks have no application binary resources/manifest.
    #[cfg(all(feature = "ipc-types", windows, target_env = "msvc"))]
    {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
