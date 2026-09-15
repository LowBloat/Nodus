fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set("ProductName", "Nodus")
            .set("FileDescription", "Nodus - notas Markdown P2P")
            .set("CompanyName", "Nodus")
            .set("LegalCopyright", "Copyright 2026")
            .set("OriginalFilename", "Nodus.exe");
        resource.compile().expect("Windows resources must compile");
    }
}
