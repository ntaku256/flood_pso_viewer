//! VoxelGrid → Bevy meshes をスポーン。
//!
//! マテリアル別に `MeshBuffer` を Bevy `Mesh` に変換し、`StandardMaterial` で表示。
//! 透過マテリアル（水・氷）は `AlphaMode::Blend` を使用。

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

use crate::greedy_mesh::{build_meshes, MeshBuffer};
use crate::material::VoxelMaterial;
use crate::voxel::VoxelGrid;

#[derive(Component)]
pub struct VoxelWorldRoot;

pub fn spawn_voxel_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<VoxelMaterial>,
    grid: &VoxelGrid,
) {
    let groups = build_meshes(grid);
    let mut total_verts = 0usize;
    let mut total_quads = 0usize;

    let root = commands.spawn((
        VoxelWorldRoot,
        Transform::default(),
        Visibility::default(),
    )).id();

    for (mat, buf) in groups {
        if buf.positions.is_empty() { continue; }
        total_verts += buf.positions.len();
        total_quads += buf.indices.len() / 6;

        let bevy_mesh = build_bevy_mesh(&buf);
        let alpha_mode = if mat.is_translucent() {
            AlphaMode::Blend
        } else { AlphaMode::Opaque };

        let v_mat = VoxelMaterial {
            color: mat.color().to_linear(),
            alpha_mode,
            double_sided: mat.is_translucent(),
        };

        let child = commands.spawn((
            Mesh3d(meshes.add(bevy_mesh)),
            MeshMaterial3d(materials.add(v_mat)),
            Transform::default(),
        )).id();
        commands.entity(root).add_child(child);
    }

    info!("Spawned voxel world: {} verts, {} quads", total_verts, total_quads);
}

fn build_bevy_mesh(buf: &MeshBuffer) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buf.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,   buf.normals.clone());
    // UVs はダミー（テクスチャ未使用）
    let uvs = vec![[0.0f32, 0.0f32]; buf.positions.len()];
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(buf.indices.clone()));
    mesh
}

