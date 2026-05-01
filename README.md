# flood_pso_viewer

`flood_pso/` が出力する **Minecraft Structure NBT (`.nbt`, gzip)** を、
**ローカルネイティブ（Bevy + wgpu）で GPU を使って閲覧する** Rust 製ビューア。

Web 版（`schematic-renderer` 等）でブラウザがクラッシュするほど巨大な NBT
（5km × 5km @ 5m → 3M 超ブロックエントリ、15km × 15km @ 5m → 9M 超）
を 16–32GB メモリ機で扱うことを目的とする。

## 主な機能

- 高速 **greedy meshing**（rayon 並列、bbox 限定走査）
- 透過ブロック（青/水色のステンドグラス＝水/氷扱い）の AlphaBlend 描画
- **PanOrbit カメラ**（マウスドラッグで回転・ホイールでズーム・右ドラッグで平行移動）
- **bevy_egui の右側パネル**で `flood_pso_meta` を表示
  - method / K / D / loss / IoU / Δh RMSE / git_revision / timestamp / preset
  - **`dh_map` のヒートマップ**（K×K の青→白→赤グラデ）
  - 全 raw キー一覧（折りたたみ）

## ビルド

```bash
# 1) システム依存（X11 + xkbcommon）
sudo apt install -y libxkbcommon-x11-0 libx11-6 libxi6 libxcursor1 libxrandr2 libxinerama1 libgl1

# 2) Rust toolchain（>= 1.80 推奨、本リポジトリは 1.94 で確認）
rustup show

# 3) ビルド
cargo build --release
```

## 実行

```bash
./target/release/flood_pso_viewer <path-to-nbt>

# 例:
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K16_seed0_md_5m_ccpso2.nbt
./target/release/flood_pso_viewer ../flood_pso/results/nbt/gobo_md_5m.nbt
```

操作：
- 左ドラッグ：軌道回転
- 右ドラッグ：平行移動
- ホイール：ズーム
- ESC：終了

## アーキテクチャ

```
              ┌─────────────────────────────┐
.nbt (gzip) ─►│ nbt_loader.rs               │
              │  - fastnbt で structure を   │
              │    deserialize              │
              │  - palette, blocks を       │
              │    Material 配列に densify  │
              │  - flood_pso_meta を         │
              │    FloodPsoMeta に整形       │
              └────┬───────────────┬────────┘
                   │ VoxelGrid     │ FloodPsoMeta
                   ▼               ▼
              ┌────────────┐  ┌─────────────┐
              │greedy_mesh │  │ ui.rs        │
              │ (rayon)    │  │ egui パネル   │
              └────┬───────┘  └─────────────┘
                   │ MeshBuffer per material
                   ▼
              ┌────────────┐
              │ render.rs  │  Bevy Mesh + StandardMaterial
              │            │  AlphaBlend は water/ice のみ
              └────┬───────┘
                   │
                   ▼
              ┌────────────┐
              │  main.rs   │  Bevy App + PanOrbitCamera
              └────────────┘
```

## ベンチマーク（K=16, md_5m, gobo_hd_K16_seed0_md_5m_ccpso2.nbt）

| 段階 | 値 |
|---|---|
| structure size | 968 × 490 × 802 voxels（≒ 380M セル） |
| block entries | 3,085,855 |
| 充填ボクセル | 3,085,855（1.0%） |
| **NBT load** | **0.54 s** |
| **Greedy mesh** | **2.71 s** |
| 出力頂点数 | 10,533,016 |
| 出力 quads | 2,633,254 |

## マテリアルパレット → 色

| Material | 由来 (NBT) | 色 | アルファ |
|---|---|---|---|
| Stone | `minecraft:stone` | 灰 | 不透明 |
| Grass | `minecraft:grass_block` | 緑 | 不透明 |
| Sand | `minecraft:sand` | 砂色 | 不透明 |
| Gravel | `minecraft:gravel` | 茶灰 | 不透明 |
| Water | `minecraft:blue_stained_glass` | 青 | α=0.62 |
| Ice | `minecraft:cyan_stained_glass` | 水色 | α=0.55 |
| Bedrock | `minecraft:bedrock` | 黒 | 不透明 |
| Other | （上記以外） | マゼンタ | 不透明（警告色） |

## 既知の制約

1. **dense grid のメモリ消費**：voxel グリッドは `nx*ny*nz` バイト連続配列。md_5m で 380MB、huge_5m で約 3.4GB。Phase 2 で chunked sparse 表現に置き換える予定。
2. **シャドウ無効**：数百万 quad 規模ではデフォルト shadowmap が重いので OFF。要なら `DirectionalLight::shadows_enabled` を true に。
3. **テクスチャ無し**：マテリアル別単色のみ。Minecraft リソースパック対応は未実装（後続作業の余地）。
4. **WSL2**: Wayland feature を外して X11 のみに限定。WSLg 環境を想定。

## 参考実装

- `web-app/schematic-renderer/mesh_builder_wasm/src/lib.rs` —— greedy meshing アルゴリズムの設計参考。
  本リポジトリでは WASM bindings 抜き、テクスチャ無し、Bevy 用に再実装。
- `web-app/redstone-lib/packages/viewer` —— React + deepslate ベースの Web ビューア構成参考。
- `web-app/flood_pso/` —— NBT 出力元（`flood_pso_meta` スキーマ定義）。
