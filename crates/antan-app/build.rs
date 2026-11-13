//! Windows'ta exe dosyasına simge gömer: assets/icon.ico → kaynak (.res) → bağlayıcı.
//! Windows SDK'daki rc.exe kullanılır; bulunamazsa simgesiz derlenir (uyarı verir).

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe bulunamadı; exe simgesiz derleniyor (Windows SDK kurun)");
        return;
    };
    let ico = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon.ico");
    let ico = ico.canonicalize().expect("assets/icon.ico bulunamadı");
    // RC dizgisinde ters eğik çizgiler ikilenir; canonicalize'ın \\?\ önekini at.
    let ico_str = ico.display().to_string();
    let ico_str = ico_str.strip_prefix(r"\\?\").unwrap_or(&ico_str).replace('\\', "\\\\");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc_file = out.join("antan.rc");
    let res_file = out.join("antan.res");
    std::fs::write(&rc_file, format!("1 ICON \"{ico_str}\"\n")).unwrap();
    let ok = Command::new(&rc).arg("/nologo").arg("/fo").arg(&res_file).arg(&rc_file).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={}", res_file.display());
    } else {
        println!("cargo:warning=rc.exe çalışmadı; exe simgesiz derleniyor");
    }
}

/// PATH'teki ya da Windows SDK'daki en yeni x64 rc.exe.
fn find_rc() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let base = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(base).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("x64").join("rc.exe").exists()).collect();
    versions.sort();
    versions.pop().map(|p| p.join("x64").join("rc.exe"))
}
