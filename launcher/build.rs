//! Linker settings that make HitMan-Rust.exe load the 2000-era engine DLLs the
//! same way the original Hitman.Exe did.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    // Original exe: not large-address-aware, no DEP opt-in, fixed base 0x400000.
    // The engine (System.dll, gsc.dll, HitmanDlc.dlc) predates DEP/ASLR.
    for arg in ["/LARGEADDRESSAWARE:NO", "/NXCOMPAT:NO", "/DYNAMICBASE:NO", "/BASE:0x400000"] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
    // Export the C-Dilla descriptor under the original (C++-mangled) name.
    println!("cargo:rustc-link-arg-bins=/EXPORT:?CDAPFN0506_WinMain@@3UCDAPFN_PROPERTIES@@A,DATA");
    println!("cargo:rerun-if-changed=build.rs");
}
