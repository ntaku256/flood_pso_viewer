//! egui で flood_pso_meta 情報パネルを表示

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::fly_cam::FlyCam;
use crate::nbt_loader::FloodPsoMeta;

#[derive(Resource, Default)]
pub struct ViewerStats {
    pub file_path: String,
    pub size_xyz: [i32; 3],
    pub n_block_entries: usize,
    pub n_filled_voxels: usize,
    pub n_quads: Option<usize>,
    pub n_vertices: Option<usize>,
    pub load_time_s: f32,
    pub mesh_time_s: f32,
}

#[derive(Resource, Default)]
pub struct LoadedMeta(pub FloodPsoMeta);

pub fn meta_panel_system(
    mut contexts: EguiContexts,
    stats: Res<ViewerStats>,
    meta_res: Res<LoadedMeta>,
    diag: Res<DiagnosticsStore>,
    fly_q: Query<&FlyCam>,
) {
    let meta = &meta_res.0;
    let fps = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let frame_ms = diag
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed());

    // Fly カメラのステータス（存在しなければ Orbit モード）
    let fly_state: Option<(bool, f32)> = fly_q.iter().next()
        .map(|c| (c.captured, c.speed));

    egui::SidePanel::right("flood_pso_meta_panel")
        .resizable(true)
        .min_width(280.0)
        .default_width(330.0)
        .show(contexts.ctx_mut(), |ui| {
            ui.heading("flood_pso viewer");
            ui.horizontal(|ui| {
                let fps_str = fps.map(|f| format!("{f:>5.1} FPS")).unwrap_or_else(|| "—".into());
                let ms_str  = frame_ms.map(|m| format!("{m:>5.1} ms/frame")).unwrap_or_else(|| "—".into());
                ui.colored_label(egui::Color32::LIGHT_GREEN, fps_str);
                ui.label("·");
                ui.colored_label(egui::Color32::LIGHT_BLUE, ms_str);
            });
            // カメラ状態表示
            ui.horizontal(|ui| {
                match fly_state {
                    Some((true, speed)) => {
                        ui.colored_label(egui::Color32::YELLOW, "● mouse: CAPTURED");
                        ui.label(format!("speed {speed:.0}"));
                    }
                    Some((false, speed)) => {
                        ui.colored_label(egui::Color32::LIGHT_GRAY, "○ mouse: free  (Tab to capture)");
                        ui.label(format!("speed {speed:.0}"));
                    }
                    None => {
                        ui.colored_label(egui::Color32::LIGHT_GRAY, "Orbit camera (F to switch to Fly)");
                    }
                }
            });
            ui.separator();
            ui.label(format!("file: {}", stats.file_path));
            ui.label(format!("size XYZ: {} × {} × {}",
                             stats.size_xyz[0], stats.size_xyz[1], stats.size_xyz[2]));
            ui.label(format!("block entries:    {}", stats.n_block_entries));
            ui.label(format!("filled voxels:    {}", stats.n_filled_voxels));
            if let (Some(q), Some(v)) = (stats.n_quads, stats.n_vertices) {
                ui.label(format!("quads (greedy):   {}", q));
                ui.label(format!("vertices:         {}", v));
            }
            ui.label(format!("load: {:.2}s   mesh: {:.2}s",
                             stats.load_time_s, stats.mesh_time_s));

            ui.separator();
            ui.heading("flood_pso_meta");
            egui::Grid::new("meta_grid").striped(true).num_columns(2).show(ui, |ui| {
                row(ui, "experiment", &meta.experiment);
                row(ui, "method",     &meta.method);
                row(ui, "method_long",&meta.method_long);
                row_i(ui, "K",         meta.k);
                row_i(ui, "D",         meta.d);
                row_i(ui, "seed",      meta.seed);
                row_f(ui, "loss",      meta.loss);
                row_f(ui, "iou",       meta.iou);
                row_f(ui, "dh_rmse",   meta.dh_rmse);
                row_f(ui, "water_level", meta.water_level);
                row_f(ui, "sigma",     meta.sigma);
                row_i(ui, "n_evals",   meta.n_evals);
                row_f(ui, "elapsed_s", meta.elapsed_s);
                row_i(ui, "K_s",       meta.k_s);
                row(ui, "preset",      &meta.preset);
                row(ui, "study_area",  &meta.study_area);
                row(ui, "dem_source",  &meta.dem_source);
                row(ui, "git_revision",&meta.git_revision);
                row(ui, "timestamp",   &meta.timestamp_utc);
            });

            // dh_map のヒートマップ（簡易）
            if let (Some(dh), Some(shape)) = (&meta.dh_map, &meta.dh_map_shape) {
                if shape.len() == 2 {
                    ui.separator();
                    let stats = grid_stats(dh);
                    ui.heading(format!("dh_map ({}×{})", shape[0], shape[1]));
                    ui.label(format!(
                        "min {:.2}  mean {:.2}  max {:.2}  |max| {:.2}",
                        stats.min, stats.mean, stats.max, stats.max_abs,
                    ));
                    draw_grid_heatmap(ui, dh, shape, ColorMap::Diverging);
                    ui.small("3D overlay: H key to toggle (blue↔red, top of world)");
                }
            }

            // sigma_map のヒートマップ（Phase1 EX2 連動）
            if let (Some(sm), Some(shape)) = (&meta.sigma_map, &meta.sigma_map_shape) {
                if shape.len() == 2 {
                    ui.separator();
                    let stats = grid_stats(sm);
                    ui.heading(format!("sigma_map ({}×{})", shape[0], shape[1]));
                    ui.label(format!(
                        "min {:.2}  mean {:.2}  max {:.2}",
                        stats.min, stats.mean, stats.max,
                    ));
                    draw_grid_heatmap(ui, sm, shape, ColorMap::Viridis);
                    ui.small("3D overlay: J key to toggle (viridis, above dh_map)");
                }
            }

            ui.separator();
            ui.collapsing("raw flood_pso_meta", |ui| {
                egui::Grid::new("raw_grid").striped(true).num_columns(2).show(ui, |ui| {
                    for (k, v) in &meta.raw {
                        ui.monospace(k);
                        ui.monospace(v);
                        ui.end_row();
                    }
                });
            });
        });
}

