fn main() {
    println!("cargo:rerun-if-env-changed=MATLAB_ROOT");
    if std::env::var_os("CARGO_FEATURE_MEX").is_some() {
        let root = std::env::var("MATLAB_ROOT")
            .expect("MATLAB_ROOT must point to the MATLAB installation");
        #[cfg(target_os = "windows")]
        println!("cargo:rustc-link-search=native={root}/extern/lib/win64/microsoft");
        #[cfg(target_os = "linux")]
        println!("cargo:rustc-link-search=native={root}/bin/glnxa64");
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        println!("cargo:rustc-link-search=native={root}/bin/maci64");
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        println!("cargo:rustc-link-search=native={root}/bin/maca64");
    }
}
