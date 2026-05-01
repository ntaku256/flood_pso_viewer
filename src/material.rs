//! 専用 VoxelMaterial — Bevy の PBR シェーダを使わずに lambert + ambient だけで描画する。
//!
//! WSL2 の Mesa GL は `GL_EXT_texture_shadow_lod` を持たず、
//! Bevy 0.15 標準の StandardMaterial フラグメントシェーダがコンパイル失敗する。
//! 影もテクスチャも要らない用途なので、PBR を完全にバイパスする最小シェーダに置き換える。

use bevy::asset::{Asset, Handle};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};

pub const VOXEL_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(0xF1_00D_50_001A);

const VOXEL_WGSL: &str = r#"
#import bevy_pbr::{
    mesh_functions::{get_world_from_local, mesh_position_local_to_clip, mesh_position_local_to_world, mesh_normal_local_to_world},
    view_transformations::position_world_to_clip,
}

struct VoxelMat {
    color: vec4<f32>,
};

@group(2) @binding(0) var<uniform> material: VoxelMat;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
};

struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal_ws: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    let world = get_world_from_local(v.instance_index);
    let world_pos = mesh_position_local_to_world(world, vec4<f32>(v.position, 1.0));
    var out: VOut;
    out.position = position_world_to_clip(world_pos.xyz);
    out.normal_ws = normalize(mesh_normal_local_to_world(v.normal, v.instance_index));
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal_ws);
    // 太陽方向（main.rs の DirectionalLight と概ね合わせる）
    let sun = normalize(vec3<f32>(0.4, 0.9, 0.25));
    let lambert = max(dot(n, sun), 0.0);
    let ambient = 0.45;
    let intensity = ambient + lambert * 0.55;
    let rgb = material.color.rgb * intensity;
    return vec4<f32>(rgb, material.color.a);
}
"#;

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct VoxelMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
}

impl Material for VoxelMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(VOXEL_SHADER_HANDLE)
    }
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(VOXEL_SHADER_HANDLE)
    }
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None; // 半透明面の裏も見せる
        Ok(())
    }
}

pub struct VoxelMaterialPlugin;

impl Plugin for VoxelMaterialPlugin {
    fn build(&self, app: &mut App) {
        // シェーダを weak handle と紐付けて Assets<Shader> に登録
        let shader = Shader::from_wgsl(VOXEL_WGSL, file!());
        app.world_mut()
            .resource_mut::<Assets<Shader>>()
            .insert(&VOXEL_SHADER_HANDLE, shader);

        app.add_plugins(MaterialPlugin::<VoxelMaterial>::default());
    }
}
