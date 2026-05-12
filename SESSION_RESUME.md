# セッション再開ガイド

> 最終更新: 2026-05-12
> 詳細は `docs/00_開発記録.md`

## いま動いている状態

- 直近のテーマ：**Phase1 EX2（sigma_map）の viewer 連動**
  - flood_pso 側 `make_nbt_hd.py` に `--ks` 追加
  - viewer 側に `J キー` で sigma_map 3D オーバーレイ追加（dh_map=H の上に viridis で投影）
- 過去の主要コミット
  - `f1625ca` Add dh_map heatmap 3D overlay (H key toggle)
  - `b4671f2` Spawn fly camera inside the world + widen FOV to 70
- リモート: <https://github.com/ntaku256/flood_pso_viewer>

## いますぐ実行

```bash
cd /home/moriken/web-app/flood_pso_viewer

# ビルド（ソース変更があった場合）
cargo build --release

# 軽い動作確認（dh_map のみ）
./target/release/flood_pso_viewer ../flood_pso/results/nbt/gobo_xs_overview.nbt

# Phase1 EX2: sigma_map 込みの NBT を viewer で見る
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K4_ks8_seed0_xs_overview_ccpso2.nbt
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K4_ks8_seed0_xs_overview_pso.nbt
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K4_ks8_seed0_xs_overview_gt.nbt

# 本命：CCPSO2 vs 標準PSO（既存 K=16 dh_map のみ）
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K16_seed0_md_5m_ccpso2.nbt
```

`flood_pso` 側で sigma_map 込み NBT を生成するには:
```bash
cd /home/moriken/web-app/flood_pso
.venv/bin/python src/make_nbt_hd.py --K 4 --ks 8 --seed 0 --preset xs_overview
.venv/bin/python src/make_nbt_hd.py --K 16 --ks 8 --seed 0 --preset md_5m   # 重い
```

## 直近の流れ

1. `flood_pso/870d3f3` で Phase1 EX2 完了（sigma_map で逆転点が D=257→D=81 に前倒し）
2. viewer に sigma_map 3D オーバーレイを追加（J キー、dh_map と同じ K×K グリッドだが viridis）
3. `make_nbt_hd.py --ks 8` で sigma_map 込み NBT を生成可能に
4. viewer の右パネルにも sigma_map ヒートマップ + 統計（min/mean/max）を追加

## 次に試して欲しいこと

```bash
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K4_ks8_seed0_xs_overview_ccpso2.nbt
```
- 起動直後にワールド全体 + 上空の K×K ヒートマップ 2 段（dh_map=赤青、sigma_map=viridis）
- **H** キーで dh_map 表示 ON/OFF
- **J** キーで sigma_map 表示 ON/OFF
- 右パネルに `K_s = 8`、sigma_map の min/mean/max が表示されるか

## 現状の操作キー

| キー | 動作 |
|---|---|
| WASD | 視線方向に前後左右（pitch 込み） |
| Space / Shift | ワールド垂直上下 |
| Mouse | 視点回転（Tab で capture 中のみ） |
| Wheel | 移動速度倍率 |
| Tab | マウス capture / release |
| F | Fly / Orbit 切替 |
| V | 水・氷の表示 ON/OFF |
| **H** | **dh_map 3D オーバーレイ ON/OFF** |
| **J** | **sigma_map 3D オーバーレイ ON/OFF（Phase1 EX2 連動）** |
| Esc | 終了 |

## ファイル早見表

```
src/
├── main.rs        Bevy App、CLI、シーン構築、F/V/H/J/Esc キーバインド
├── voxel.rs       Material enum、VoxelGrid、strip_water
├── nbt_loader.rs  fastnbt → VoxelGrid + FloodPsoMeta（sigma_map / K_s 含む）
├── greedy_mesh.rs build_meshes_chunked（rayon 並列、bbox 限定、CHUNK_XZ=128）
├── render.rs      MeshBuffer → Bevy Mesh、WaterLayer マーカー
├── material.rs    VoxelMaterial（自作 WGSL、PBR バイパス）
├── fly_cam.rs     FlyCam component + 4 systems（toggle/look/move/wheel）
├── ui.rs          egui パネル：FPS、capture 状態、flood_pso_meta、dh_map / sigma_map
└── heatmap.rs     dh_map / sigma_map 両 overlay（H/J トグル、共通 spawn_grid_overlay）
```

## 既知の課題（次セッションでの候補タスク）

詳細は `docs/00_開発記録.md` の「既知の課題」節：

- A. 性能：Mesa Zink で 10 FPS。LOD・チャンク粒度・dense grid のスパース化
- B. UX：マウス invert オプション、CursorGrabMode フォールバック、テクスチャ
- C. 機能：複数 NBT 並列ロード比較（PSO/CCPSO2/GT 並べ）、dh_map の地形投影、CLI クエリ

---

何か聞かれたら、まず `docs/00_開発記録.md` を読み返すのが手っ取り早いです。
