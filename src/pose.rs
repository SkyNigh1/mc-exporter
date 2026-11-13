//! Équivalent du PoseStack / ModelPart de Minecraft : matrices 4×4 et cubes de modèles d'entités.

use crate::registry::{Quad, Tint};

pub type M4 = [[f32; 4]; 4];

pub const IDENT: M4 = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];

pub fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j] + a[i][3] * b[3][j];
        }
    }
    r
}

pub fn translation(x: f32, y: f32, z: f32) -> M4 {
    let mut m = IDENT;
    m[0][3] = x;
    m[1][3] = y;
    m[2][3] = z;
    m
}

pub fn scaling(x: f32, y: f32, z: f32) -> M4 {
    let mut m = IDENT;
    m[0][0] = x;
    m[1][1] = y;
    m[2][2] = z;
    m
}

pub fn rot_x(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    [[1., 0., 0., 0.], [0., c, -s, 0.], [0., s, c, 0.], [0., 0., 0., 1.]]
}
pub fn rot_y(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    [[c, 0., s, 0.], [0., 1., 0., 0.], [-s, 0., c, 0.], [0., 0., 0., 1.]]
}
pub fn rot_z(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    [[c, -s, 0., 0.], [s, c, 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]]
}

/// Quaternion (x, y, z, w) → matrice
pub fn quat(q: [f32; 4]) -> M4 {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-9);
    let (x, y, z, w) = (q[0] / n, q[1] / n, q[2] / n, q[3] / n);
    [
        [1. - 2. * (y * y + z * z), 2. * (x * y - z * w), 2. * (x * z + y * w), 0.],
        [2. * (x * y + z * w), 1. - 2. * (x * x + z * z), 2. * (y * z - x * w), 0.],
        [2. * (x * z - y * w), 2. * (y * z + x * w), 1. - 2. * (x * x + y * y), 0.],
        [0., 0., 0., 1.],
    ]
}

/// PoseStack simplifié
#[derive(Clone, Copy)]
pub struct Pose(pub M4);

impl Pose {
    pub fn new() -> Pose {
        Pose(IDENT)
    }
    pub fn at(m: M4) -> Pose {
        Pose(m)
    }
    pub fn t(mut self, x: f32, y: f32, z: f32) -> Pose {
        self.0 = mul(&self.0, &translation(x, y, z));
        self
    }
    pub fn s(mut self, x: f32, y: f32, z: f32) -> Pose {
        self.0 = mul(&self.0, &scaling(x, y, z));
        self
    }
    pub fn rx(mut self, deg: f32) -> Pose {
        self.0 = mul(&self.0, &rot_x(deg.to_radians()));
        self
    }
    pub fn ry(mut self, deg: f32) -> Pose {
        self.0 = mul(&self.0, &rot_y(deg.to_radians()));
        self
    }
    pub fn rz(mut self, deg: f32) -> Pose {
        self.0 = mul(&self.0, &rot_z(deg.to_radians()));
        self
    }
    pub fn m(mut self, m: &M4) -> Pose {
        self.0 = mul(&self.0, m);
        self
    }
    /// ModelPart.translateAndRotate : offset en pixels, rotations en radians (ordre ZYX)
    pub fn part(self, offset: [f32; 3], rot: [f32; 3]) -> Pose {
        let p = self.t(offset[0] / 16.0, offset[1] / 16.0, offset[2] / 16.0);
        Pose(mul(&p.0, &mul(&rot_z(rot[2]), &mul(&rot_y(rot[1]), &rot_x(rot[0])))))
    }
    /// Comme `part`, avec une échelle de pièce (PartPose.scaled)
    pub fn part_s(self, offset: [f32; 3], rot: [f32; 3], scale: f32) -> Pose {
        self.part(offset, rot).s(scale, scale, scale)
    }

    pub fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }
    pub fn det(&self) -> f32 {
        let m = &self.0;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }
    /// Transforme un quad (positions) en conservant l'orientation des faces.
    pub fn quad(&self, q: &Quad) -> Quad {
        let mut o = q.clone();
        for i in 0..4 {
            o.pos[i] = self.point(q.pos[i]);
        }
        if self.det() < 0.0 {
            o.pos = [o.pos[3], o.pos[2], o.pos[1], o.pos[0]];
            o.uv = [o.uv[3], o.uv[2], o.uv[1], o.uv[0]];
        }
        o.dir = None;
        o.cull = None;
        o
    }
}

/// Texture d'entité pour la construction de cubes
#[derive(Clone, Copy)]
pub struct Skin {
    pub tex: u32,
    pub w: f32,
    pub h: f32,
    pub tint: Tint,
}

