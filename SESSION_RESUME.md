# セッション再開ガイド

> 最終更新: 2026-05-07
> 詳細は `docs/00_開発記録.md`

## いま動いている状態

- 最新コミット: `b4671f2` Spawn fly camera inside the world + widen FOV to 70
- ビルド済みバイナリ: `target/release/flood_pso_viewer`（再ビルド不要）
- リモート: <https://github.com/ntaku256/flood_pso_viewer> （main 同期済み）

## いますぐ実行

```bash
cd /home/moriken/web-app/flood_pso_viewer

# 軽い動作確認（推奨）
./target/release/flood_pso_viewer ../flood_pso/results/nbt/gobo_xs_overview.nbt

# 本命：CCPSO2 vs 標準PSO の md_5m
./target/release/flood_pso_viewer ../flood_pso/results/nbt/hd/gobo_hd_K16_seed0_md_5m_ccpso2.nbt
```

## 直近の流れ（最後にやり取りしたこと）

1. ユーザ「マウス視点が F の orbit みたいに世界が回って見える、Creative みたく自然に上向きたい」
2. → カメラ初期位置を「ワールド天井のすぐ上、中心の少し南」に変更、FOV を 45°→70° に拡大（`b4671f2`）
3. ユーザは試したかどうか報告前に **このセッションを閉じることに**

## 次に試して欲しいこと

`b4671f2` の挙動を確認：
- 起動直後にワールド全体が画面下〜前に広がっているか
- Tab → マウスで **頭を回す感覚** で視点が回るか
- WASD で視線方向に進むか

それでもまだ違和感があれば次の手：

| 症状 | 対策 |
|---|---|
| マウス上下が逆 | `--invert-y` フラグを追加（未実装） |
| Tab 押しても視点動かない | `CursorGrabMode::Locked` が WSLg で効いてない可能性 → `Confined` への fallback |
| 重すぎる（FPS < 15） | `gobo_xs_overview.nbt` でテスト、`--no-water`、`CHUNK_XZ` を 64 に細分化 |
| F 切替で位置が変 | orbit focus の snap ロジック再点検 |

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
| Esc | 終了 |

## ファイル早見表

```
src/
├── main.rs        Bevy App、CLI、シーン構築、F/V/Esc キーバインド
├── voxel.rs       Material enum、VoxelGrid、strip_water
├── nbt_loader.rs  fastnbt → VoxelGrid + FloodPsoMeta
├── greedy_mesh.rs build_meshes_chunked（rayon 並列、bbox 限定、CHUNK_XZ=128）
├── render.rs      MeshBuffer → Bevy Mesh、WaterLayer マーカー
├── material.rs    VoxelMaterial（自作 WGSL、PBR バイパス）
├── fly_cam.rs     FlyCam component + 4 systems（toggle/look/move/wheel）
└── ui.rs          egui パネル：FPS、capture 状態、flood_pso_meta、dh_map
```

## 既知の課題（次セッションでの候補タスク）

詳細は `docs/00_開発記録.md` の「既知の課題」節：

- A. 性能：Mesa Zink で 10 FPS。LOD・チャンク粒度・dense grid のスパース化
- B. UX：マウス invert オプション、CursorGrabMode フォールバック、テクスチャ
- C. 機能：複数 NBT 並列ロード比較、dh_map の地形投影、CLI クエリ

---

何か聞かれたら、まず `docs/00_開発記録.md` を読み返すのが手っ取り早いです。
