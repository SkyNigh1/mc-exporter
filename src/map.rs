//! Top-down map rendering, one 512×512 image per region.

use crate::registry::{Fluid, Registry, Tint};
use crate::world::Source;
use std::collections::HashMap;
use std::path::Path;

pub struct RegionMap {
    pub rx: i32,
    pub rz: i32,
    pub rgba: Vec<u8>,
    /// visible block state (0 = nothing)
    pub top: Vec<u32>,
    pub height: Vec<i16>,
}

struct ColorCache<'a> {
    reg: &'a Registry,
    cache: HashMap<u32, Option<([f32; 3], Tint, bool)>>,
}

impl ColorCache<'_> {
    fn get(&mut self, id: u32) -> Option<([f32; 3], Tint, bool)> {
        if let Some(c) = self.cache.get(&id) {
            return *c;
        }
        let g = self.reg.geom(id);
        let r = if g.air {
            None
        } else {
            g.map_tex.and_then(|t| {
                let tex = self.reg.assets.texture(&self.reg.tex_name(t))?;
                if tex.avg[3] < 0.05 {
                    return None;
                }
                let water = matches!(g.fluid, Fluid::Water(_));
                Some(([tex.avg[0] / 255.0, tex.avg[1] / 255.0, tex.avg[2] / 255.0], g.map_tint, water))
            })
        };
        let r = if g.unknown { Some(([1.0, 0.0, 0.85], Tint::None, false)) } else { r };
        self.cache.insert(id, r);
        r
    }
}

pub fn render_region(reg: &Registry, source: &Source, rx: i32, rz: i32, floor: i32, ceil: i32) -> RegionMap {
    let chunks = source.region_chunks(rx, rz, reg);
    let mut rgba = vec![0u8; 512 * 512 * 4];
    let mut top = vec![0u32; 512 * 512];
    let mut height = vec![i16::MIN; 512 * 512];
    let mut cc = ColorCache { reg, cache: HashMap::new() };
    let mut col = vec![[0f32; 3]; 512 * 512];
    for c in &chunks {
        let bx = (c.cx - rx * 32) as usize * 16;
        let bz = (c.cz - rz * 32) as usize * 16;
        let ymax = c.max_y().min(ceil);
        for lz in 0..16 {
            for lx in 0..16 {
                let idx = (bz + lz) * 512 + bx + lx;
                let mut y = ymax;
                let mut water: Option<([f32; 3], i32)> = None;
                while y >= floor {
                    let id = c.block(lx, y, lz);
                    if id != 0 {
                        if let Some((rgb, tint, is_water)) = cc.get(id) {
                            let b = reg.biome_colors(c.biome(lx, y, lz));
                            let t = tint.color(&b).map(|t| [t[0] as f32 / 255.0, t[1] as f32 / 255.0, t[2] as f32 / 255.0]);
                            let rgb = match t {
                                Some(t) => [rgb[0] * t[0], rgb[1] * t[1], rgb[2] * t[2]],
                                None => rgb,
                            };
                            if is_water {
                                if water.is_none() {
                                    water = Some((rgb, y));
                                    top[idx] = id;
                                    height[idx] = y as i16;
                                }
                                if y < water.unwrap().1 - 14 {
                                    break;
                                }
                            } else {
                                let final_rgb = match water {
                                    Some((w, wy)) => {
                                        let depth = (wy - y) as f32;
                                        let k = (0.55 + depth * 0.04).min(0.92);
                                        [w[0] * k + rgb[0] * (1.0 - k), w[1] * k + rgb[1] * (1.0 - k), w[2] * k + rgb[2] * (1.0 - k)]
                                    }
                                    None => {
                                        top[idx] = id;
                                        height[idx] = y as i16;
                                        rgb
                                    }
                                };
                                col[idx] = final_rgb;
                                rgba[idx * 4 + 3] = 255;
                                water = None;
                                break;
                            }
                        }
                    }
                    y -= 1;
                }
                if let Some((w, _)) = water {
                    col[idx] = w;
                    rgba[idx * 4 + 3] = 255;
                }
                if rgba[idx * 4 + 3] == 0 && !c.sections.is_empty() {
                    // chunk present but empty in the slice: dark background
                    col[idx] = [0.08, 0.08, 0.09];
                    rgba[idx * 4 + 3] = 255;
                }
            }
        }
    }
    // relief: shading relative to the northern neighbor
    for z in 0..512 {
        for x in 0..512 {
            let i = z * 512 + x;
            if rgba[i * 4 + 3] == 0 {
                continue;
            }
            let h = height[i];
            let mut f = 1.0;
            if z > 0 && h != i16::MIN {
                let hn = height[i - 512];
                if hn != i16::MIN {
                    if h > hn {
                        f = 1.12;
                    } else if h < hn {
                        f = 0.82;
                    }
                }
            }
            let c = col[i];
            rgba[i * 4] = (c[0] * f * 255.0).clamp(0.0, 255.0) as u8;
            rgba[i * 4 + 1] = (c[1] * f * 255.0).clamp(0.0, 255.0) as u8;
            rgba[i * 4 + 2] = (c[2] * f * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
    RegionMap { rx, rz, rgba, top, height }
}
