use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let args: &[&'static str] = &[
        "-fretain-comments-from-system-headers",
        "-fparse-all-comments",
        #[cfg(feature = "v8")] "-DPDF_ENABLE_V8",
        #[cfg(feature = "xfa")] "-DPDF_ENABLE_XFA",
        #[cfg(feature = "skia")] "-D_SKIA_SUPPORT_",
    ];

    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .dynamic_library_name("libpdfium")
        .dynamic_link_require_all(cfg!(feature = "dylib-require-all"))
        .clang_args(args)
        .generate_comments(true)
        .generate()
        .expect("Unable to generate pdfium bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
