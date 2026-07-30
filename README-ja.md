# cornix-prospector-rmk

JezailFunder製[Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard)向けの非公式[RMK](https://github.com/HaoboGu/rmk)ファームウェア。
[Prospector](https://github.com/carrefinho/prospector)をBLE centralにします。

[README.md for English is available here](./README.md)

## Features

- Vial（[vial.rocks](https://vial.rocks/)）でのキーマップ変更に対応
- インジケーターLEDによる接続状態とバッテリーの表示

## Notes

- RMKはcrates.ioのリリース版ではなく`main`ブランチのコミットで固定しています
  - peripheralのエンコーダの追従性を保つsplitリンクの接続パラメータが、まだどのリリースにも入っていないためです
- `vial.json`がエンコーダを宣言していないため、エンコーダはVialから変更できません
  - 動作は`keyboard.toml`の`encoders`で決まります
- Bluetoothチャンネルを切り替えるキーは、キーマップに割り当てていません
  - ドングルはUSBでホストへつながるため、BLEのチャンネルを使う場面が限られます

## Devices

Prospector（ドングル）、Cornix LP（左右）の3デバイス構成です。
ドングルがホストと通信し、左右のキーボードはBLEでドングルへ接続します。
右手をドングル経由にすることで、左手を中継するぶんのBLEホップが1つ減ります。

| firmware | 基板 |
| --- | --- |
| `prospector-central.uf2` | Prospector |
| `cornix-left.uf2` | Cornix LP 左手 |
| `cornix-right.uf2` | Cornix LP 右手 |

## Indicator LED

左右それぞれにフルカラーLED（WS2812）が2個あります。
表示する内容がある間だけ給電するため、問題のない状態では消灯します。
起動から2秒間は内側が赤、外側が緑に点灯します。

役割は公式ファームウェアに合わせ、左右で異なります。

| ユニット | 内側 | 外側 |
| --- | --- | --- |
| 左手 | バッテリーとドングルとの接続 | Bluetoothチャンネル |
| 右手 | バッテリー | ドングルとの接続 |

| 表示 | 意味 |
| --- | --- |
| チャンネルに対応した色でゆっくり点滅 | ホストを検索中 |
| チャンネルに対応した色で1秒点灯 | ホストへ接続 |
| 青でゆっくり点滅 | ドングルとの接続が切断 |
| 青で1秒点灯 | ドングルへ接続 |
| 緑でゆっくり点滅 | 充電中 |
| 緑で1秒点灯 | 充電完了 |
| 赤で点滅 | バッテリー残量が20%未満 |

チャンネルごとの色は0が緑、1が赤、2が青、3が黄、4がシアンです。
ドングルをUSBでホストに使っている間は、検索中の表示をしません。

## How to build

### Prerequisites

以下のいずれかを満たす必要があります。

- [mise](https://mise.jdx.dev/)が使用可能
- [Rust](https://www.rust-lang.org/)が使用可能

### miseを使用する場合

必要なツールをインストールします。

```shell
mise install
```

uf2ファイルをビルドします。

```shell
mise run uf2
```

### miseを使用しない場合

ツールチェーンとビルドに必要なコマンドを個別に用意します。
バージョン・ターゲット・コンポーネントは`rust-toolchain.toml`に記述してあるため、`rustup toolchain install`で解決されます。

```shell
rustup toolchain install
cargo install flip-link cargo-binutils cargo-hex-to-uf2
```

miseが解決するバージョンに合わせる場合は、`mise.toml`の`[tools]`を参照してください。

3つのバイナリをビルドします。

```shell
cargo build --release
```

Intel HEXに変換します。

```shell
cargo objcopy --release --bin central -- -O ihex prospector-central.hex
cargo objcopy --release --bin peripheral_left -- -O ihex cornix-left.hex
cargo objcopy --release --bin peripheral_right -- -O ihex cornix-right.hex
```

uf2に変換します。

```shell
cargo hex-to-uf2 --input-path prospector-central.hex --output-path prospector-central.uf2 --family nrf52840
cargo hex-to-uf2 --input-path cornix-left.hex --output-path cornix-left.uf2 --family nrf52840
cargo hex-to-uf2 --input-path cornix-right.hex --output-path cornix-right.uf2 --family nrf52840
```

## How to flash

3台とも、UF2形式に対応したブートローダから書き込みます。
`memory.x`はアプリケーションを`0x1000`から配置するレイアウトに合わせています。
Prospectorに使うXIAO nRF52840は[Adafruit nRF52 Bootloader](https://github.com/adafruit/Adafruit_nRF52_Bootloader)に対応しています。

1. リセットボタンを2回押してブートローダモードに入ると、USBドライブとしてマウントされます
2. 対応する`.uf2`をコピーします

マウントされたドライブの`INFO_UF2.TXT`に、ブートローダの種類とバージョンが記載されています。

ペアリング情報は各基板のストレージにあります。
役割の入れ替えやドングルの交換のあとで接続しなくなった場合は、`keyboard.toml`の`[storage]`に`clear_storage = true`を設定して3台とも書き込み、設定を戻します。

RMKは既存のSoftDeviceを自前のBLEスタックで置き換えます。
ZMKなどSoftDevice前提のファームへ戻すには、ブートローダの再書き込みが必要です。

## License

[MIT License](./LICENSE)
