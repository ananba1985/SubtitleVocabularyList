fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../.local/android-ocr");
        let libraries = root.join("install/lib");
        for name in ["libtesseract.a", "libleptonica.a", "libndk_compat.a", "libc++_shared.so"] {
            assert!(libraries.join(name).is_file(), "Android OCR libraries missing; run pnpm android:ocr first: {name}");
        }
        println!("cargo:rerun-if-changed={}",root.join("manifest.json").display());
        println!("cargo:rustc-link-search=native={}",libraries.display());
        for library in ["tesseract","leptonica","ndk_compat"] { println!("cargo:rustc-link-lib=static={library}"); }
        for library in ["c++_shared","m","dl"] { println!("cargo:rustc-link-lib=dylib={library}"); }
    }
    #[cfg(feature = "app-host")]
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
