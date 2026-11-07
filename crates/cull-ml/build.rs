//! macOS: the prebuilt ONNX Runtime static library calls
//! `__isPlatformVersionAtLeast` (the helper behind Objective-C `@available`)
//! from its CoreML code. That function lives in clang's runtime
//! (`libclang_rt.osx.a`), which rustc leaves out of the link
//! (`-nodefaultlibs`): without it every binary depending on this crate fails
//! with "Undefined symbols … ___isPlatformVersionAtLeast" (seen with
//! Xcode 27). Link it explicitly, unbundled, so the final linker only pulls
//! the members it needs.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PELLI_CULL_CLANG_RT_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let dir = match std::env::var("PELLI_CULL_CLANG_RT_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => match clang_runtime_dir() {
            Some(d) => d,
            None => {
                println!("cargo:warning=clang runtime not found (xcrun clang --print-resource-dir): linking may fail on ___isPlatformVersionAtLeast");
                return;
            }
        },
    };
    if !dir.join("libclang_rt.osx.a").exists() {
        println!("cargo:warning=libclang_rt.osx.a not found in {}", dir.display());
        return;
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static:-bundle=clang_rt.osx");
}

fn clang_runtime_dir() -> Option<PathBuf> {
    let out = Command::new("xcrun").args(["clang", "--print-resource-dir"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8(out.stdout).ok()?;
    Some(PathBuf::from(root.trim()).join("lib").join("darwin"))
}
