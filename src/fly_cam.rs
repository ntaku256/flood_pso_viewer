//! Minecraft Creative 風の自由飛行カメラ。
//!
//! 操作:
//!   WASD             : 前後左右（カメラ局所）
//!   Space            : 上昇（ワールド +Y）
//!   Shift            : 下降（ワールド -Y）
//!   Mouse            : 視点回転（capture 中のみ）
//!   Wheel            : 移動速度を調整
//!   Tab / Esc(短押し): マウスキャプチャの ON/OFF
//!   F                : Fly / Orbit 切替（main.rs 側でハンドル）
//!
//! egui パネル上では camera 入力を奪わないよう判定する。

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy_egui::EguiContexts;

#[derive(Component, Debug)]
pub struct FlyCam {
    pub yaw: f32,            // ラジアン
    pub pitch: f32,
    pub speed: f32,          // 1秒あたり進む blocks
    pub mouse_sensitivity: f32,
    pub captured: bool,
}

impl Default for FlyCam {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            speed: 60.0,
            mouse_sensitivity: 0.0025,
            captured: false,
        }
    }
}

pub struct FlyCamPlugin;
impl Plugin for FlyCamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            toggle_capture_system,
            mouse_look_system,
            keyboard_movement_system,
            wheel_speed_system,
        ));
    }
}

/// Tab または Esc 短押しで capture 切替
fn toggle_capture_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut FlyCam>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }

    let toggle = keys.just_pressed(KeyCode::Tab);
    if !toggle { return; }

    for mut cam in q.iter_mut() {
        cam.captured = !cam.captured;
        if let Ok(mut win) = windows.get_single_mut() {
            if cam.captured {
                win.cursor_options.grab_mode = CursorGrabMode::Locked;
                win.cursor_options.visible = false;
            } else {
                win.cursor_options.grab_mode = CursorGrabMode::None;
                win.cursor_options.visible = true;
            }
        }
    }
}

/// マウス移動 → yaw/pitch
fn mouse_look_system(
    mut motion: EventReader<MouseMotion>,
    mut q: Query<(&mut FlyCam, &mut Transform)>,
) {
    let delta: Vec2 = motion.read().map(|e| e.delta).sum();
    for (mut cam, mut tf) in q.iter_mut() {
        if !cam.captured { continue; }
        cam.yaw   -= delta.x * cam.mouse_sensitivity;
        cam.pitch -= delta.y * cam.mouse_sensitivity;
        // pitch は ±89° にクランプして反転を防ぐ
        let clamp = std::f32::consts::FRAC_PI_2 - 0.01;
        cam.pitch = cam.pitch.clamp(-clamp, clamp);
        tf.rotation = Quat::from_axis_angle(Vec3::Y, cam.yaw)
            * Quat::from_axis_angle(Vec3::X, cam.pitch);
    }
}

/// WASD/Space/Shift で並進
fn keyboard_movement_system(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<(&FlyCam, &mut Transform)>,
    mut egui: EguiContexts,
) {
    if egui.ctx_mut().wants_keyboard_input() { return; }

    for (cam, mut tf) in q.iter_mut() {
        if !cam.captured { continue; }

        // カメラ局所方向（yaw だけで水平投影、Creative の感覚に合わせる）
        let yaw_rot = Quat::from_axis_angle(Vec3::Y, cam.yaw);
        let forward = yaw_rot * Vec3::NEG_Z;
        let right   = yaw_rot * Vec3::X;
        let up      = Vec3::Y;

        let mut dir = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW)         { dir += forward; }
        if keys.pressed(KeyCode::KeyS)         { dir -= forward; }
        if keys.pressed(KeyCode::KeyD)         { dir += right;   }
        if keys.pressed(KeyCode::KeyA)         { dir -= right;   }
        if keys.pressed(KeyCode::Space)        { dir += up;      }
        if keys.pressed(KeyCode::ShiftLeft)
            || keys.pressed(KeyCode::ShiftRight) { dir -= up;    }
        if dir.length_squared() > 0.0 {
            // ダッシュ（Ctrl）で速度倍化
            let mult = if keys.pressed(KeyCode::ControlLeft)
                       || keys.pressed(KeyCode::ControlRight) { 4.0 } else { 1.0 };
            tf.translation += dir.normalize() * cam.speed * mult * time.delta_secs();
        }
    }
}

/// マウスホイールで速度調整（capture 中のみ）
fn wheel_speed_system(
    mut wheel: EventReader<MouseWheel>,
    mut q: Query<&mut FlyCam>,
) {
    let dy: f32 = wheel.read().map(|e| e.y).sum();
    if dy.abs() < f32::EPSILON { return; }
    for mut cam in q.iter_mut() {
        if !cam.captured { continue; }
        let factor = 1.15_f32.powf(dy);
        cam.speed = (cam.speed * factor).clamp(1.0, 4000.0);
    }
}
