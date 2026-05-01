//! VoxelGrid → Bevy meshes をスポーン。
//!
//! マテリアル別に `MeshBuffer` を Bevy `Mesh` に変換し、`StandardMaterial` で表示。
//! 透過マテリアル（水・氷）は `AlphaMode::Blend` を使用。

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

use crate::greedy_mesh::{build_meshes_chunked, MeshBuffer};
use crate::material::VoxelMaterial;
use crate::voxel::{Material as VxMat, VoxelGrid};

#[derive(Component)]
pub struct VoxelWorldRoot;

/// 水・氷 entity に付けるマーカー（V キーで Visibility をトグル）
#[derive(Component)]
pub struct WaterLayer;

/// 1チャンクあたりのXZブロック数。Bevyのfrustum cullingがチャンク単位で効く。
pub const CHUNK_XZ: usize = 128;

pub fn spawn_voxel_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<VoxelMaterial>,
    grid: &VoxelGrid,
) {
    let chunks = build_meshes_chunked(grid, CHUNK_XZ);
    let n_chunks = chunks.len();
    let mut total_verts = 0usize;
    let mut total_quads = 0usize;
    let mut n_entities = 0usize;

    let root = commands.spawn((
        VoxelWorldRoot,
        Transform::default(),
        Visibility::default(),
    )).id();

    // material → cached VoxelMaterial handle（同じ色を何度も addしないため）
    let mut mat_handles: std::collections::HashMap<u8, Handle<VoxelMaterial>> = Default::default();

    for chunk in chunks {
        for (mat, buf) in chunk.meshes {
            if buf.positions.is_empty() { continue; }
            total_verts += buf.positions.len();
            total_quads += buf.indices.len() / 6;

            let key = mat as u8;
            let mat_handle = mat_handles.entry(key).or_insert_with(|| {
                let alpha_mode = if mat.is_translucent() {
                    AlphaMode::Blend
                } else { AlphaMode::Opaque };
                materials.add(VoxelMaterial {
                    color: mat.color().to_linear(),
                    alpha_mode,
                    double_sided: mat.is_translucent(),
                })
            }).clone();

            let bevy_mesh = build_bevy_mesh(&buf);
            let mut ent = commands.spawn((
                Mesh3d(meshes.add(bevy_mesh)),
                MeshMaterial3d(mat_handle),
                Transform::default(),
            ));
            if matches!(mat, VxMat::Water | VxMat::Ice) {
                ent.insert(WaterLayer);
            }
            let child = ent.id();
            commands.entity(root).add_child(child);
            n_entities += 1;
        }
    }

    info!(
        "Spawned voxel world: {} chunks, {} entities, {} verts, {} quads",
        n_chunks, n_entities, total_verts, total_quads
    );
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

