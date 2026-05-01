//! Greedy meshing — voxel グリッド → 面が大きく統合された四角形群。
//!
//! 設計参考：schematic-renderer/mesh_builder_wasm/src/lib.rs
//! ただし WASM bindings 抜き、テクスチャ無し（マテリアル別の単色）に専用化。
//!
//! 出力：マテリアル別の `MeshBuffer { positions, normals, indices }`。
//!  - positions: f32 × 3 × vert_count
//!  - normals  : f32 × 3 × vert_count
//!  - indices  : u32（>65535 vertex を扱うため u32 で統一）
//!
//! 高速化：
//!  - 充填ボクセルの bbox 内に走査範囲を限定（slice ごとの空走を避ける）
//!  - 6 面方向 × slice を rayon で並列化

use std::collections::HashMap;
use std::ops::Range;

use rayon::prelude::*;

use crate::voxel::{Material, VoxelGrid};

#[derive(Default, Clone)]
pub struct MeshBuffer {
    pub positions: Vec<[f32; 3]>,
    pub normals:   Vec<[f32; 3]>,
    pub indices:   Vec<u32>,
}

impl MeshBuffer {
    fn push_quad(&mut self, v: [[f32; 3]; 4], n: [f32; 3]) {
        let base = self.positions.len() as u32;
        for p in &v { self.positions.push(*p); }
        for _ in 0..4 { self.normals.push(n); }
        // 三角形分割：(0,1,2), (0,2,3)
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn append_with_offset(&mut self, other: &mut MeshBuffer) {
        let base = self.positions.len() as u32;
        self.positions.append(&mut other.positions);
        self.normals.append(&mut other.normals);
        for idx in &mut other.indices { *idx += base; }
        self.indices.append(&mut other.indices);
    }
}

#[derive(Clone, Copy)]
struct FaceAxis {
    normal: [f32; 3],
    /// (axis, dir): axis ∈ {0,1,2}, dir ∈ {+1, -1}
    axis: usize,
    dir: i32,
    /// 面が乗る平面の u, v 軸（位置インデックス）
    u_axis: usize,
    v_axis: usize,
}

const FACES: [FaceAxis; 6] = [
    FaceAxis { normal: [ 1.0, 0.0, 0.0], axis: 0, dir:  1, u_axis: 2, v_axis: 1 }, // +X
    FaceAxis { normal: [-1.0, 0.0, 0.0], axis: 0, dir: -1, u_axis: 2, v_axis: 1 }, // -X
    FaceAxis { normal: [ 0.0, 1.0, 0.0], axis: 1, dir:  1, u_axis: 0, v_axis: 2 }, // +Y
    FaceAxis { normal: [ 0.0,-1.0, 0.0], axis: 1, dir: -1, u_axis: 0, v_axis: 2 }, // -Y
    FaceAxis { normal: [ 0.0, 0.0, 1.0], axis: 2, dir:  1, u_axis: 0, v_axis: 1 }, // +Z
    FaceAxis { normal: [ 0.0, 0.0,-1.0], axis: 2, dir: -1, u_axis: 0, v_axis: 1 }, // -Z
];

/// VoxelGrid から、(material, MeshBuffer) のペア配列を返す。
pub fn build_meshes(grid: &VoxelGrid) -> Vec<(Material, MeshBuffer)> {
    let bbox = match grid.filled_bbox() {
        Some(b) => b,
        None    => return Material::all_visible().iter()
                            .map(|m| (*m, MeshBuffer::default())).collect(),
    };

    // 各面方向 × 各 slice を並列処理 → 面方向ごとの slice 結果を集約
    let mut out: Vec<(Material, MeshBuffer)> = Material::all_visible()
        .iter().map(|m| (*m, MeshBuffer::default())).collect();
    let buf_index: HashMap<Material, usize> = out.iter()
        .enumerate().map(|(i, (m, _))| (*m, i)).collect();

    for face in FACES.iter() {
        let slice_results: Vec<HashMap<Material, MeshBuffer>> = bbox[face.axis].clone()
            .into_par_iter()
            .map(|slice| process_one_slice(grid, face, slice, &bbox))
            .collect();

        for slice_map in slice_results {
            for (mat, mut buf) in slice_map {
                let idx = *buf_index.get(&mat).unwrap_or(&0);
                out[idx].1.append_with_offset(&mut buf);
            }
        }
    }
    out
}

fn process_one_slice(
    grid: &VoxelGrid,
    face: &FaceAxis,
    slice: i32,
    bbox: &[Range<i32>; 3],
) -> HashMap<Material, MeshBuffer> {
    let n_u = grid.size[face.u_axis];
    let n_v = grid.size[face.v_axis];
    let u_range = bbox[face.u_axis].clone();
    let v_range = bbox[face.v_axis].clone();

    // mask[v * n_u + u]: その面に出すマテリアル（None = 不要）
    let mut mask: Vec<Option<Material>> = vec![None; n_u * n_v];

    for v in v_range.clone() {
        for u in u_range.clone() {
            let pos = compose(face, slice, u as usize, v as usize);
            let m_self = grid.get(pos[0], pos[1], pos[2]);
            if !m_self.is_solid() { continue; }
            let neighbor_pos = [
                pos[0] + if face.axis == 0 { face.dir } else { 0 },
                pos[1] + if face.axis == 1 { face.dir } else { 0 },
                pos[2] + if face.axis == 2 { face.dir } else { 0 },
            ];
            let m_neigh = grid.get(neighbor_pos[0], neighbor_pos[1], neighbor_pos[2]);
            let need_face = if m_self.is_translucent() {
                !m_neigh.is_solid()
            } else {
                !m_neigh.is_solid() || m_neigh.is_translucent()
            };
            if need_face {
                mask[(v as usize) * n_u + (u as usize)] = Some(m_self);
            }
        }
    }

    // 2D greedy
    let mut used = vec![false; n_u * n_v];
    let mut local: HashMap<Material, MeshBuffer> = HashMap::new();

    for v0 in v_range.clone() {
        for u0 in u_range.clone() {
            let idx0 = (v0 as usize) * n_u + (u0 as usize);
            if used[idx0] { continue; }
            let m = match mask[idx0] { Some(m) => m, None => continue };

            let mut w = 1i32;
            while u0 + w < u_range.end {
                let idx = (v0 as usize) * n_u + ((u0 + w) as usize);
                if used[idx] || mask[idx] != Some(m) { break; }
                w += 1;
            }

            let mut h = 1i32;
            'rows: while v0 + h < v_range.end {
                for du in 0..w {
                    let idx = ((v0 + h) as usize) * n_u + ((u0 + du) as usize);
                    if used[idx] || mask[idx] != Some(m) { break 'rows; }
                }
                h += 1;
            }

            for dv in 0..h {
                for du in 0..w {
                    used[((v0 + dv) as usize) * n_u + ((u0 + du) as usize)] = true;
                }
            }

            let buf = local.entry(m).or_default();
            emit_quad(face, slice, u0, v0, w, h, buf);
        }
    }
    local
}

fn compose(face: &FaceAxis, slice: i32, u: usize, v: usize) -> [i32; 3] {
    let mut p = [0i32; 3];
    p[face.axis]   = slice;
    p[face.u_axis] = u as i32;
    p[face.v_axis] = v as i32;
    p
}

fn emit_quad(
    face: &FaceAxis,
    slice: i32, u0: i32, v0: i32, w: i32, h: i32,
    buf: &mut MeshBuffer,
) {
    let plane_coord: f32 = if face.dir > 0 { (slice + 1) as f32 } else { slice as f32 };
    let make_vert = |u: i32, v: i32| -> [f32; 3] {
        let mut p = [0.0f32; 3];
        p[face.axis]   = plane_coord;
        p[face.u_axis] = u as f32;
        p[face.v_axis] = v as f32;
        p
    };
    let p0 = make_vert(u0,        v0);
    let p1 = make_vert(u0 + w,    v0);
    let p2 = make_vert(u0 + w,    v0 + h);
    let p3 = make_vert(u0,        v0 + h);
    let quad = if face.dir > 0 { [p0, p1, p2, p3] } else { [p0, p3, p2, p1] };
    buf.push_quad(quad, face.normal);
}
