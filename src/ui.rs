//! egui で flood_pso_meta 情報パネルを表示

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

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
) {
    let meta = &meta_res.0;
    let fps = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let frame_ms = diag
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed());

    egui::SidePanel::right("flood_pso_meta_panel")
        .resizable(true)
        .min_width(280.0)
        .default_width(330.0)
        .show(contexts.ctx_mut(), |ui| {
            ui.heading("flood_pso viewer");
            // FPS / frame time を強調表示
            ui.horizontal(|ui| {
                let fps_str = fps.map(|f| format!("{f:>5.1} FPS")).unwrap_or_else(|| "—".into());
                let ms_str  = frame_ms.map(|m| format!("{m:>5.1} ms/frame")).unwrap_or_else(|| "—".into());
                ui.colored_label(egui::Color32::LIGHT_GREEN, fps_str);
                ui.label("·");
                ui.colored_label(egui::Color32::LIGHT_BLUE, ms_str);
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
                    ui.heading(format!("dh_map ({}×{})", shape[0], shape[1]));
                    draw_dh_heatmap(ui, dh, shape);
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

fn draw_dh_heatmap(ui: &mut egui::Ui, dh: &[f32], shape: &[i32]) {
    let kx = shape[0].max(1) as usize;
    let ky = shape[1].max(1) as usize;
    if dh.len() != kx * ky { ui.label("(dh_map shape mismatch)"); return; }
    let max_abs = dh.iter().fold(1e-6f32, |a, b| a.max(b.abs()));
    let cell = 14.0;
    let total_w = cell * kx as f32;
    let total_h = cell * ky as f32;
    let (resp, painter) = ui.allocate_painter(
        egui::vec2(total_w + 8.0, total_h + 8.0),
        egui::Sense::hover()
    );
    let origin = resp.rect.min + egui::vec2(4.0, 4.0);
    for j in 0..ky {
        for i in 0..kx {
            let v = dh[j * kx + i];
            let t = (v / max_abs).clamp(-1.0, 1.0);
            // 青(-) → 白 → 赤(+) のマップ
            let (r, g, b) = if t >= 0.0 {
                (255u8, ((1.0 - t) * 255.0) as u8, ((1.0 - t) * 255.0) as u8)
            } else {
                (((1.0 + t) * 255.0) as u8, ((1.0 + t) * 255.0) as u8, 255u8)
            };
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(i as f32 * cell, j as f32 * cell),
                egui::vec2(cell - 1.0, cell - 1.0),
            );
            painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(r, g, b));
        }
    }
}
