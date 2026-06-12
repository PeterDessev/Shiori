//! Embeds the 栞 icon and version metadata into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/shiori.ico");
    if !cfg!(target_os = "windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icon/shiori.ico");
    res.set("ProductName", "Shiori");
    res.set("FileDescription", "Shiori — Japanese reading companion");
    if let Err(e) = res.compile() {
        // A missing rc.exe shouldn't break the build; the exe just
        // ships without an embedded icon.
        println!("cargo:warning=icon resource embedding failed: {e}");
    }
}
