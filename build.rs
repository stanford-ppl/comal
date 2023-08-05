use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "tortilla/proto/tortilla.proto",
            "tortilla/proto/stream.proto",
            "tortilla/proto/ops.proto",
        ],
        &["tortilla/proto/"],
    )?;
    Ok(())
}
