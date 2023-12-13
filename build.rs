use std::path::PathBuf;

fn extract_debug_symbols(kernel: &PathBuf) -> PathBuf {
    let path = kernel.with_extension("sym");
    let mut cmd = std::process::Command::new("x86_64-elf-objcopy");
    cmd.arg("--only-keep-debug").arg(&kernel).arg(&path);
    cmd.spawn().unwrap().wait().unwrap();
    path
}

fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel").unwrap());

    let sym = extract_debug_symbols(&kernel);

    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
    println!("cargo:rustc-env=KERNEL_SYM_PATH={}", sym.display());
}
