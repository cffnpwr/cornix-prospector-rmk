# cornix-prospector-rmk

Unofficial [RMK](https://github.com/HaoboGu/rmk) firmware for the JezailFunder
[Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard).
A [Prospector](https://github.com/carrefinho/prospector) acts as the BLE central.

[日本語版のREADMEはこちら](./README-ja.md)

## Features

- Keymap editing with Vial ([vial.rocks](https://vial.rocks/))

## Notes

- RMK is pinned to a commit on its `main` branch rather than a crates.io release
  - The split link connection parameters that keep the peripheral encoders responsive are not in any release yet
- Encoders cannot be remapped from Vial because `vial.json` does not declare them
  - Their actions come from `encoders` in `keyboard.toml`

## Devices

Three devices: a Prospector dongle and the two Cornix LP halves.
The dongle talks to the host, and both halves connect to the dongle over BLE.
Routing the right half through the dongle removes the extra BLE hop it would otherwise take through the left half.

| firmware | Board |
| --- | --- |
| `prospector-central.uf2` | Prospector |
| `cornix-left.uf2` | Cornix LP, left |
| `cornix-right.uf2` | Cornix LP, right |

## How to build

### Prerequisites

Either of the following.

- [mise](https://mise.jdx.dev/) is available
- [Rust](https://www.rust-lang.org/) is available

### With mise

Install the required tools.

```shell
mise install
```

Build the uf2 files.

```shell
mise run uf2
```

### Without mise

Set up the toolchain and the commands the build needs.
The version, target and components are declared in `rust-toolchain.toml`, so `rustup toolchain install` resolves them.

```shell
rustup toolchain install
cargo install flip-link cargo-binutils cargo-hex-to-uf2
```

See `[tools]` in `mise.toml` to match the versions mise resolves.

Build the three binaries.

```shell
cargo build --release
```

Convert them to Intel HEX.

```shell
cargo objcopy --release --bin central -- -O ihex prospector-central.hex
cargo objcopy --release --bin peripheral_left -- -O ihex cornix-left.hex
cargo objcopy --release --bin peripheral_right -- -O ihex cornix-right.hex
```

Convert them to uf2.

```shell
cargo hex-to-uf2 --input-path prospector-central.hex --output-path prospector-central.uf2 --family nrf52840
cargo hex-to-uf2 --input-path cornix-left.hex --output-path cornix-left.uf2 --family nrf52840
cargo hex-to-uf2 --input-path cornix-right.hex --output-path cornix-right.uf2 --family nrf52840
```

## How to flash

All three devices are flashed from a bootloader that accepts the UF2 format.
`memory.x` matches a layout that places the application at `0x1000`.
The XIAO nRF52840 used for the Prospector is supported by the
[Adafruit nRF52 Bootloader](https://github.com/adafruit/Adafruit_nRF52_Bootloader).

1. Double-tap reset to enter bootloader mode, and the board mounts as a USB drive
2. Copy the matching `.uf2` onto it

`INFO_UF2.TXT` on the mounted drive names the bootloader and its version.

Pairing information lives in each board's storage.
If the halves stop connecting after roles change or a dongle is swapped, set `clear_storage = true` under `[storage]`
in `keyboard.toml`, flash all three, then revert the setting.

RMK replaces any existing SoftDevice with its own BLE stack.
Going back to SoftDevice-based firmware such as ZMK requires reflashing the bootloader.

## License

[MIT License](./LICENSE)
