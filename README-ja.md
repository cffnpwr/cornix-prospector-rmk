# cornix-prospector-rmk

JezailFunder製[Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard)向けの非公式[RMK](https://github.com/rmk-rs/rmk)ファームウェア。
[Prospector](https://github.com/carrefinho/prospector)をBLE centralにします。

[README.md for English is available here](./README.md)

## Devices

Prospector（ドングル）、Cornix LP（左右）の3デバイス構成です。
ドングルがホストと通信し、左右のキーボードはBLEでドングルへ接続します。
右手をドングル経由にすることで、左手を中継するぶんのBLEホップが1つ減ります。

| firmware | 基板 |
| --- | --- |
| `prospector-central.uf2` | Prospector |
| `cornix-left.uf2` | Cornix LP 左手 |
| `cornix-right.uf2` | Cornix LP 右手 |

## Features

- Vial（[vial.rocks](https://vial.rocks/)）でのキーマップ変更に対応
- 左右のインジケーターLEDによる接続状態とバッテリーの表示
- ドングルのLCDへのキーボードの状態表示と、キーからの輝度変更
- 切断したキーボードの離鍵とバッテリー残量表示のリセット

### Keymap

既定のキーマップはCornix LP公式ファームウェアのものに合わせています。

キーマップはVialで変更します。
`BT0`から`BT4`、`Next BT`、`Prev BT`、`Clear BT`、`Switch Output`、`Clear Peer`、`BL Up`、`BL Down`をVialから任意のキーに割り当てられます。

### Indicator LED

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

### LCD

ドングルはLCD画面にキーボードの状態を表示します。

| 表示位置 | 内容 |
| --- | --- |
| 左上 | 有効な接続方式。USBアイコン、またはBLEプロファイル番号付きのBluetoothアイコン |
| 右上 | 押下中の修飾キー（Control、Option、Shift、Command） |
| 中央 | 現在のレイヤー |
| 下 | 左右それぞれのバッテリー残量 |

#### LCDの輝度変更

LCD自体の輝度を変更できます。
輝度の変更はVialで`BL Up`と`BL Down`をキーに割り当てることで可能になります。
輝度を変更すると、最後にキーを押してから約2秒間レイヤー表示の横に輝度バーが表示されます。

輝度は16段階で起動時は常に最大輝度になります。
最小輝度に設定するとLCDのバックライトが消灯します。

### キーボードが切断したとき

左右どちらかのキーボードがドングルとの接続を失うと、そのキーボードについて次の2つが起こります。

- 押下中だったキーを離鍵します。押したまま切断してもホストに押しっぱなしとして残らず、レイヤーキーであればレイヤーも戻ります
- LCDのバッテリー残量表示を「残量なし」に戻します

どちらもBLEリンクが切断とみなすまで待つため、電源を切った場合・電池が切れた場合・電波が届かなくなった場合は10秒以上かかります。
バッテリー残量表示は、再接続すると元に戻ります。

## How to install

3つのuf2をリリースのダウンロードまたはソースからのビルドで用意し、各基板へ書き込みます。

### Download the prebuilt firmware

ビルド済みのuf2は[Releases](https://github.com/cffnpwr/cornix-prospector-rmk/releases)の各リリースに添付しています。

`cornix-prospector-rmk_<version>.tar.gz`と`checksums.txt`をダウンロードし、アーカイブを検証してから展開します。

```shell
sha256sum -c checksums.txt
tar -xzf cornix-prospector-rmk_<version>.tar.gz
```

アーカイブには3つのuf2と、配布にあたって必要なライセンス文書が入っています。

### Build from source

キーマップや挙動を変更する場合はソースからビルドします。

#### Prerequisites

以下のいずれかを満たす必要があります。

- [mise](https://mise.jdx.dev/)が使用可能
- [Rust](https://www.rust-lang.org/)が使用可能

#### miseを使用する場合

必要なツールをインストールします。

```shell
mise install
```

uf2ファイルをビルドします。

```shell
mise run uf2
```

#### miseを使用しない場合

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

### Flash the firmware

3台とも、UF2形式に対応したブートローダーから書き込みます。
`memory.x`はアプリケーションを`0x1000`から配置するレイアウトに合わせています。
Prospectorに使うXIAO nRF52840は[Adafruit nRF52 Bootloader](https://github.com/adafruit/Adafruit_nRF52_Bootloader)に対応しています。

1. リセットボタンを2回押してブートローダーモードに入ると、USBドライブとしてマウントされます
2. 対応する`.uf2`をコピーします

マウントされたドライブの`INFO_UF2.TXT`に、ブートローダーの種類とバージョンが記載されています。

ペアリング情報は各基板のストレージにあります。
役割の入れ替えやドングルの交換のあとで接続しなくなった場合は、`keyboard.toml`の`[storage]`に`clear_storage = true`を設定して3台とも書き込み、設定を戻します。
Vialで`Clear Peer`を割り当てておけば、5秒長押しで左右のペアリング情報だけを消せます。

RMKは既存のSoftDeviceを自前のBLEスタックで置き換えます。
ZMKなどSoftDevice前提のファームウェアへ戻すには、ブートローダーの再書き込みが必要です。

## License

このファームウェア自体は[MIT License](./LICENSE)です。

ステータス画面では[`u8g2-fonts`](https://crates.io/crates/u8g2-fonts)経由でInconsolata LGCフォントのビットマップを埋め込んで使用しています。
フォントのライセンスは[`LICENSES/OFL-1.1.txt`](./LICENSES/OFL-1.1.txt)を参照してください。
