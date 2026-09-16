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
    }
}
