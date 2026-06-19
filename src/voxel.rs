//! ボクセルグリッドとパレット定義
//!
//! flood_pso が出力する NBT は Minecraft Structure 形式：
//! palette は `[{Name: "minecraft:stone"}, ...]` のリスト、
//! blocks は `[{pos:[x,y,z], state:i32}, ...]` のスパースなリスト。
//!
//! ここでは densify して `Vec<u8>` 3D グリッドに直し、greedy mesher が扱える形にする。

use bevy::color::Color;

use crate::block_colors::block_rgb_role;

/// マテリアル = NBT palette の index。`air` は常に 0（flood_pso 出力規約）。
/// 色・透過は per-load の [`Palette`] が解決する（任意のバニラブロックに対応）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Material(pub u16);

impl Material {
    pub const AIR: Material = Material(0);

    #[inline]
    pub fn is_solid(&self) -> bool {
        *self != Material::AIR
    }
}

/// ロード毎のブロック色テーブル（NBT palette 名を block_colors.rs で解決）。
#[derive(Clone, Default)]
pub struct Palette {
    pub colors: Vec<[f32; 4]>,   // 各 index の srgba（0..1）
    pub translucent: Vec<bool>,  // 半透明（水/氷）か
    pub water_like: Vec<bool>,   // 水/氷レイヤか（V キー・WaterLayer 用）
    pub air_index: u16,
}

impl Palette {
    /// NBT palette のブロック名リストから色テーブルを構築。未知ブロックはマゼンタ。
    pub fn from_names(names: &[String]) -> Self {
        let mut colors = Vec::with_capacity(names.len());
        let mut translucent = Vec::with_capacity(names.len());
        let mut water_like = Vec::with_capacity(names.len());
        let mut air_index = 0u16;
        for (i, n) in names.iter().enumerate() {
            if n == "minecraft:air" {
                air_index = i as u16;
                colors.push([0.0, 0.0, 0.0, 0.0]);
                translucent.push(false);
                water_like.push(false);
                continue;
            }
            match block_rgb_role(n) {
                Some(([r, g, b], role)) => {
                    let a = if role == 0 { 1.0 } else { 0.6 };
                    colors.push([r, g, b, a]);
                    translucent.push(role != 0);
                    water_like.push(role != 0);
                }
                None => {
                    colors.push([1.0, 0.0, 1.0, 1.0]); // 未知＝マゼンタ
                    translucent.push(false);
                    water_like.push(false);
                }
            }
        }
        Self { colors, translucent, water_like, air_index }
    }

    #[inline]
    pub fn len(&self) -> usize { self.colors.len() }

    #[inline]
    pub fn color(&self, m: Material) -> Color {
        let c = self.colors.get(m.0 as usize).copied().unwrap_or([1.0, 0.0, 1.0, 1.0]);
        Color::srgba(c[0], c[1], c[2], c[3])
    }

    #[inline]
    pub fn is_translucent(&self, m: Material) -> bool {
        self.translucent.get(m.0 as usize).copied().unwrap_or(false)
    }

    #[inline]
    pub fn is_water_like(&self, m: Material) -> bool {
        self.water_like.get(m.0 as usize).copied().unwrap_or(false)
    }
}

/// 直方体ボクセルグリッド。境界外アクセスは Air として扱う。
pub struct VoxelGrid {
    pub size: [usize; 3], // [nx, ny, nz]
    pub palette: Palette, // index → 色/透過（任意のバニラブロック対応）
    cells: Vec<Material>, // index = x + y*nx + z*nx*ny
    /// 充填ボクセルの bbox。set 中に追跡する。空のときは None。
    bbox_min: Option<[i32; 3]>,
    bbox_max: Option<[i32; 3]>,
}

impl VoxelGrid {
    pub fn new(size: [usize; 3], palette: Palette) -> Self {
        let n = size[0] * size[1] * size[2];
        Self {
            size,
            palette,
            cells: vec![Material::AIR; n],
            bbox_min: None,
            bbox_max: None,
        }
    }

    #[inline]
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.size[0] + z * self.size[0] * self.size[1]
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> Material {
        if x < 0 || y < 0 || z < 0 { return Material::AIR; }
        let (xu, yu, zu) = (x as usize, y as usize, z as usize);
        if xu >= self.size[0] || yu >= self.size[1] || zu >= self.size[2] {
            return Material::AIR;
        }
        self.cells[self.index(xu, yu, zu)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, m: Material) {
        if x < self.size[0] && y < self.size[1] && z < self.size[2] {
            let idx = self.index(x, y, z);
            self.cells[idx] = m;
            if m != Material::AIR {
                let p = [x as i32, y as i32, z as i32];
                match (&mut self.bbox_min, &mut self.bbox_max) {
                    (Some(mn), Some(mx)) => {
                        for k in 0..3 {
                            if p[k] < mn[k] { mn[k] = p[k]; }
                            if p[k] > mx[k] { mx[k] = p[k]; }
                        }
                    }
                    _ => { self.bbox_min = Some(p); self.bbox_max = Some(p); }
                }
            }
        }
    }

    pub fn count_non_air(&self) -> usize {
        self.cells.iter().filter(|m| **m != Material::AIR).count()
    }

    /// 水・氷を Air に置き換え、bbox を再計算する（--no-water 用）
    pub fn strip_water(&mut self) {
        let water = &self.palette.water_like;
        for c in self.cells.iter_mut() {
            if water.get(c.0 as usize).copied().unwrap_or(false) {
                *c = Material::AIR;
            }
        }
        // bbox 再計算
        self.bbox_min = None;
        self.bbox_max = None;
        let [nx, ny, nz] = self.size;
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let idx = self.index(x, y, z);
                    if self.cells[idx] != Material::AIR {
                        let p = [x as i32, y as i32, z as i32];
                        match (&mut self.bbox_min, &mut self.bbox_max) {
                            (Some(mn), Some(mx)) => {
                                for k in 0..3 {
                                    if p[k] < mn[k] { mn[k] = p[k]; }
                                    if p[k] > mx[k] { mx[k] = p[k]; }
                                }
                            }
                            _ => { self.bbox_min = Some(p); self.bbox_max = Some(p); }
                        }
                    }
                }
            }
        }
    }

    /// 充填ボクセルが存在する範囲（[xmin..xmax+1) 等）を返す。
    /// 空の場合は None。set() 中に追跡しているので O(1)。
    pub fn filled_bbox(&self) -> Option<[std::ops::Range<i32>; 3]> {
        let mn = self.bbox_min?;
        let mx = self.bbox_max?;
        Some([mn[0]..mx[0] + 1, mn[1]..mx[1] + 1, mn[2]..mx[2] + 1])
    }
}
