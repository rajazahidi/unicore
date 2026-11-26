//! Minimal zero-dependency Windows PE test executable for UniCore OS Win32 verification.
//! Compile on Windows with:
//!   rustc --edition 2021 -O -C target-feature=+crt-static os-builder/fixtures/unicore_hello.rs -o os-builder/out/unicore_hello.exe

fn main() {
    println!("UniCore Win32 Subsystem OK!");
    println!("Executed .exe inside UniCore OS without third-party launchers.");
}
