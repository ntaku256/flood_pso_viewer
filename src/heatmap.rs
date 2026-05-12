//! 校正結果の K×K グリッドを 3D 地形上空に半透明色付きキューブとして描画する
//! オーバーレイ群。
//!
//! - **dh_map**（H キー）：水位補正マップ（青⇔赤の発散カラーマップ）
//! - **sigma_map**（J キー）：DEM 平滑化の場所別パラメータ（viridis 風連続カラーマップ）
//!
//! どちらも flood_pso 側 (`nbt_export.py` / `make_nbt_hd.py`) が
//! NBT 内 `flood_pso_meta.{dh_map, sigma_map}` に埋め込んだ Phase1 校正結果。
//! 既存の `WaterLayer` + V キーと同じパターンで Visibility をトグルする。

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::material::VoxelMaterial;
use crate::nbt_loader::FloodPsoMeta;

// ─────────────────────────────────────────────────────────────
// dh_map オーバーレイ（H キー、既存）
// ─────────────────────────────────────────────────────────────

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
    spawn_grid_overlay(
        commands, meshes, materials,
        world_size, dh, shape,
        /* y_offset_factor = */ 1.5,
        /* color_fn = */ |v, vmax| diverging_color(v, vmax),
        /* alpha = */ 0.70,
        initial_visible,
        OverlayKind::Dh,
    )
}

/// H キーで dh_map オーバーレイの Visibility をトグル
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

// ─────────────────────────────────────────────────────────────
// sigma_map オーバーレイ（J キー、Phase1 EX2 連動）
// ─────────────────────────────────────────────────────────────

/// sigma_map オーバーレイ entity に付けるマーカー（J キーで Visibility をトグル）
#[derive(Component)]
pub struct SigmaOverlay;

#[derive(Resource, Default, Clone, Copy)]
pub struct SigmaVisible(pub bool);

/// sigma_map を dh_map のさらに上空に色付きキューブとして spawn する。
/// FloodPsoMeta に `sigma_map` / `sigma_map_shape` が無い NBT では None を返す。
///
/// 値域は flood_sim の `sigma_levels = [0, 0.5, 1, 2, 4]` に合わせて [0, 4] を想定し、
/// 実値の絶対最大値で正規化する（黒→紫→赤→黄の viridis 風）。
pub fn spawn_sigma_overlay(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<VoxelMaterial>,
    world_size: [usize; 3],
    meta: &FloodPsoMeta,
    initial_visible: bool,
) -> Option<usize> {
    let sm = meta.sigma_map.as_ref()?;
    let shape = meta.sigma_map_shape.as_ref()?;
    spawn_grid_overlay(
        commands, meshes, materials,
        world_size, sm, shape,
        /* y_offset_factor = */ 4.0, // dh の更に上に分離
        /* color_fn = */ |v, vmax| viridis_like_color(v, vmax),
        /* alpha = */ 0.65,
        initial_visible,
        OverlayKind::Sigma,
    )
}

/// J キーで sigma_map オーバーレイの Visibility をトグル
pub fn toggle_sigma_visibility(
    keys: Res<ButtonInput<KeyCode>>,
    mut egui: EguiContexts,
    mut state: ResMut<SigmaVisible>,
    mut q: Query<&mut Visibility, With<SigmaOverlay>>,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }
    if !keys.just_pressed(KeyCode::KeyJ) { return; }
    state.0 = !state.0;
    let v = if state.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in q.iter_mut() { *vis = v; }
}

// ─────────────────────────────────────────────────────────────
// 共通：K×K のグリッドを地形上空に並べる
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum OverlayKind { Dh, Sigma }

