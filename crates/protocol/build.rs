use std::io::Result;

fn main() -> Result<()> {
    prost_build::Config::new()
        .out_dir("src/proto")
        .compile_protos(&["proto/kaze.proto"], &["proto/"])?;
    Ok(())
}
