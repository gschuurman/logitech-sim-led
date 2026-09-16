fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/icon.ico");
        if let Err(e) = res.compile() {
            // Don't fail the whole build over a missing icon (e.g. a
            // stripped-down source checkout) -- just ship without one.
            println!("cargo:warning=failed to embed Windows icon resource: {e}");
        }

        #[cfg(feature = "gui")]
        copy_webview2_loader();
    }
}

/// `wry`'s WebView2 backend needs `WebView2Loader.dll` sitting next to the
/// final exe at runtime -- it's not the WebView2 Runtime itself (a
/// separate, normally-already-present system install; Windows ships it
/// with Edge), just a small loader DLL. `webview2-com-sys`'s own build.rs
/// copies it into *its own* Cargo `OUT_DIR`, not ours, and not next to the
/// binary -- confirmed the hard way (a fresh install's sld-service.exe
/// failed to start, "missing WebView2Loader.dll"). There's no official
/// Cargo channel to that path (no `links` key/`DEP_*` env var exposing
/// it), so this reaches into the shared `target/<profile>/build/`
/// directory Cargo puts every crate's build output under and finds it by
/// its crate-name prefix -- the same trick other wry-based projects use
/// for this exact problem. `packaging/windows/product.wxs` installs the
/// copy this leaves in `target/<profile>/`; see the comment on its
/// `WebView2LoaderComponent`.
#[cfg(all(windows, feature = "gui"))]
fn copy_webview2_loader() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // OUT_DIR is target/<profile>/build/sld-service-<hash>/out -- three
    // levels up is target/<profile>/, where the final exe lands.
    let Some(target_profile_dir) = out_dir.ancestors().nth(3) else {
        println!("cargo:warning=couldn't locate target/<profile> dir from OUT_DIR to place WebView2Loader.dll");
        return;
    };
    let build_dir = target_profile_dir.join("build");
    let Ok(entries) = std::fs::read_dir(&build_dir) else {
        println!(
            "cargo:warning=couldn't read {} to find webview2-com-sys's build output",
            build_dir.display()
        );
        return;
    };

    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("webview2-com-sys-") {
            continue;
        }
        let src = entry
            .path()
            .join("out")
            .join(arch)
            .join("WebView2Loader.dll");
        if !src.is_file() {
            continue;
        }
        let dest = target_profile_dir.join("WebView2Loader.dll");
        if let Err(e) = std::fs::copy(&src, &dest) {
            println!(
                "cargo:warning=failed to copy {} to {}: {e}",
                src.display(),
                dest.display()
            );
        }
        return;
    }
    println!("cargo:warning=WebView2Loader.dll not found under {} (webview2-com-sys build output) -- the built exe won't run until it's placed next to it", build_dir.display());
}
