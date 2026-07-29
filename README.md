# cornix-rmk

[RMK](https://github.com/HaoboGu/rmk) firmware for the JezailFunder [Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard).
A Prospector dongle acts as the BLE central.

[日本語版のREADMEはこちら](./README-ja.md)

## Devices

Three devices.
The dongle talks to the host, and both halves connect to the dongle over BLE.
Routing the right half through the dongle removes the extra BLE hop it would otherwise take through the left half.

| uf2 | Board |
| --- | --- |
| `cornix-rmk-central.uf2` | Dongle (Seeed Studio XIAO nRF52840) |
| `cornix-rmk-peripheral-left.uf2` | Cornix LP, left |
| `cornix-rmk-peripheral-right.uf2` | Cornix LP, right |

## Build

```shell
mise install
mise run uf2
```

RMK is pinned to a commit on its `main` branch rather than a crates.io release.
The split link connection parameters that keep the peripheral encoders responsive are not in any release yet.

## Flash

Requires the Adafruit nRF52 Bootloader, and all three devices need flashing.

1. Double-tap reset to enter bootloader mode, and the board mounts as a USB drive
2. Copy the matching `.uf2` onto it

Pairing information lives in each board's storage.
If the halves stop connecting after roles change or a dongle is swapped, clear it.
Set `clear_storage = true` under `[storage]` in `keyboard.toml`, flash all three, then revert the setting.

RMK replaces any existing SoftDevice with its own BLE stack.
Going back to SoftDevice-based firmware such as ZMK requires reflashing the bootloader.

## Keymap

Vial ([vial.rocks](https://vial.rocks/)) is supported.
Encoders cannot be remapped from Vial because `vial.json` does not declare them.
Their actions come from `encoders` in `keyboard.toml`.
