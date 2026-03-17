fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_manifest_file("dpetoolbox.manifest");
        res.compile().expect("Failed to compile Windows resources");
    }
}
