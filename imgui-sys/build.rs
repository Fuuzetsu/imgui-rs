#![allow(dead_code)]

use std::fs;
use std::io;
use std::path::Path;

const DEFINES: &[(&str, Option<&str>)] = &[
    // Rust `char` is a unicode scalar value, e.g. 32 bits.
    ("IMGUI_USE_WCHAR32", None),
    // Disabled due to linking issues
    ("CIMGUI_NO_EXPORT", None),
    ("IMGUI_DISABLE_WIN32_FUNCTIONS", None),
    ("IMGUI_DISABLE_OSX_FUNCTIONS", None),
];

fn assert_file_exists(path: &std::path::Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
            panic!(
                "Can't access {}. Did you forget to fetch git submodules?",
                path.display()
            );
        }
        Err(e) => Err(e),
    }
}

fn main() -> io::Result<()> {
    // OUT_DIR is always set: either by cargo or whatever build system is
    // mimicking it.
    //
    // It's also the default for `cc::Build` though we set it anyway to ensure
    // no divergence in the future.
    let dst = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // A directory in OUT_DIR where we'll put the imgui sources. Crucially, this
    // means that this can point _outside_ of the build directory! Some systems
    // build in a sanbox in a transient build directory but set OUT_DIR to
    // somewhere permanent/immovable. If we don't do this, the below
    // cargo:THIRD_PARTY will point at the possibly transient build directory
    // and will cause anyone depending on DEP_IMGUI_THIRD_PARTY to fail.
    //
    // You can see other -sys crates do this as well, such as libz-sys.
    let dst_sources = dst.join("include");
    // Poor-man's recursive copy.
    fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(&dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                std::fs::copy(entry.path(), &dst.join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_dir_all(&manifest_dir.join("third-party"), &dst_sources).unwrap();

    println!("cargo:THIRD_PARTY={}", dst_sources.display());
    for (key, value) in DEFINES.iter() {
        println!("cargo:DEFINE_{}={}", key, value.unwrap_or(""));
    }
    if std::env::var_os("CARGO_FEATURE_WASM").is_none() {
        // Check submodule status. (Anything else should be a compile error in
        // the C code).
        assert_file_exists(&dst_sources.join("cimgui.cpp"))?;
        assert_file_exists(&dst_sources.join("imgui/imgui.cpp"))?;

        let mut build = cc::Build::new();

        build.cpp(true);
        for (key, value) in DEFINES.iter() {
            build.define(key, *value);
        }

        // Freetype font rasterizer feature
        #[cfg(feature = "freetype")]
        {
            let freetype = pkg_config::Config::new().find("freetype2").unwrap();
            for include in freetype.include_paths.iter() {
                build.include(include);
            }
            build.define("IMGUI_ENABLE_FREETYPE", None);
            println!("cargo:DEFINE_IMGUI_ENABLE_FREETYPE=");

            // imgui_freetype.cpp needs access to imgui.h
            build.include(dst_sources.join("imgui"));
        }

        let compiler = build.get_compiler();
        // Avoid the if-supported flag functions for easy cases, as they're
        // kinda costly.
        if compiler.is_like_gnu() || compiler.is_like_clang() {
            build.flag("-fno-exceptions").flag("-fno-rtti");
        }
        // TODO: disable linking C++ stdlib? Not sure if it's allowed.
        build
            .out_dir(&dst.join("lib"))
            .warnings(false)
            .file("include_all_imgui.cpp")
            .compile("libcimgui.a");
    }
    Ok(())
}
