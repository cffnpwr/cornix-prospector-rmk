# Changelog

## [1.1.0](https://github.com/cffnpwr/cornix-prospector-rmk/compare/v1.0.2...v1.1.0) (2026-09-04)


### Features

* :sparkles: LCDヘッダーに優先接続方式を暗色アイコンで常時表示する ([#69](https://github.com/cffnpwr/cornix-prospector-rmk/issues/69)) ([78d5714](https://github.com/cffnpwr/cornix-prospector-rmk/commit/78d5714886dc17a2565b6c4a8e7dd800c7ca8af8))


### Bug Fixes

* **deps:** :package: update rust crate bt-hci to v0.10.0 ([#63](https://github.com/cffnpwr/cornix-prospector-rmk/issues/63)) ([06fd3fe](https://github.com/cffnpwr/cornix-prospector-rmk/commit/06fd3fe074f1f4e0e30b848301268783f48a1225))
* **deps:** :package: update rust crate bt-hci to v0.10.1 ([#64](https://github.com/cffnpwr/cornix-prospector-rmk/issues/64)) ([47b6f67](https://github.com/cffnpwr/cornix-prospector-rmk/commit/47b6f676f561d3f70ae0c8ae23b89a20de5c100d))
* **deps:** :package: update rust crate cortex-m to v0.7.8 ([#46](https://github.com/cffnpwr/cornix-prospector-rmk/issues/46)) ([a6f299e](https://github.com/cffnpwr/cornix-prospector-rmk/commit/a6f299e0e83b8e333ea673d0660d46737cc38933))
* **deps:** :package: update rust crate cortex-m to v0.7.9 ([#65](https://github.com/cffnpwr/cornix-prospector-rmk/issues/65)) ([8173302](https://github.com/cffnpwr/cornix-prospector-rmk/commit/817330285bc5ab78389b7399bba425311a8fd518))

## [1.0.2](https://github.com/cffnpwr/cornix-prospector-rmk/compare/v1.0.1...v1.0.2) (2026-08-08)


### Performance Improvements

* :zap: LCDフレームバッファをウィンドウ方式にしてRAM占有を減らす ([#44](https://github.com/cffnpwr/cornix-prospector-rmk/issues/44)) ([92acfb5](https://github.com/cffnpwr/cornix-prospector-rmk/commit/92acfb5f2b704fc519df71e91f30d384c36d67df))

## [1.0.1](https://github.com/cffnpwr/cornix-prospector-rmk/compare/v1.0.0...v1.0.1) (2026-08-04)


### Bug Fixes

* :bug: 切断されたキーボードのバッテリー残量表示を消す ([#31](https://github.com/cffnpwr/cornix-prospector-rmk/issues/31)) ([9453190](https://github.com/cffnpwr/cornix-prospector-rmk/commit/9453190acd0993d8971285936d393fea4bd6e8a9))
* :bug: 切断されたキーボードの押下中のキーを離鍵する ([#32](https://github.com/cffnpwr/cornix-prospector-rmk/issues/32)) ([b62eaf0](https://github.com/cffnpwr/cornix-prospector-rmk/commit/b62eaf0cbd543e45af2481c549da835622c6c3a6))
* **deps:** :package: update rust crate embedded-graphics to v0.8.2 ([#24](https://github.com/cffnpwr/cornix-prospector-rmk/issues/24)) ([a95fc52](https://github.com/cffnpwr/cornix-prospector-rmk/commit/a95fc520929fc861876c2c11d4eed79a7254681c))
* **deps:** pin dependencies ([#23](https://github.com/cffnpwr/cornix-prospector-rmk/issues/23)) ([862ea8e](https://github.com/cffnpwr/cornix-prospector-rmk/commit/862ea8ef129ef6df3255332c2eaeb9ec44e4f8ab))


### Performance Improvements

* :zap: LCDの描画と転送を帯単位にする ([#22](https://github.com/cffnpwr/cornix-prospector-rmk/issues/22)) ([0b83227](https://github.com/cffnpwr/cornix-prospector-rmk/commit/0b832276c6689a7289fd0277af246eee22f0c35e))

## 1.0.0 (2026-08-02)


### Features

* :sparkles: Cornix LP向けRMKファームウェアプロジェクトを初期化 ([#2](https://github.com/cffnpwr/cornix-prospector-rmk/issues/2)) ([266dc8e](https://github.com/cffnpwr/cornix-prospector-rmk/commit/266dc8e0f433b34ded3af306a4ac5688cb0f568d))
* :sparkles: Prospectorをcentralとする3デバイス構成へ移行 ([#8](https://github.com/cffnpwr/cornix-prospector-rmk/issues/8)) ([d4c79db](https://github.com/cffnpwr/cornix-prospector-rmk/commit/d4c79dbcb9d551b0b9f2082781621a6f29fe2d63))
* :sparkles: インジケーターLEDを追加し電力とBLEの設定を公式ファームに合わせる ([#11](https://github.com/cffnpwr/cornix-prospector-rmk/issues/11)) ([2cd4f98](https://github.com/cffnpwr/cornix-prospector-rmk/commit/2cd4f9835f2f47a94c6744b78653ceed8ce3f9d2))
* :sparkles: ドングルのLCDにキーボードの状態を表示し輝度をキーから変更できるようにする ([#15](https://github.com/cffnpwr/cornix-prospector-rmk/issues/15)) ([042c0ea](https://github.com/cffnpwr/cornix-prospector-rmk/commit/042c0ead1c49a6363f120960f17033a3329a2ce6))
* :sparkles: ビルド済みファームウェアの配布と公式準拠の既定キーマップを追加する ([#18](https://github.com/cffnpwr/cornix-prospector-rmk/issues/18)) ([f5aa987](https://github.com/cffnpwr/cornix-prospector-rmk/commit/f5aa9870d07b77bdc981e2475ae8a4f5b41c36f2))
* :tada: Initial commit ([8f98371](https://github.com/cffnpwr/cornix-prospector-rmk/commit/8f9837155f9cd6e4b50a9bf9f7a155d808121162))


### Bug Fixes

* :bug: mise管理のcargoツールをmiseが解決できるバージョン指定に修正 ([#9](https://github.com/cffnpwr/cornix-prospector-rmk/issues/9)) ([ddc7203](https://github.com/cffnpwr/cornix-prospector-rmk/commit/ddc7203fa359cf197fa3d82d157c4749d6ba8711))
* :bug: release-pleaseがCargo.tomlを解釈できない問題を修正する ([#19](https://github.com/cffnpwr/cornix-prospector-rmk/issues/19)) ([e254ce0](https://github.com/cffnpwr/cornix-prospector-rmk/commit/e254ce08dfe7a5576fe21250e6b5f8105ddec722))
* :bug: vial.jsonにCLR_PEERを追加しキー割り当ての案内をREADMEへ追記 ([#12](https://github.com/cffnpwr/cornix-prospector-rmk/issues/12)) ([955d57c](https://github.com/cffnpwr/cornix-prospector-rmk/commit/955d57c5c6e3649f839fcf16b101430a39c68fa0))
