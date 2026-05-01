//! flood_pso_viewer — flood_pso が出力する Minecraft Structure NBT をネイティブ GPU で閲覧する
//!
//! 実行例:
//!   cargo run --release -- ../flood_pso/results/nbt/hd/gobo_hd_K16_seed0_md_5m_ccpso2.nbt
//!
//! 操作:
//!   左ドラッグ: 回転   右ドラッグ: 平行移動   ホイール: ズーム   ESC: 終了

mod voxel;
mod nbt_loader;
mod greedy_mesh;
mod render;
mod material;
mod ui;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use bevy::render::settings::{Backends, PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use clap::Parser;

use crate::greedy_mesh::build_meshes;
use crate::material::{VoxelMaterial, VoxelMaterialPlugin};
use crate::render::spawn_voxel_world;
use crate::ui::{meta_panel_system, LoadedMeta, ViewerStats};

#[derive(Parser, Debug, Resource, Clone)]
#[command(version, about = "Native GPU 3D viewer for flood_pso NBT outputs")]
struct Cli {
    /// 表示する .nbt ファイル (gzip 圧縮された Minecraft Structure NBT)
    #[arg(value_name = "NBT_FILE")]
    file: PathBuf,
    /// 起動時にカメラ距離を世界サイズの何倍にするか
    #[arg(long, default_value_t = 1.4)]
    fit_scale: f32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // NBT を最初に読み込んでおく（メイン thread でブロッキング）
    println!("Loading {}...", cli.file.display());
    let t0 = Instant::now();
    let loaded = nbt_loader::load_structure_nbt(&cli.file)?;
    let load_time = t0.elapsed().as_secs_f32();
    let n_filled = loaded.grid.count_non_air();
    println!(
        "  size = {} × {} × {}   block entries = {}   filled = {}   load = {:.2}s",
        loaded.size_xyz[0], loaded.size_xyz[1], loaded.size_xyz[2],
        loaded.n_block_entries, n_filled, load_time,
    );

    // greedy meshing も先に走らせて統計を取る（描画は後段）
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

    // バックエンド選定：
    //   - VoxelMaterial（自作 WGSL）で PBR バイパス済 → GL_EXT_texture_shadow_lod 問題は無い。
    //   - WSL2 では Vulkan は lavapipe(CPU) しか居ないことが多いため、
    //     GPU ハードに到達するルート Mesa Zink (GL→Vulkan→D3D12→GPU) を使う必要があり GL を最優先。
    //   - ネイティブ Linux では Vulkan が直接 GPU を掴む。
    //   - 環境変数 FLOOD_PSO_VIEWER_BACKEND で強制指定可能 (vulkan|dx12|gl|metal|all)。
    let is_wsl = std::fs::read_to_string("/proc/version")
        .map(|s| { let l = s.to_lowercase(); l.contains("microsoft") || l.contains("wsl") })
        .unwrap_or(false);
    let backends = match std::env::var("FLOOD_PSO_VIEWER_BACKEND").ok().as_deref() {
        Some("vulkan")  => Backends::VULKAN,
        Some("dx12")    => Backends::DX12,
        Some("gl")      => Backends::GL,
        Some("metal")   => Backends::METAL,
        Some("all")     => Backends::all(),
        None | Some(_)  => if is_wsl {
            // WSL2: lavapipe(CPU) を避けるため Vulkan を後ろに
            Backends::GL | Backends::DX12 | Backends::METAL | Backends::VULKAN
        } else {
            Backends::VULKAN | Backends::DX12 | Backends::METAL | Backends::GL
        },
    };
    eprintln!("[backend] is_wsl={is_wsl}, backends mask = {:?}", backends);

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
            }))
        .add_plugins(EguiPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .add_plugins(VoxelMaterialPlugin)
        .insert_resource(stats)
        .insert_resource(meta)
        .insert_resource(cli.clone())
        .insert_resource(LoadedGrid(Some(loaded)))
        .insert_resource(ClearColor(Color::srgb(0.74, 0.86, 0.95)))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (
            meta_panel_system,
            exit_on_esc.run_if(input_just_pressed(KeyCode::Escape)),
        ))
        .run();

    Ok(())
}

#[derive(Resource)]
struct LoadedGrid(Option<nbt_loader::LoadedNbt>);

fn setup_scene(
    mut commands: Commands,
    cli: Res<Cli>,
    mut grid_res: ResMut<LoadedGrid>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelMaterial>>,
) {
    let loaded = grid_res.0.take().expect("LoadedGrid was already taken");
    let size = loaded.grid.size;

    spawn_voxel_world(&mut commands, &mut meshes, &mut materials, &loaded.grid);

    // 中心合わせ：VoxelWorldRoot 全体を原点中心に
    let half = [
        size[0] as f32 * 0.5,
        0.0,
        size[2] as f32 * 0.5,
    ];
    // 既にスポーンされた最後の root を取り直すのではなく、
    // shift transform を別 entity として乗せる手もあるが、ここでは Camera 側で原点を覗く方式に。

    let max_dim = size[0].max(size[2]) as f32;
    let cam_dist = max_dim * cli.fit_scale;
    let cam_pos = Vec3::new(half[0] + cam_dist, max_dim * 0.6, half[2] + cam_dist);
    let target  = Vec3::new(half[0], size[1] as f32 * 0.25, half[2]);

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(cam_pos).looking_at(target, Vec3::Y),
        PanOrbitCamera {
            focus: target,
            radius: Some(cam_dist * 1.2),
            yaw: Some(std::f32::consts::FRAC_PI_4),
            pitch: Some(-std::f32::consts::FRAC_PI_6),
            ..default()
        },
    ));

    // 太陽光
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false, // 数百万 quad 規模ではシャドウマップが重い
            illuminance: 14_000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.4, 0.0)),
    ));

    // 環境光
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 250.0,
    });

    let _ = loaded; // grid_res からは take 済み
}

fn exit_on_esc(mut exit: EventWriter<AppExit>) {
    exit.send(AppExit::Success);
}
