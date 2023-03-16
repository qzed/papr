use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let defines: &[&'static str] = &[
        #[cfg(feature = "v8")]
        "PDF_ENABLE_V8",

        #[cfg(feature = "xfa")]
        "PDF_ENABLE_XFA",

        #[cfg(feature = "skia")]
        "_SKIA_SUPPORT_",
    ];

    println!("cargo:rerun-if-changed=wrapper.h");

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .dynamic_library_name("libpdfium")
        .dynamic_link_require_all(cfg!(feature = "dylib-require-all"));

    for d in defines {
        builder = builder.clang_arg(format!("-D{d}"));
    }

    builder
        .generate()
        .expect("Unable to generate pdfium bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
