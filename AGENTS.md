# cornix-prospector-rmk

## 概要

JezailFunder Cornix LP向けの非公式RMKファームウェア。
Prospector（XIAO nRF52840）をBLE centralのドングルとし、Cornix LPの左右半身をperipheralとする3デバイス構成をとる。

Rust（edition 2024）・`no_std`・Embassyで書かれ、ターゲットはnRF52840（`thumbv7em-none-eabihf`）。
3つのバイナリを1つのクレートからビルドする。

## ビルド・lint・フォーマット

mise経由で実行する。

- ビルド: `mise run build`（`cargo build --release`）
- lint: `mise run lint`（`cargo clippy --all-features -- -D warnings`）
- フォーマット: `mise run fmt`（treefmtでrustfmt・yamlfmt・tombiを実行する）
- uf2生成: `mise run uf2`

実装を変更したらlint・フォーマット・ビルドを通す。
uf2生成と実機への書き込みは、依頼されたときだけ行う。

ホストで実行できるテストは無い。
`.cargo/config.toml`でビルドターゲットが`thumbv7em-none-eabihf`に固定されているため、`cargo test`は使わない。

## 主要ファイル

- `src/central.rs`: Prospectorドングル向けで、BLE centralとしてUSBでホストへつなぎLCDに状態を表示する
- `src/peripheral_left.rs`・`src/peripheral_right.rs`: Cornix LPの左右半身向け
- `src/lib.rs`: 3バイナリの共有コード
  - `backlight`・`lcd`・`status_screen`はcentral用
  - `battery_resend`・`indicator`・`ws2812`はperipheral用
- `keyboard.toml`: RMKのキーボード定義（マトリクス・ピン割り当て・レイヤー・エンコーダ・BLE・ストレージ）
- `vial.json`: Vialのキーボード定義で、`build.rs`がxz圧縮してファームウェアへ埋め込む
- `build.rs`: vial定義の生成・`memory.x`の配置・リンカスクリプトとリンクフラグの指定
- `memory.x`: FLASHとRAMのレイアウトで、Adafruit nRF52 Bootloaderを前提にアプリを`0x1000`へ配置する

## コーディング規約

- フォーマットはtreefmtと`.editorconfig`の設定に従う
- clippyの構成は`Cargo.toml`の`[lints]`に従い、`panic`・`unwrap_used`・`absolute_paths`・`pedantic`等が`deny`で警告は全てエラーになる
- モジュールは`mod.rs`を使わず、`src/lcd.rs`と`src/lcd/`のように親モジュールをファイルで置く（`mod_module_files`）
- Rustコードのコメント・doc commentは英語で書き、TOML等の設定ファイルのコメントは日本語で書く
- `README.md`（英語）と`README-ja.md`（日本語）は同じ内容を保ち、一方を変更したらもう一方も同時に更新する
- キーマップ・マトリクス・BLE等の挙動は`keyboard.toml`で変更する
- エンコーダは`vial.json`で宣言してVialから変更でき、既定の動作は`keyboard.toml`の`encoders`にレイヤーごとに`[時計回り, 反時計回り]`の順で書く
- コミットメッセージはConventional Commits・gitmoji・日本語の1行で書く（例は`feat: :sparkles: インジケーターLEDを追加する`）

## 禁止事項・制約

- `Cargo.toml`の`[lints]`を緩めない
- lintの指摘はコードを直して解消する
- lintを抑制する必要がある場合は`#[allow]`ではなく`#[expect(..., reason = "...")]`を使い、理由を必ず添える
