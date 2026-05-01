//! ボクセルグリッドとパレット定義
//!
//! flood_pso が出力する NBT は Minecraft Structure 形式：
//! palette は `[{Name: "minecraft:stone"}, ...]` のリスト、
//! blocks は `[{pos:[x,y,z], state:i32}, ...]` のスパースなリスト。
//!
//! ここでは densify して `Vec<u8>` 3D グリッドに直し、greedy mesher が扱える形にする。

use bevy::color::Color;

/// 各マテリアル（簡略化したパレット ID）。
/// `air = 0` を空白扱いにするため、その他は 1 から開始。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Material {
    Air = 0,
    Stone = 1,
    Grass = 2,
    Sand = 3,
    Gravel = 4,
    Water = 5,        // blue_stained_glass
    Ice = 6,          // cyan_stained_glass
    Bedrock = 7,
    Other = 255,
}

impl Material {
    pub fn from_minecraft_name(name: &str) -> Self {
        match name {
            "minecraft:air"                  => Material::Air,
            "minecraft:stone"                => Material::Stone,
            "minecraft:grass_block"          => Material::Grass,
            "minecraft:sand"                 => Material::Sand,
            "minecraft:gravel"               => Material::Gravel,
            "minecraft:blue_stained_glass"   => Material::Water,
            "minecraft:cyan_stained_glass"   => Material::Ice,
            "minecraft:bedrock"              => Material::Bedrock,
            _                                => Material::Other,
        }
    }

    pub fn color(&self) -> Color {
        // 半透明の水だけ alpha < 1 にする
        match self {
            Material::Air     => Color::NONE,
            Material::Stone   => Color::srgb(0.55, 0.55, 0.58),
            Material::Grass   => Color::srgb(0.34, 0.62, 0.30),
            Material::Sand    => Color::srgb(0.85, 0.78, 0.55),
            Material::Gravel  => Color::srgb(0.55, 0.50, 0.45),
            Material::Water   => Color::srgba(0.20, 0.45, 0.85, 0.62),
            Material::Ice     => Color::srgba(0.55, 0.85, 0.95, 0.55),
            Material::Bedrock => Color::srgb(0.10, 0.10, 0.10),
            Material::Other   => Color::srgb(1.0, 0.0, 1.0),
        }
    }

    pub fn is_translucent(&self) -> bool {
        matches!(self, Material::Water | Material::Ice)
    }

    pub fn is_solid(&self) -> bool {
        !matches!(self, Material::Air)
    }

    pub fn all_visible() -> &'static [Material] {
        &[
            Material::Stone,
            Material::Grass,
            Material::Sand,
            Material::Gravel,
            Material::Water,
            Material::Ice,
            Material::Bedrock,
            Material::Other,
        ]
    }
}

/// 直方体ボクセルグリッド。境界外アクセスは Air として扱う。
pub struct VoxelGrid {
    pub size: [usize; 3], // [nx, ny, nz]
    cells: Vec<Material>, // index = x + y*nx + z*nx*ny
    /// 充填ボクセルの bbox。set 中に追跡する。空のときは None。
    bbox_min: Option<[i32; 3]>,
    bbox_max: Option<[i32; 3]>,
}

impl VoxelGrid {
    pub fn new(size: [usize; 3]) -> Self {
        let n = size[0] * size[1] * size[2];
        Self {
            size,
            cells: vec![Material::Air; n],
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
        if x < 0 || y < 0 || z < 0 { return Material::Air; }
        let (xu, yu, zu) = (x as usize, y as usize, z as usize);
        if xu >= self.size[0] || yu >= self.size[1] || zu >= self.size[2] {
            return Material::Air;
        }
        self.cells[self.index(xu, yu, zu)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, m: Material) {
        if x < self.size[0] && y < self.size[1] && z < self.size[2] {
            let idx = self.index(x, y, z);
            self.cells[idx] = m;
            if m != Material::Air {
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
        self.cells.iter().filter(|m| **m != Material::Air).count()
    }

    /// 充填ボクセルが存在する範囲（[xmin..xmax+1) 等）を返す。
    /// 空の場合は None。set() 中に追跡しているので O(1)。
    pub fn filled_bbox(&self) -> Option<[std::ops::Range<i32>; 3]> {
        let mn = self.bbox_min?;
        let mx = self.bbox_max?;
        Some([mn[0]..mx[0] + 1, mn[1]..mx[1] + 1, mn[2]..mx[2] + 1])
    }
}
