# cornix-rmk

JezailFunder 製 Cornix LP（nRF52840 無線分割キーボード）向けの [RMK](https://github.com/HaoboGu/rmk) ファームウェア。

## 構成

ドングルを central とする 3 デバイス構成。ドングルが USB / BLE でホストに接続し、Vial とストレージを持つ。左右のキーボードはどちらも peripheral として BLE でドングルへ接続する。

| バイナリ | 対象 | 基板 |
| --- | --- | --- |
| `central` | ドングル | Seeed Studio XIAO nRF52840 |
| `peripheral_left` | 左手 | Cornix LP |
| `peripheral_right` | 右手 | Cornix LP |

ドングルを介することで、右手の入力が左手を中継せず直接ホストへ届く。標準の 2 デバイス split では BLE のホップが 1 つ余分にかかり、右手の入力遅延として現れる。

主要な設定値は `keyboard.toml` にある。

- マトリクス: 論理 8 行 × 7 列（片手 4 行 × 7 列、右手は `row_offset = 4`）
- ロータリーエンコーダ: 左右とも `P1_04` / `P1_06`
- バッテリ: SAADC、分圧比 `2000 / 2806`
- BLE スタック: trouble-host + nrf-sdc（SoftDevice ブロブは使わない）
- RMK: crates.io のリリース版ではなく main ブランチのコミットを `Cargo.toml` で固定している。split リンクの接続パラメータ改善（`max_latency` 30 → 10）がリリース版に入っていないため。

## 必要なツール

[mise](https://mise.jdx.dev/) がツールを解決する。

```shell
mise install
```

Rust ツールチェーンは `rust-toolchain.toml` で 1.97.1 に固定しており、`thumbv7em-none-eabihf` ターゲットと `llvm-tools` を含む。固定はビルドの再現性のためで、RMK 側の制約によるものではない（RMK 上流は `stable` を使っている）。

## ビルド

```shell
mise run uf2
```

`cornix-rmk-central.uf2` / `cornix-rmk-peripheral-left.uf2` / `cornix-rmk-peripheral-right.uf2` が生成される。ELF のみ必要なら `mise run build`。

## 書き込み

Adafruit nRF52 Bootloader を前提とする。ドングル・左手・右手の 3 台すべてに書き込む。

| uf2 | 書き込み先 |
| --- | --- |
| `cornix-rmk-central.uf2` | ドングル |
| `cornix-rmk-peripheral-left.uf2` | 左手 |
| `cornix-rmk-peripheral-right.uf2` | 右手 |

1. 対象の基板をブートローダモードにする（リセットボタンを 2 回押す）。USB ドライブとしてマウントされる。
2. 対応する `.uf2` をドライブへコピーする。

左右とドングルのペアリング情報は各基板のストレージに保存される。役割を入れ替えたり別のドングルへ移行したりしたときに接続しない場合は、`keyboard.toml` の `[storage]` に `clear_storage = true` を設定してビルドし、3 台とも一度書き込んでストレージを消したうえで設定を戻す。

RMK は v0.7.x 以降、既存の SoftDevice を自前の BLE スタックで置き換える。ZMK など SoftDevice 前提のファームへ戻す場合はブートローダの再書き込みが必要になる。

## キーマップの変更

Vial（[vial.rocks](https://vial.rocks/)）に対応している。物理配列の定義は `vial.json`、ファーム側の既定キーマップは `keyboard.toml` の `[[layer]]` にある。

## 既知の制限

- `vial.json` にエンコーダの宣言が無いため、Vial からエンコーダの割り当てを編集できない。ファーム側の既定は `keyboard.toml` の `[[layer]]` の `encoders` で決まる。