pub const ALL_FACES: u8 = 0b111111;
pub const F_DOWN: u8 = 1;
pub const F_UP: u8 = 2;
pub const F_WEST: u8 = 4;
pub const F_NORTH: u8 = 8;
pub const F_EAST: u8 = 16;
pub const F_SOUTH: u8 = 32;

/// Reproduit ModelPart.Cube : boîte en pixels, UV au gabarit standard.
#[allow(clippy::too_many_arguments)]
pub fn cube(out: &mut Vec<Quad>, pose: &Pose, sk: &Skin, u: f32, v: f32, from: [f32; 3], size: [f32; 3], grow: f32, mirror: bool, faces: u8) {
    cube_uv(out, pose, sk, u, v, from, size, size, grow, mirror, faces)
}

/// Comme `cube`, avec une taille de zone de texture différente de la taille géométrique
#[allow(clippy::too_many_arguments)]
pub fn cube_uv(out: &mut Vec<Quad>, pose: &Pose, sk: &Skin, u: f32, v: f32, from: [f32; 3], size: [f32; 3], uvsize: [f32; 3], grow: f32, mirror: bool, faces: u8) {
    let (w, h, d) = (size[0], size[1], size[2]);
    let (tw, th, td) = (uvsize[0], uvsize[1], uvsize[2]);
    let (mut x, y, z) = (from[0] - grow, from[1] - grow, from[2] - grow);
    let (mut f, g, hh) = (from[0] + w + grow, from[1] + h + grow, from[2] + d + grow);
    if mirror {
        std::mem::swap(&mut x, &mut f);
    }
    let p = |a: f32, b: f32, c: f32| [a / 16.0, b / 16.0, c / 16.0];
    let v0 = p(x, y, z);
    let v1 = p(f, y, z);
    let v2 = p(f, g, z);
    let v3 = p(x, g, z);
    let v4 = p(x, y, hh);
    let v5 = p(f, y, hh);
    let v6 = p(f, g, hh);
    let v7 = p(x, g, hh);
    let (j, k, l, m, n, o) = (u, u + td, u + td + tw, u + td + tw + tw, u + td + tw + td, u + td + tw + td + tw);
    let (pp, q, r) = (v, v + td, v + td + th);
    let polys: [(u8, [[f32; 3]; 4], [f32; 4]); 6] = [
        (F_DOWN, [v5, v4, v0, v1], [k, pp, l, q]),
        (F_UP, [v2, v3, v7, v6], [l, q, m, pp]),
        (F_WEST, [v0, v4, v7, v3], [j, q, k, r]),
        (F_NORTH, [v1, v0, v3, v2], [k, q, l, r]),
        (F_EAST, [v5, v1, v2, v6], [l, q, n, r]),
        (F_SOUTH, [v4, v5, v6, v7], [n, q, o, r]),
    ];
    for (flag, verts, [u1, v1_, u2, v2_]) in polys {
        if faces & flag == 0 {
            continue;
        }
        let uv = |uu: f32, vv: f32| [uu / sk.w, 1.0 - vv / sk.h];
        let mut pos = verts;
        let mut uvs = [uv(u2, v1_), uv(u1, v1_), uv(u1, v2_), uv(u2, v2_)];
        if mirror {
            pos = [pos[3], pos[2], pos[1], pos[0]];
            uvs = [uvs[3], uvs[2], uvs[1], uvs[0]];
        }
        // faces dégénérées (épaisseur nulle sur les deux axes du plan)
        let q = Quad { pos, uv: uvs, tex: sk.tex, tint: sk.tint, cull: None, dir: None, flat: w == 0.0 || h == 0.0 || d == 0.0 };
        out.push(pose.quad(&q));
    }
}

/// Raccourci : cube sans gonflement ni miroir
pub fn bx(out: &mut Vec<Quad>, pose: &Pose, sk: &Skin, uv: (f32, f32), from: [f32; 3], size: [f32; 3]) {
    cube(out, pose, sk, uv.0, uv.1, from, size, 0.0, false, ALL_FACES);
}

/// Quad plat texturé dans le plan z = 0 (x vers la droite, y vers le haut), face vers +z.
pub fn flat_quad(pose: &Pose, tex: u32, w: f32, h: f32, uv: [f32; 4]) -> Quad {
    let q = Quad {
        pos: [[0., 0., 0.], [w, 0., 0.], [w, h, 0.], [0., h, 0.]],
        uv: [[uv[0], uv[3]], [uv[2], uv[3]], [uv[2], uv[1]], [uv[0], uv[1]]],
        tex,
        tint: Tint::None,
        cull: None,
        dir: None,
        flat: true,
    };
    pose.quad(&q)
}
