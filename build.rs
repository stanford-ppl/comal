use cmake;
use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "tortilla/proto/comal.proto",
            "tortilla/proto/tortilla.proto",
            "tortilla/proto/stream.proto",
            "tortilla/proto/ops.proto",
        ],
        &["tortilla/proto/"],
    )?;
    let lib_path = "external/ramulator2_wrapper/ext/ramulator2/";
    // let dst = cmake::Config::new(lib_path).build();
    println!("cargo:rustc-link-search=native={}", lib_path);
    // println!("cargo:rustc-link-search=native={}", lib_path);
    println!("cargo:rustc-link-lib=ramulator");
    // println!("cargo:rustc-link-lib=ramulator");
    // println!("cargo:rustc-link-lib=ramulator");
    Ok(())
}
