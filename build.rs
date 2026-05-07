use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    #[cfg(all(feature = "embed-resource", feature = "external-resource"))]
    compile_error!(
        "feature \"embed-resource\" and feature \"external-resource\" cannot be enabled at the same time"
    );
    #[cfg(not(any(feature = "embed-resource", feature = "external-resource")))]
    compile_error!("feature \"embed-resource\" or feature \"external-resource\" must be enabled");
    #[cfg(not(any(feature = "portable", feature = "xdg")))]
    compile_error!("feature \"portable\" or feature \"xdg\" must be enabled");
    #[cfg(all(feature = "portable", feature = "xdg"))]
    compile_error!("feature \"portable\" and feature \"xdg\" cannot be enabled at the same time");
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "android" {
        android();
    }
}

fn android() {
    println!("cargo:rustc-link-lib=c++_shared");
    let out_dir = env::var("OUT_DIR").unwrap();
    let output_path = env::var("CARGO_NDK_OUTPUT_PATH").unwrap_or(out_dir);
    let sysroot_libs_path = PathBuf::from(env::var_os("CARGO_NDK_SYSROOT_LIBS_PATH").unwrap());
    let lib_path = sysroot_libs_path.join("libc++_shared.so");
    let to_path = Path::new(&output_path).join("libc++_shared.so");
    std::fs::copy(lib_path, to_path).unwrap();
}
