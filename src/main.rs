//! flood_pso_viewer — flood_pso が出力する Minecraft Structure NBT をネイティブ GPU で閲覧する
//!
//! 実行例:
//!   cargo run --release -- ../flood_pso/results/nbt/hd/gobo_hd_K16_seed0_md_5m_ccpso2.nbt
//!   ./target/release/flood_pso_viewer <NBT> --no-water     # 浸水前の地形だけ
//!   ./target/release/flood_pso_viewer <NBT> --camera orbit # 起動時から軌道カメラ
//!
//! 操作:
//!   [Fly モード = デフォルト]
//!     WASD       : 前後左右
//!     Space      : 上昇    Shift : 下降    Ctrl押し : ダッシュ4倍
//!     Mouse      : 視点回転（capture 時）
//!     Wheel      : 移動速度調整
//!     Tab        : マウス capture / release
//!   [Orbit モード]
//!     左ドラッグ : 軌道回転   右ドラッグ : 平行移動   Wheel : ズーム
//!   [共通]
//!     F          : Fly / Orbit 切替
//!     V          : 水・氷ブロック表示の ON/OFF（浸水前/後を比較）
//!     Esc        : 終了

mod voxel;
mod nbt_loader;
mod greedy_mesh;
mod render;
mod material;
mod fly_cam;
mod ui;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use bevy::core_pipeline::core_3d::Camera3d;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::camera::PerspectiveProjection;
use bevy::render::settings::{Backends, PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::view::Msaa;
use bevy::render::RenderPlugin;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_egui::{EguiContexts, EguiPlugin};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use clap::{Parser, ValueEnum};

use crate::fly_cam::{FlyCam, FlyCamPlugin};
use crate::greedy_mesh::build_meshes;
use crate::material::{VoxelMaterial, VoxelMaterialPlugin};
use crate::render::{spawn_voxel_world, WaterLayer};
use crate::ui::{meta_panel_system, LoadedMeta, ViewerStats};

#[derive(Parser, Debug, Resource, Clone)]
#[command(version, about = "Native GPU 3D viewer for flood_pso NBT outputs")]
struct Cli {
    /// 表示する .nbt ファイル (gzip 圧縮された Minecraft Structure NBT)
    #[arg(value_name = "NBT_FILE")]
    file: PathBuf,
    /// 起動時にカメラ距離を世界サイズの何倍にするか（orbit 用）
    #[arg(long, default_value_t = 1.4)]
    fit_scale: f32,
    /// 水・氷ブロックを読み込まない（浸水前の地形のみ）
    #[arg(long, default_value_t = false)]
    no_water: bool,
    /// 起動時のカメラモード
    #[arg(long, value_enum, default_value_t = CameraMode::Fly)]
    camera: CameraMode,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
enum CameraMode { Fly, Orbit }

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
enum CurrentCam { Fly, Orbit }

#[derive(Resource, Default, Clone, Copy)]
struct WaterVisible(bool);

fn main() -> Result<()> {
    let cli = Cli::parse();

    // NBT を最初に読み込んでおく
    println!("Loading {}...", cli.file.display());
    let t0 = Instant::now();
    let mut loaded = nbt_loader::load_structure_nbt(&cli.file)?;
    let load_time = t0.elapsed().as_secs_f32();
    if cli.no_water {
        // 水・氷を air に置き換えて bbox も再計算
        loaded.grid.strip_water();
    }
    let n_filled = loaded.grid.count_non_air();
    println!(
        "  size = {} × {} × {}   block entries = {}   filled = {}{}   load = {:.2}s",
        loaded.size_xyz[0], loaded.size_xyz[1], loaded.size_xyz[2],
        loaded.n_block_entries, n_filled,
        if cli.no_water { "  [no water]" } else { "" },
        load_time,
    );

    let t1 = Instant::now();
    let groups = build_meshes(&loaded.grid);
    let mesh_time = t1.elapsed().as_secs_f32();
    let total_quads: usize = groups.iter().map(|(_, b)| b.indices.len() / 6).sum();
    let total_verts: usize = groups.iter().map(|(_, b)| b.positions.len()).sum();
    println!(
        "  greedy mesh: {} verts, {} quads, {:.2}s",
        total_verts, total_quads, mesh_time,
    );

    let stats = ViewerStats {
        file_path: cli.file.display().to_string(),
        size_xyz: loaded.size_xyz,
        n_block_entries: loaded.n_block_entries,
        n_filled_voxels: n_filled,
        n_quads: Some(total_quads),
        n_vertices: Some(total_verts),
        load_time_s: load_time,
        mesh_time_s: mesh_time,
    };
    let meta = LoadedMeta(loaded.meta.clone());

    // バックエンド選定（WSL2 では GL 優先 / native は Vulkan 優先）
    let is_wsl = std::fs::read_to_string("/proc/version")
        .map(|s| { let l = s.to_lowercase(); l.contains("microsoft") || l.contains("wsl") })
        .unwrap_or(false);
    let backends = match std::env::var("FLOOD_PSO_VIEWER_BACKEND").ok().as_deref() {
        Some("vulkan")  => Backends::VULKAN,
        Some("dx12")    => Backends::DX12,
        Some("gl")      => Backends::GL,
        Some("metal")   => Backends::METAL,
        Some("all")     => Backends::all(),
        _               => if is_wsl {
            Backends::GL | Backends::DX12 | Backends::METAL | Backends::VULKAN
        } else {
            Backends::VULKAN | Backends::DX12 | Backends::METAL | Backends::GL
        },
    };
    eprintln!("[backend] is_wsl={is_wsl}, backends mask = {:?}", backends);

    let initial_cam = match cli.camera {
        CameraMode::Fly   => CurrentCam::Fly,
        CameraMode::Orbit => CurrentCam::Orbit,
    };

    App::new()
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("flood_pso_viewer — {}", cli.file.display()),
                    resolution: (1400.0, 900.0).into(),
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    backends: Some(backends),
                    power_preference: PowerPreference::HighPerformance,
                    ..default()
                }),
                ..default()
            })
            // Mesa Zink GL の wgpu-hal heuristic warning スパムを抑制。
            // 元々は ERROR レベルだが致命でないので OFF にする。
            // 必要なら RUST_LOG=wgpu_hal=info で再表示可能。
            .set(LogPlugin {
                filter: "wgpu_hal::gles=off,wgpu_core=warn,naga=warn,bevy_render=warn".into(),
                level: bevy::log::Level::INFO,
                ..default()
            }))
        .add_plugins(EguiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(FlyCamPlugin)
        .add_plugins(VoxelMaterialPlugin)
        .add_plugins(FrameTimeDiagnosticsPlugin)
        .insert_resource(stats)
        .insert_resource(meta)
        .insert_resource(cli.clone())
        .insert_resource(LoadedGrid(Some(loaded)))
        .insert_resource(ClearColor(Color::srgb(0.74, 0.86, 0.95)))
        .insert_resource(initial_cam)
        .insert_resource(WaterVisible(!cli.no_water))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (
            meta_panel_system,
            exit_on_esc,
            toggle_camera_mode,
            toggle_water_visibility,
        ))
        .run();

    Ok(())
}