fn row(ui: &mut egui::Ui, label: &str, val: &Option<String>) {
    ui.label(label);
    ui.monospace(val.as_deref().unwrap_or("-"));
    ui.end_row();
}
fn row_i(ui: &mut egui::Ui, label: &str, val: Option<i32>) {
    ui.label(label);
    ui.monospace(match val { Some(v) => v.to_string(), None => "-".into() });
    ui.end_row();
}
fn row_f(ui: &mut egui::Ui, label: &str, val: Option<f64>) {
    ui.label(label);
    ui.monospace(match val { Some(v) => format!("{:.4}", v), None => "-".into() });
    ui.end_row();
}

struct GridStats {
    min: f32,
    max: f32,
    mean: f32,
    max_abs: f32,
}

fn grid_stats(v: &[f32]) -> GridStats {
    if v.is_empty() {
        return GridStats { min: 0.0, max: 0.0, mean: 0.0, max_abs: 0.0 };
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0_f32;
    let mut max_abs = 0.0_f32;
    for &x in v {
        if x < min { min = x; }
        if x > max { max = x; }
        sum += x;
        let a = x.abs();
        if a > max_abs { max_abs = a; }
    }
    GridStats { min, max, mean: sum / v.len() as f32, max_abs }
}

#[derive(Copy, Clone)]
enum ColorMap { Diverging, Viridis }

fn draw_grid_heatmap(ui: &mut egui::Ui, vals: &[f32], shape: &[i32], cmap: ColorMap) {
    let kz = shape[0].max(1) as usize; // rows
    let kx = shape[1].max(1) as usize; // cols
    if vals.len() != kx * kz { ui.label("(shape mismatch)"); return; }
    let max_abs = vals.iter().fold(1e-6f32, |a, b| a.max(b.abs()));
    let max_val = vals.iter().cloned().fold(1e-6f32, f32::max);
    let cell = 14.0;
    let total_w = cell * kx as f32;
    let total_h = cell * kz as f32;
    let (resp, painter) = ui.allocate_painter(
        egui::vec2(total_w + 8.0, total_h + 8.0),
        egui::Sense::hover()
    );
    let origin = resp.rect.min + egui::vec2(4.0, 4.0);
    for j in 0..kz {
        for i in 0..kx {
            let v = vals[j * kx + i];
            let (r, g, b) = match cmap {
                ColorMap::Diverging => diverging_rgb(v, max_abs),
                ColorMap::Viridis   => viridis_rgb(v, max_val),
            };
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(i as f32 * cell, j as f32 * cell),
                egui::vec2(cell - 1.0, cell - 1.0),
            );
            painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(r, g, b));
        }
    }
}

fn diverging_rgb(v: f32, max_abs: f32) -> (u8, u8, u8) {
    let t = (v / max_abs).clamp(-1.0, 1.0);
    if t >= 0.0 {
        (255, ((1.0 - t) * 255.0) as u8, ((1.0 - t) * 255.0) as u8)
    } else {
        (((1.0 + t) * 255.0) as u8, ((1.0 + t) * 255.0) as u8, 255)
    }
}

fn viridis_rgb(v: f32, max_val: f32) -> (u8, u8, u8) {
    let t = (v / max_val).clamp(0.0, 1.0);
    // heatmap.rs::viridis_like_color と同じキー色を使う
    let stops: [(f32, f32, f32, f32); 4] = [
        (0.00, 0.27, 0.00, 0.33),
        (0.33, 0.13, 0.45, 0.55),
        (0.66, 0.20, 0.72, 0.36),
        (1.00, 0.99, 0.91, 0.14),
    ];
    let (r, g, b) = {
        let mut out = (0.99, 0.91, 0.14);
        for w in stops.windows(2) {
            let (t0, r0, g0, b0) = w[0];
            let (t1, r1, g1, b1) = w[1];
            if t <= t1 {
                let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
                out = (
                    r0 + (r1 - r0) * f,
                    g0 + (g1 - g0) * f,
                    b0 + (b1 - b0) * f,
                );
                break;
            }
        }
        out
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
