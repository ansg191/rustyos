use std::process::Command;

fn set_debug(cmd: &mut Command) {
    // Set qemu to wait for a debugger to attach
    cmd.arg("-s").arg("-S");

    let kernel_sym_path = env!("KERNEL_SYM_PATH");

    let fmt = format!(
        r#"
    target remote localhost:1234
    symbol-file -o 0x8000000000 {kernel_sym_path}
    set substitute-path /rustc/3340d49d22b1aba425779767278c40781826c2f5 /Volumes/Source/.rustup/toolchains/nightly-aarch64-apple-darwin/lib/rustlib/src/rust
    b kmain
    "#
    );

    // Output .gdbinit
    std::fs::write(".gdbinit", fmt).unwrap();

    println!("Run `gdb` to debug the kernel");
}

fn main() {
    let bios_path = env!("BIOS_PATH");

    let args = std::env::args().collect::<Vec<_>>();

    let debug = args.get(1) == Some(&"debug".to_string());

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.arg("-drive")
        .arg(format!("format=raw,file={bios_path}"));

    // cmd.arg("-serial").arg("stdio");

    cmd.arg("-m").arg("1G");
    cmd.arg("-smp").arg("4");
    cmd.arg("-nographic");

    if debug {
        set_debug(&mut cmd);
    }

    let mut child = cmd.spawn().unwrap();
    child.wait().unwrap();
}