#[derive(Resource)]
struct LoadedGrid(Option<nbt_loader::LoadedNbt>);

fn setup_scene(
    mut commands: Commands,
    cli: Res<Cli>,
    cur_cam: Res<CurrentCam>,
    mut grid_res: ResMut<LoadedGrid>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
) {
    let loaded = grid_res.0.take().expect("LoadedGrid was already taken");
    let size = loaded.grid.size;

    spawn_voxel_world(&mut commands, &mut meshes, &mut materials, &loaded.grid);

    let half = Vec3::new(size[0] as f32 * 0.5, 0.0, size[2] as f32 * 0.5);
    let max_dim = size[0].max(size[2]) as f32;
    let cam_dist = max_dim * cli.fit_scale;
    let cam_pos = Vec3::new(half.x + cam_dist, max_dim * 0.6, half.z + cam_dist);
    let target  = Vec3::new(half.x, size[1] as f32 * 0.25, half.z);

    // far クリップ平面を世界対角の数倍まで広げる（デフォルト 1000 だと巨大ワールドが消える）
    let world_diag = ((size[0].pow(2) + size[1].pow(2) + size[2].pow(2)) as f32).sqrt();
    let far = (world_diag * 4.0).max(10000.0);

    let mut entity = commands.spawn((
        Camera3d::default(),
        Transform::from_translation(cam_pos).looking_at(target, Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            near: 0.1,
            far,
            fov: std::f32::consts::FRAC_PI_4,    // 45°
            aspect_ratio: 16.0 / 9.0,
        }),
        Msaa::Off, // Mesa Zink でのフラグメントコスト削減
    ));
    match *cur_cam {
        CurrentCam::Fly => {
            // Fly mode: 初期視線方向に合わせて yaw/pitch をセット
            let dir = (target - cam_pos).normalize();
            let yaw   = dir.x.atan2(-dir.z);            // -Z 前方
            let pitch = dir.y.asin();
            let speed = max_dim * 0.04;                  // ワールドサイズに比例
            entity.insert(FlyCam {
                yaw, pitch,
                speed: speed.max(20.0),
                ..default()
            });
        }
        CurrentCam::Orbit => {
            entity.insert(PanOrbitCamera {
                focus: target,
                radius: Some(cam_dist * 1.2),
                yaw: Some(std::f32::consts::FRAC_PI_4),
                pitch: Some(-std::f32::consts::FRAC_PI_6),
                ..default()
            });
        }
    }

    eprintln!("[camera] pos={cam_pos:?} target={target:?} far={far:.0}");

    let _ = loaded;
}

