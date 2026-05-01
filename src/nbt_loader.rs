//! flood_pso が吐く Minecraft Structure NBT (.nbt, gzip) を読んで
//!  - VoxelGrid を densify
//!  - flood_pso_meta コンパウンドを取り出す
//!
//! Structure NBT のフォーマットは公式仕様準拠：
//!   { DataVersion, size:[x,y,z], palette:[{Name,Properties?}, ...],
//!     blocks:[{pos:[x,y,z], state:i32, ...}, ...], entities:[..] }
//!
//! flood_pso_meta は本リポジトリ独自のコンパウンドで、
//!  schema_version / method / K / D / loss / iou / dh_map 等を含む。

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::voxel::{Material, VoxelGrid};

#[derive(Debug, Deserialize)]
struct PaletteEntry {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct BlockEntry {
    pos: Vec<i32>,
    state: i32,
}

#[derive(Debug, Deserialize)]
struct StructureRoot {
    #[serde(default)]
    #[allow(dead_code)]
    #[serde(rename = "DataVersion")]
    data_version: Option<i32>,
    size: Vec<i32>,
    palette: Vec<PaletteEntry>,
    blocks: Vec<BlockEntry>,
    /// 我々が埋め込んだメタデータ。fastnbt の Value で保持して任意型に対応。
    #[serde(default, rename = "flood_pso_meta")]
    flood_pso_meta: Option<fastnbt::Value>,
}

/// NBT 内の flood_pso_meta を扱いやすい構造に整理した結果。
#[derive(Debug, Clone, Default)]
pub struct FloodPsoMeta {
    pub method:        Option<String>,
    pub method_long:   Option<String>,
    pub experiment:    Option<String>,
    pub k:             Option<i32>,
    pub d:             Option<i32>,
    pub seed:          Option<i32>,
    pub loss:          Option<f64>,
    pub iou:           Option<f64>,
    pub dh_rmse:       Option<f64>,
    pub water_level:   Option<f64>,
    pub sigma:         Option<f64>,
    pub n_evals:       Option<i32>,
    pub elapsed_s:     Option<f64>,
    pub preset:        Option<String>,
    pub dem_source:    Option<String>,
    pub study_area:    Option<String>,
    pub git_revision:  Option<String>,
    pub timestamp_utc: Option<String>,
    pub dh_map:        Option<Vec<f32>>,
    pub dh_map_shape:  Option<Vec<i32>>,
    /// その他全フィールドを保持（UI 側で展開可能）
    pub raw:           BTreeMap<String, String>,
}

pub struct LoadedNbt {
    pub grid: VoxelGrid,
    pub meta: FloodPsoMeta,
    pub size_xyz: [i32; 3],
    pub n_block_entries: usize,
}

/// `.nbt` (gzip) を読んで VoxelGrid + flood_pso_meta を返す。
pub fn load_structure_nbt<P: AsRef<Path>>(path: P) -> Result<LoadedNbt> {
    let path_ref = path.as_ref();
    let f = File::open(path_ref)
        .with_context(|| format!("opening {}", path_ref.display()))?;
    let mut buf = Vec::new();
    GzDecoder::new(f).read_to_end(&mut buf)
        .with_context(|| format!("gunzip {}", path_ref.display()))?;

    let root: StructureRoot = fastnbt::from_bytes(&buf)
        .with_context(|| format!("parse NBT {}", path_ref.display()))?;

    if root.size.len() != 3 {
        return Err(anyhow!("size must be 3 elements, got {}", root.size.len()));
    }
    let size_xyz = [root.size[0], root.size[1], root.size[2]];
    let nx = size_xyz[0].max(0) as usize;
    let ny = size_xyz[1].max(0) as usize;
    let nz = size_xyz[2].max(0) as usize;

    // palette index → Material
    let palette_mat: Vec<Material> = root.palette.iter()
        .map(|e| Material::from_minecraft_name(&e.name))
        .collect();

    let mut grid = VoxelGrid::new([nx, ny, nz]);
    let n_blocks = root.blocks.len();
    for b in &root.blocks {
        if b.pos.len() != 3 { continue; }
        let (x, y, z) = (b.pos[0], b.pos[1], b.pos[2]);
        if x < 0 || y < 0 || z < 0 { continue; }
        let s = b.state.max(0) as usize;
        let mat = palette_mat.get(s).copied().unwrap_or(Material::Other);
        if mat == Material::Air { continue; }
        grid.set(x as usize, y as usize, z as usize, mat);
    }

    let meta = root.flood_pso_meta.as_ref()
        .map(decode_flood_pso_meta)
        .unwrap_or_default();

    Ok(LoadedNbt {
        grid,
        meta,
        size_xyz,
        n_block_entries: n_blocks,
    })
}

fn decode_flood_pso_meta(v: &fastnbt::Value) -> FloodPsoMeta {
    let mut out = FloodPsoMeta::default();
    if let fastnbt::Value::Compound(map) = v {
        for (key, val) in map {
            // 主要フィールド抽出
            match key.as_str() {
                "method"        => out.method      = as_string(val),
                "method_long"   => out.method_long = as_string(val),
                "experiment"    => out.experiment  = as_string(val),
                "K"             => out.k           = as_i32(val),
                "D"             => out.d           = as_i32(val),
                "seed"          => out.seed        = as_i32(val),
                "loss"          => out.loss        = as_f64(val),
                "iou"           => out.iou         = as_f64(val),
                "dh_rmse"       => out.dh_rmse     = as_f64(val),
                "water_level_global_m" => out.water_level = as_f64(val),
                "sigma"         => out.sigma       = as_f64(val),
                "n_evals"       => out.n_evals     = as_i32(val),
                "elapsed_s"     => out.elapsed_s   = as_f64(val),
                "preset"        => out.preset      = as_string(val),
                "dem_source"    => out.dem_source  = as_string(val),
                "study_area"    => out.study_area  = as_string(val),
                "git_revision"  => out.git_revision = as_string(val),
                "timestamp_utc" => out.timestamp_utc = as_string(val),
                "dh_map"        => out.dh_map      = as_f32_list(val),
                "dh_map_shape"  => out.dh_map_shape = as_i32_list(val),
                _ => {}
            }
            // 簡易表示用の文字列形にして raw にも保存
            out.raw.insert(key.clone(), short_repr(val));
        }
    }
    out
}

fn as_string(v: &fastnbt::Value) -> Option<String> {
    if let fastnbt::Value::String(s) = v { Some(s.clone()) } else { None }
}
fn as_i32(v: &fastnbt::Value) -> Option<i32> {
    match v {
        fastnbt::Value::Byte(b)  => Some(*b as i32),
        fastnbt::Value::Short(s) => Some(*s as i32),
        fastnbt::Value::Int(i)   => Some(*i),
        fastnbt::Value::Long(l)  => Some(*l as i32),
        _ => None,
    }
}
fn as_f64(v: &fastnbt::Value) -> Option<f64> {
    match v {
        fastnbt::Value::Float(f)  => Some(*f as f64),
        fastnbt::Value::Double(d) => Some(*d),
        fastnbt::Value::Int(i)    => Some(*i as f64),
        fastnbt::Value::Long(l)   => Some(*l as f64),
        _ => None,
    }
}
fn as_f32_list(v: &fastnbt::Value) -> Option<Vec<f32>> {
    if let fastnbt::Value::List(list) = v {
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            match item {
                fastnbt::Value::Float(f)  => out.push(*f),
                fastnbt::Value::Double(d) => out.push(*d as f32),
                _ => return None,
            }
        }
        Some(out)
    } else { None }
}
fn as_i32_list(v: &fastnbt::Value) -> Option<Vec<i32>> {
    if let fastnbt::Value::List(list) = v {
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            if let Some(i) = as_i32(item) { out.push(i); } else { return None; }
        }
        Some(out)
    } else { None }
}

fn short_repr(v: &fastnbt::Value) -> String {
    use fastnbt::Value::*;
    match v {
        Byte(b)   => format!("{b}b"),
        Short(s)  => format!("{s}s"),
        Int(i)    => i.to_string(),
        Long(l)   => format!("{l}L"),
        Float(f)  => format!("{f:.4}f"),
        Double(d) => format!("{d:.4}"),
        String(s) => s.clone(),
        List(l)   => format!("[..{} items]", l.len()),
        Compound(m) => format!("{{..{} keys}}", m.len()),
        ByteArray(a)  => format!("byte[{}]", a.len()),
        IntArray(a)   => format!("int[{}]", a.len()),
        LongArray(a)  => format!("long[{}]", a.len()),
    }
}
