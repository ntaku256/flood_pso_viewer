//! dh_map (K×K の水位補正マップ) を 3D 地形上空に半透明色付きキューブとして描画。
//!
//! flood_pso 側 (nbt_export.py) が NBT 内 `flood_pso_meta.dh_map` に埋め込んだ
//! Phase1 校正結果を、3D 地形のキービジュアルとして投影する。
//! H キーで Visibility をトグル可能（既存の WaterLayer + V キーと同パターン）。

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::material::VoxelMaterial;
use crate::nbt_loader::FloodPsoMeta;

/// dh_map オーバーレイ entity に付けるマーカー（H キーで Visibility をトグル）
#[derive(Component)]
pub struct HeatmapOverlay;

#[derive(Resource, Default, Clone, Copy)]
pub struct HeatmapVisible(pub bool);

/// dh_map を 3D 地形の上空に色付きキューブとして spawn する。
/// FloodPsoMeta に `dh_map` / `dh_map_shape` が無い NBT では何もせず None を返す。
///
/// world_size      : VoxelGrid の [nx, ny, nz]（ボクセル単位）
/// initial_visible : 初期表示状態
///
/// Returns: spawn したセル数（dh_map が無ければ None）
pub fn spawn_heatmap_overlay(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<VoxelMaterial>,
    world_size: [usize; 3],
    meta: &FloodPsoMeta,
    initial_visible: bool,
) -> Option<usize> {
    let dh = meta.dh_map.as_ref()?;
    let shape = meta.dh_map_shape.as_ref()?;
    if shape.len() != 2 { return None; }
    // flood_pso 側では dh_map_shape = [K, K] = (rows, cols) として埋め込み
    let kz = shape[0].max(0) as usize;
    let kx = shape[1].max(0) as usize;
    if kx == 0 || kz == 0 || dh.len() != kx * kz { return None; }

    let nx = world_size[0] as f32;
    let ny = world_size[1] as f32;
    let nz = world_size[2] as f32;
    let cell_w = nx / kx as f32;
    let cell_d = nz / kz as f32;
    // 厚みは世界高の 2% 程度（最低 1.5 ボクセル）
    let cell_h = (ny * 0.02).max(1.5);

    // 発散カラーマップの値域（絶対最大、ゼロ割回避）
    let max_abs = dh.iter().map(|v| v.abs()).fold(0.0_f32, f32::max).max(1e-6);

    // 共通 mesh を 1 つだけ追加（K×K セル全部で再利用）
    let mesh_handle = meshes.add(Mesh::from(Cuboid::new(cell_w, cell_h, cell_d)));

    let visibility = if initial_visible { Visibility::Inherited } else { Visibility::Hidden };

    // Y 位置：ワールド天井の少し上に浮かす（地形を覆い隠さない）
    let y_top = ny + cell_h * 1.5;

    let mut spawned = 0usize;
    for j in 0..kz {
        for i in 0..kx {
            let v = dh[j * kx + i];
            let color = diverging_color(v, max_abs);
            let mat_handle = materials.add(VoxelMaterial {
                color: color.to_linear(),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
            });
            let cx = (i as f32 + 0.5) * cell_w;
            let cz = (j as f32 + 0.5) * cell_d;
            commands.spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(cx, y_top, cz),
                visibility,
                HeatmapOverlay,
            ));
            spawned += 1;
        }
    }
    info!(
        "Spawned heatmap overlay: {}×{} = {} cells, max|Δh|={:.3} m, y_top={:.1}",
        kx, kz, spawned, max_abs, y_top
    );
    Some(spawned)
}

/// dh 値を 青(-) → 白(0) → 赤(+) の発散カラーマップに写像（alpha 0.70 半透明）
fn diverging_color(v: f32, max_abs: f32) -> Color {
    let t = (v / max_abs).clamp(-1.0, 1.0);
    let alpha = 0.70_f32;
    if t >= 0.0 {
        // 白 → 赤
        Color::srgba(1.0, 1.0 - t, 1.0 - t, alpha)
    } else {
        let s = -t;
        // 白 → 青
        Color::srgba(1.0 - s, 1.0 - s, 1.0, alpha)
    }
}

/// H キーで heatmap オーバーレイの Visibility をトグル
pub fn toggle_heatmap_visibility(
    keys: Res<ButtonInput<KeyCode>>,
    mut egui: EguiContexts,
    mut state: ResMut<HeatmapVisible>,
    mut q: Query<&mut Visibility, With<HeatmapOverlay>>,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }
    if !keys.just_pressed(KeyCode::KeyH) { return; }
    state.0 = !state.0;
    let v = if state.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in q.iter_mut() { *vis = v; }
}