/// F キーで Fly / Orbit を切り替える。Camera コンポーネントを動的に差し替え。
fn toggle_camera_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut egui: EguiContexts,
    mut current: ResMut<CurrentCam>,
    mut commands: Commands,
    mut q_cam: Query<(Entity, &mut Transform), With<Camera3d>>,
    q_orbit: Query<&PanOrbitCamera>,
    q_fly: Query<&FlyCam>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }
    if !keys.just_pressed(KeyCode::KeyF) { return; }

    for (ent, mut tf) in q_cam.iter_mut() {
        match *current {
            CurrentCam::Fly => {
                // Fly → Orbit
                let pos = tf.translation;
                let forward = tf.forward();
                let target = pos + *forward * 200.0;
                commands.entity(ent).remove::<FlyCam>();
                commands.entity(ent).insert(PanOrbitCamera {
                    focus: target,
                    radius: Some((target - pos).length()),
                    ..default()
                });
                if let Ok(mut win) = windows.get_single_mut() {
                    win.cursor_options.grab_mode = CursorGrabMode::None;
                    win.cursor_options.visible = true;
                }
                *current = CurrentCam::Orbit;
                let _ = q_fly; // suppress unused warning
            }
            CurrentCam::Orbit => {
                // Orbit → Fly
                let dir = tf.forward();
                let yaw   = dir.x.atan2(-dir.z);
                let pitch = dir.y.asin();
                commands.entity(ent).remove::<PanOrbitCamera>();
                commands.entity(ent).insert(FlyCam {
                    yaw, pitch,
                    speed: 60.0,
                    ..default()
                });
                let _ = tf.as_mut(); // ensure Transform is preserved
                let _ = q_orbit;
                *current = CurrentCam::Fly;
            }
        }
    }
}

/// V キーで水・氷の Visibility をトグル
fn toggle_water_visibility(
    keys: Res<ButtonInput<KeyCode>>,
    mut egui: EguiContexts,
    mut state: ResMut<WaterVisible>,
    mut q: Query<&mut Visibility, With<WaterLayer>>,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }
    if !keys.just_pressed(KeyCode::KeyV) { return; }
    state.0 = !state.0;
    let v = if state.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in q.iter_mut() { *vis = v; }
}

fn exit_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: EventWriter<AppExit>,
    mut egui: EguiContexts,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
