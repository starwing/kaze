fn main() {
    cc::Build::new().file("kaze.c").compile("kaze");
    println!("cargo::rerun-if-changed=kaze.h");
}