/// shape = [rows, cols] の K×K 値配列を、地形上空に半透明キューブで並べる。
/// 値の正規化は配列内の絶対最大値（>=1e-6）で行う。
fn spawn_grid_overlay<F>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<VoxelMaterial>,
    world_size: [usize; 3],
    values: &[f32],
    shape: &[i32],
    y_offset_factor: f32,
    color_fn: F,
    alpha: f32,
    initial_visible: bool,
    kind: OverlayKind,
) -> Option<usize>
where
    F: Fn(f32, f32) -> (f32, f32, f32),
{
    if shape.len() != 2 { return None; }
    let kz = shape[0].max(0) as usize;
    let kx = shape[1].max(0) as usize;
    if kx == 0 || kz == 0 || values.len() != kx * kz { return None; }

    let nx = world_size[0] as f32;
    let ny = world_size[1] as f32;
    let nz = world_size[2] as f32;
    let cell_w = nx / kx as f32;
    let cell_d = nz / kz as f32;
    let cell_h = (ny * 0.02).max(1.5);

    let max_abs = values.iter().map(|v| v.abs()).fold(0.0_f32, f32::max).max(1e-6);

    let mesh_handle = meshes.add(Mesh::from(Cuboid::new(cell_w, cell_h, cell_d)));
    let visibility = if initial_visible { Visibility::Inherited } else { Visibility::Hidden };
    let y_top = ny + cell_h * y_offset_factor;

    let mut spawned = 0usize;
    for j in 0..kz {
        for i in 0..kx {
            let v = values[j * kx + i];
            let (r, g, b) = color_fn(v, max_abs);
            let mat_handle = materials.add(VoxelMaterial {
                color: Color::srgba(r, g, b, alpha).to_linear(),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
            });
            let cx = (i as f32 + 0.5) * cell_w;
            let cz = (j as f32 + 0.5) * cell_d;
            let id = commands.spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(cx, y_top, cz),
                visibility,
            )).id();
            match kind {
                OverlayKind::Dh    => { commands.entity(id).insert(HeatmapOverlay); }
                OverlayKind::Sigma => { commands.entity(id).insert(SigmaOverlay); }
            }
            spawned += 1;
        }
    }
    let label = match kind { OverlayKind::Dh => "dh_map", OverlayKind::Sigma => "sigma_map" };
    info!(
        "Spawned {label} overlay: {}×{} = {} cells, max|v|={:.3}, y_top={:.1}",
        kx, kz, spawned, max_abs, y_top
    );
    Some(spawned)
}

// ─────────────────────────────────────────────────────────────
// カラーマップ
// ─────────────────────────────────────────────────────────────

/// dh 値を 青(-) → 白(0) → 赤(+) の発散カラーマップに写像
fn diverging_color(v: f32, max_abs: f32) -> (f32, f32, f32) {
    let t = (v / max_abs).clamp(-1.0, 1.0);
    if t >= 0.0 {
        (1.0, 1.0 - t, 1.0 - t)         // 白 → 赤
    } else {
        let s = -t;
        (1.0 - s, 1.0 - s, 1.0)         // 白 → 青
    }
}

/// sigma 値を viridis 風（黒紫→青→緑→黄）の連続カラーマップに写像。
/// 入力は 0..max_abs を 0..1 に正規化（負値は 0 にクランプ）。
fn viridis_like_color(v: f32, max_abs: f32) -> (f32, f32, f32) {
    let t = (v / max_abs).clamp(0.0, 1.0);
    // 4 色キーポイント補間（viridis に寄せた近似）：
    //   0.00  暗紫    (0.27, 0.00, 0.33)
    //   0.33  青      (0.13, 0.45, 0.55)
    //   0.66  緑      (0.20, 0.72, 0.36)
    //   1.00  黄      (0.99, 0.91, 0.14)
    let stops: [(f32, f32, f32, f32); 4] = [
        (0.00, 0.27, 0.00, 0.33),
        (0.33, 0.13, 0.45, 0.55),
        (0.66, 0.20, 0.72, 0.36),
        (1.00, 0.99, 0.91, 0.14),
    ];
    for w in stops.windows(2) {
        let (t0, r0, g0, b0) = w[0];
        let (t1, r1, g1, b1) = w[1];
        if t <= t1 {
            let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return (
                r0 + (r1 - r0) * f,
                g0 + (g1 - g0) * f,
                b0 + (b1 - b0) * f,
            );
        }
    }
    (0.99, 0.91, 0.14)
}
