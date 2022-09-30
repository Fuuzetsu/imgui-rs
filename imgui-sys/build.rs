#![allow(dead_code)]

const DEFINES: &[(&str, Option<&str>)] = &[
    // Rust `char` is a unicode scalar value, e.g. 32 bits.
    ("IMGUI_USE_WCHAR32", None),
    // Disabled due to linking issues
    ("CIMGUI_NO_EXPORT", None),
    ("IMGUI_DISABLE_WIN32_FUNCTIONS", None),
    ("IMGUI_DISABLE_OSX_FUNCTIONS", None),
];

fn main() -> std::io::Result<()> {
    // Root of imgui-sys
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    // Output define args for compiler
    for (key, value) in DEFINES.iter() {
        println!("cargo:DEFINE_{}={}", key, value.unwrap_or(""));
    }

    // Feature flags - no extra dependencies, so these are queried as
    // env-vars to avoid recompilation of build.rs
    let docking_enabled = std::env::var_os("CARGO_FEATURE_DOCKING").is_some();
    let wasm_enabled = std::env::var_os("CARGO_FEATURE_WASM").is_some();

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
    copy_dir_all(
        &if docking_enabled {
            manifest_dir.join("third-party/imgui-docking")
        } else {
            manifest_dir.join("third-party/imgui-master")
        },
        &dst_sources,
    )
    .unwrap();

    // We can now expose the path of the imgui files to other projects like
    // implot-rs via `DEP_IMGUI_THIRD_PARTY` env-var, so they can build against
    // the same thing.
    println!("cargo:THIRD_PARTY={}", dst_sources.display());

    // If we aren't building WASM output, bunch of extra stuff to do
    if !wasm_enabled {
        // C++ compiler
        let mut build = cc::Build::new();
        // Put the library in lib, as now we're also including sources in
        // OUT_DIR.
        build.out_dir(&dst.join("lib"));
        build.cpp(true);

        // Set defines for compiler
        for (key, value) in DEFINES.iter() {
            build.define(key, *value);
        }

        // Freetype font rasterizer feature
        #[cfg(feature = "freetype")]
        {
            // Find library
            let freetype = pkg_config::Config::new().find("freetype2").unwrap();
            for include in freetype.include_paths.iter() {
                build.include(include);
            }
            // Set flag for dear imgui
            build.define("IMGUI_ENABLE_FREETYPE", None);
            println!("cargo:DEFINE_IMGUI_ENABLE_FREETYPE=");

            // imgui_freetype.cpp needs access to `#include "imgui.h"` so we
            // include the directory that contains it.
            build.include(dst_sources.join("imgui"));
        }

        // Which "all imgui" file to use
        let imgui_cpp = if docking_enabled {
            "include_imgui_docking.cpp"
        } else {
            "include_imgui_master.cpp"
        };

        // Set up compiler
        let compiler = build.get_compiler();

        // Avoid the if-supported flag functions for easy cases, as they're
        // kinda costly.
        if compiler.is_like_gnu() || compiler.is_like_clang() {
            build.flag("-fno-exceptions").flag("-fno-rtti");
        }

        // Build imgui lib, suppressing warnings.
        // TODO: disable linking C++ stdlib? Not sure if it's allowed.
        build.warnings(false).file(imgui_cpp).compile("libcimgui.a");
    }
    Ok(())
}
