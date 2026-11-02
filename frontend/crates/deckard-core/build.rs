fn main() {
    println!("cargo:rerun-if-changed=src/log_shim.c");
    cc::Build::new()
        .file("src/log_shim.c")
        .compile("deckard_log_shim");
}
