# cornix-rmk

JezailFunder製[Cornix LP](https://jezailfunder.jp/products/cornix-lp-keyboard)向けの非公式[RMK](https://github.com/HaoboGu/rmk)ファームウェア。
ProspectorドングルをBLE centralにする。

[README.md for English is available here](./README.md)

## デバイス構成

3デバイス構成。
ドングルがホストと通信し、左右のキーボードはBLEでドングルへ接続する。
右手をドングル経由にすることで、左手を中継するぶんのBLEホップが1つ減る。

| uf2 | 基板 |
| --- | --- |
| `cornix-rmk-central.uf2` | ドングル（Seeed Studio XIAO nRF52840） |
| `cornix-rmk-peripheral-left.uf2` | Cornix LP 左手 |
| `cornix-rmk-peripheral-right.uf2` | Cornix LP 右手 |

## ビルド

```shell
mise install
mise run uf2
```

peripheralのエンコーダの追従性を保つsplitリンクの接続パラメータが、まだどのリリースにも入っていないため、RMKはcrates.ioのリリース版ではなく`main`ブランチのコミットで固定している。

## 書き込み

Adafruit nRF52 Bootloaderを前提とし、3台すべてに書き込む。

1. リセットボタンを2回押してブートローダモードに入ると、USBドライブとしてマウントされる
2. 対応する`.uf2`をコピーする

ペアリング情報は各基板のストレージにある。
役割の入れ替えやドングルの交換のあとで接続しなくなった場合は、`keyboard.toml`の`[storage]`に`clear_storage = true`を設定して3台とも書き込み、設定を戻す。

RMKは既存のSoftDeviceを自前のBLEスタックで置き換える。
ZMKなどSoftDevice前提のファームへ戻すには、ブートローダの再書き込みが要る。

## キーマップ

Vial（[vial.rocks](https://vial.rocks/)）に対応している。
`vial.json`がエンコーダを宣言していないため、エンコーダはVialから変更できず、動作は`keyboard.toml`の`encoders`で決まる。
