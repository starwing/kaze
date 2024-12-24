fn main() {
    cc::Build::new().file("kaze.c").compile("kaze");

    println!("cargo:rustc-rerun-if-changed=kaze.h");
}
