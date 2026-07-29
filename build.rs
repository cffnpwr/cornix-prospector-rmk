//! Build script.
//!
//! - Compresses `vial.json` into the constants RMK embeds in the firmware.
//! - Copies `memory.x` into `OUT_DIR` so the linker always finds it.
//! - Passes the linker scripts and flags the firmware needs.

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::{env, fs};

use xz2::read::XzEncoder;

fn main() {
    println!("cargo:rerun-if-changed=vial.json");
    println!("cargo:rerun-if-changed=keyboard.toml");
    println!("cargo:rerun-if-changed=memory.x");

    generate_vial_config();

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    File::create(out.join("memory.x"))
        .expect("Cannot create memory.x in OUT_DIR")
        .write_all(include_bytes!("memory.x"))
        .expect("Cannot write memory.x in OUT_DIR");
    println!("cargo:rustc-link-search={}", out.display());

    // `--nmagic` is required if memory section addresses are not aligned to 0x10000,
    // for example the FLASH and RAM sections in `memory.x`.
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    println!("cargo:rustc-link-arg=--nmagic");
    // The link script provided by cortex-m-rt.
    println!("cargo:rustc-link-arg=-Tlink.x");
    // The extra link script from defmt.
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // Stack overflow check: https://github.com/knurling-rs/flip-link
    println!("cargo:rustc-linker=flip-link");
}

fn generate_vial_config() {
    let out_file =
        Path::new(&env::var_os("OUT_DIR").expect("OUT_DIR is not set")).join("config_generated.rs");

    let mut content = String::new();
    File::open("vial.json")
        .expect("Cannot open vial.json")
        .read_to_string(&mut content)
        .expect("Cannot read vial.json");

    let vial_cfg = json::stringify(json::parse(&content).expect("Cannot parse vial.json"));
    let mut keyboard_def_compressed: Vec<u8> = Vec::new();
    XzEncoder::new(vial_cfg.as_bytes(), 6)
        .read_to_end(&mut keyboard_def_compressed)
        .expect("Cannot compress the vial keyboard definition");

    let keyboard_id: [u8; 8] = [0xB9, 0xBC, 0x09, 0xB2, 0x9D, 0x37, 0x4C, 0xEA];
    let const_declarations = format!(
        "pub const VIAL_KEYBOARD_DEF: &[u8] = &{keyboard_def_compressed:?};\n\
         pub const VIAL_KEYBOARD_ID: &[u8] = &{keyboard_id:?};\n"
    );
    fs::write(out_file, const_declarations).expect("Cannot write the generated vial config");
}
