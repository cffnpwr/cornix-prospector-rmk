# cornix-rmk

JezailFunder 製 Cornix LP（nRF52840 無線分割キーボード）向けの [RMK](https://github.com/HaoboGu/rmk) ファームウェア。

## 構成

現状は標準の 2 デバイス split 構成。左手が central（USB / BLE ホスト接続、Vial、ストレージ）、右手が peripheral。

| バイナリ | 対象 |
| --- | --- |
| `central` | 左手 |
| `peripheral` | 右手 |

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

`cornix-rmk-central.uf2` と `cornix-rmk-peripheral.uf2` が生成される。ELF のみ必要なら `mise run build`。

## 書き込み

Adafruit nRF52 Bootloader を前提とする。

1. 対象の基板をブートローダモードにする（リセットボタンを 2 回押す）。USB ドライブとしてマウントされる。
2. 対応する `.uf2` をドライブへコピーする。

RMK は v0.7.x 以降、既存の SoftDevice を自前の BLE スタックで置き換える。ZMK など SoftDevice 前提のファームへ戻す場合はブートローダの再書き込みが必要になる。

## キーマップの変更

Vial（[vial.rocks](https://vial.rocks/)）に対応している。物理配列の定義は `vial.json`、ファーム側の既定キーマップは `keyboard.toml` の `[[layer]]` にある。

## 検証状況

- バッテリ ADC ピン `P0_05` は公式ファームの解析で確証が取れていない。分圧比は確定しているため、報告バッテリ電圧と実測電圧の突き合わせで判定できる。
