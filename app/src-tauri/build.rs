fn main() {
    println!("cargo:rerun-if-changed=../ui");
    println!("cargo:rerun-if-changed=../../tools");
    tauri_build::build()
}
