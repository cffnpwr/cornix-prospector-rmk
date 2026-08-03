# cornix-prospector-rmk

Unofficial [RMK](https://github.com/rmk-rs/rmk) firmware for the JezailFunder
[Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard).
A [Prospector](https://github.com/carrefinho/prospector) acts as the BLE central.

[日本語版のREADMEはこちら](./README-ja.md)

## Devices

Three devices: a Prospector dongle and the two Cornix LP halves.
The dongle talks to the host, and both halves connect to the dongle over BLE.
Routing the right half through the dongle removes the extra BLE hop it would otherwise take through the left half.

| firmware | Board |
| --- | --- |
| `prospector-central.uf2` | Prospector |
| `cornix-left.uf2` | Cornix LP, left |
| `cornix-right.uf2` | Cornix LP, right |

## Features

- Keymap editing with Vial ([vial.rocks](https://vial.rocks/))
- Indicator LEDs on both halves for the connection state and the battery
- A status screen on the dongle's LCD, with the brightness adjustable from a key

### Keymap

The default keymap matches the one from the official Cornix LP firmware.

Keymaps are edited with Vial.
`BT0` through `BT4`, `Next BT`, `Prev BT`, `Clear BT`, `Switch Output`, `Clear Peer`, `BL Up` and `BL Down`
can be assigned to any key from Vial.

### Indicator LED

Each half carries two full-color LEDs (WS2812).
They are only powered while there is something to show, so they stay dark when nothing is wrong.
For the first two seconds after power-on, the inner LED lights red and the outer one green.

The roles differ between the halves, following the official firmware.

| Unit | Inner | Outer |
| --- | --- | --- |
| Left | Battery and the link to the dongle | Bluetooth channel |
| Right | Battery | Link to the dongle |

| Display | Meaning |
| --- | --- |
| Slow blink in the channel color | Searching for a host |
| Lit for one second in the channel color | Connected to a host |
| Slow blink in blue | Link to the dongle lost |
| Lit for one second in blue | Connected to the dongle |
| Slow blink in green | Charging |
| Lit for one second in green | Charging finished |
| Blink in red | Battery below 20% |

The color for each channel is 0 green, 1 red, 2 blue, 3 yellow, 4 cyan.
The searching indication is suppressed while a host is using the dongle over USB.

### LCD

The dongle shows the keyboard state on its LCD panel.

| Area | Shows |
| --- | --- |
| Top left | Active transport: a USB icon, or a Bluetooth icon with the BLE profile number |
| Top right | Modifiers currently held: Control, Option, Shift, Command |
| Middle | Active layer |
| Bottom | Battery of each half |

#### Changing the LCD brightness

The LCD's own brightness can be changed.
Assign `BL Up` and `BL Down` to keys in Vial to change it.
Changing the brightness brings up a brightness bar beside the layer for about two seconds after the last press.

The brightness has 16 steps and is always at the brightest on power-on.
Setting it to the lowest step turns the LCD backlight off.

## How to install

Get the three uf2 images, either by downloading a release or by building them from source, then flash every board.

### Download the prebuilt firmware

Prebuilt uf2 images are attached to every entry on the
[Releases](https://github.com/cffnpwr/cornix-prospector-rmk/releases) page.

Download `cornix-prospector-rmk_<version>.tar.gz` along with `checksums.txt`, verify the archive, then extract it.

```shell
sha256sum -c checksums.txt
tar -xzf cornix-prospector-rmk_<version>.tar.gz
```

The archive holds the three uf2 images and the license texts they are distributed under.

### Build from source

Build from source when the keymap or the behaviour has to change.

#### Prerequisites

Either of the following.

- [mise](https://mise.jdx.dev/) is available
- [Rust](https://www.rust-lang.org/) is available

#### With mise

Install the required tools.

```shell
mise install
```

Build the uf2 files.

```shell
mise run uf2
```

#### Without mise

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

### Flash the firmware

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
With `Clear Peer` assigned from Vial, holding it for five seconds clears just the split pairing info.

RMK replaces any existing SoftDevice with its own BLE stack.
Going back to SoftDevice-based firmware such as ZMK requires reflashing the bootloader.

## License

[MIT License](./LICENSE) covers this firmware.

The status screen embeds bitmaps of the Inconsolata LGC font through
[`u8g2-fonts`](https://crates.io/crates/u8g2-fonts); see [`LICENSES/OFL-1.1.txt`](./LICENSES/OFL-1.1.txt) for its
license.
