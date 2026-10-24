//! Procedural armoured knights: bone hierarchy of primitive-built armour plus a scripted sword-fight choreography.
//! Facing convention: a knight looks down local +Z, the sword arm is on local +X.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Blue,
    Red,
}

#[derive(Clone, Copy)]
#[repr(usize)]
enum B {
    Pelvis,
    Torso,
    Head,
    ArmRU,
    ArmRL,
    ArmLU,
    ArmLL,
    LegRU,
    LegRL,
    LegLU,
    LegLL,
    FootR,
    FootL,
    Cape,
    WristR,
}
const NB: usize = 15;

#[derive(Component)]
pub struct Bone;

#[derive(Component)]
pub struct Rig([Entity; NB]);

#[derive(Component)]
pub struct Fighter {
    pub duel: usize,
    pub side: u8,
    pub seed: f32,
}

/// Two knights trading blows around `center` along the axis given by `yaw`.
#[derive(Clone, Copy)]
pub struct Duel {
    pub center: Vec2,
    pub yaw: f32,
    pub period: f32,
    pub phase: f32,
    pub style: usize,
}

impl Duel {
    pub fn dir(&self) -> Vec2 {
        Vec2::new(self.yaw.sin(), self.yaw.cos())
    }
    /// (attacking side 0/1, u in 0..1 within the half cycle, style index)
    fn state(&self, t: f32) -> (u8, f32, usize) {
        let half = self.period * 0.5;
        let c = (t + self.phase).max(0.0);
        let k = (c / half).floor() as usize;
        ((k % 2) as u8, c / half - k as f32, (k / 2 + self.style) % 3)
    }
    /// Time of the most recent blade impact (None before the first one).
    pub fn last_impact(&self, t: f32) -> Option<f32> {
        let half = self.period * 0.5;
        let c = t + self.phase;
        let k = ((c / half) - IMPACT_U).floor();
        (k >= 0.0).then(|| (k + IMPACT_U) * half - self.phase)
    }
}

pub const IMPACT_U: f32 = 0.52;
const GAP: f32 = 2.9;

// ───────────────────────────── Pose ─────────────────────────────

const FWD: usize = 0;
const PELV_PITCH: usize = 1;
const T_YAW: usize = 2;
const T_PITCH: usize = 3;
const T_ROLL: usize = 4;
const H_YAW: usize = 5;
const H_PITCH: usize = 6;
const L: usize = 11; // shield arm: p, r, twist, elbow
const FRONT: usize = 15;
const BACK: usize = 16;
const HEIGHT: usize = 17;
const CAPE: usize = 18;
const SWH: usize = 19; // sword hand (root space)
const SWD: usize = 22; // blade direction (root space)
const AIM: usize = 25; // 0 = key direction, 1 = blade passes through the clash point
const N: usize = 26;

#[derive(Clone, Copy)]
pub struct Pose([f32; N]);

impl Pose {
    fn lerp(a: &Pose, b: &Pose, t: f32) -> Pose {
        let mut o = [0.0; N];
        for (i, v) in o.iter_mut().enumerate() {
            *v = a.0[i] + (b.0[i] - a.0[i]) * t;
        }
        Pose(o)
    }
    fn sword(mut self, h: [f32; 3], d: [f32; 3], aim: f32) -> Pose {
        self.0[SWH..SWH + 3].copy_from_slice(&h);
        self.0[SWD..SWD + 3].copy_from_slice(&d);
        self.0[AIM] = aim;
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn pose(fwd: f32, pp: f32, ty: f32, tp: f32, l: [f32; 4], fr: f32, bk: f32, h: f32, cape: f32) -> Pose {
    let mut p = [0.0; N];
    p[FWD] = fwd;
    p[PELV_PITCH] = pp;
    p[T_YAW] = ty;
    p[T_PITCH] = tp;
    p[L..L + 4].copy_from_slice(&l);
    p[FRONT] = fr;
    p[BACK] = bk;
    p[HEIGHT] = h;
    p[CAPE] = cape;
    Pose(p)
}

fn guard() -> Pose {
    pose(0.0, 0.10, -0.25, 0.04, [0.6, 0.35, 0.0, 1.6], 0.50, -0.34, 0.90, 0.18).sword([0.30, 1.22, 0.52], [0.05, 0.85, 0.5], 0.0)
}

fn wind(style: usize) -> Pose {
    match style {
        0 => pose(-0.15, -0.04, 0.45, -0.22, [0.25, 0.7, 0.0, 0.7], 0.34, -0.30, 0.93, 0.05).sword([0.28, 1.95, 0.0], [0.1, 0.55, -0.8], 0.0),
        1 => pose(-0.1, 0.0, 1.0, 0.0, [0.95, -0.15, 0.0, 2.1], 0.40, -0.32, 0.90, 0.10).sword([0.72, 1.45, -0.1], [0.85, 0.1, -0.5], 0.0),
        _ => pose(-0.22, 0.0, 0.55, 0.05, [0.8, 0.1, 0.0, 2.1], 0.36, -0.38, 0.88, 0.05).sword([0.3, 1.32, 0.12], [-0.05, 0.25, 0.95], 0.3),
    }
}

fn strike(style: usize) -> Pose {
    match style {
        0 => pose(0.55, 0.36, -0.5, 0.42, [0.15, 0.55, 0.0, 1.3], 0.95, -0.55, 0.72, 0.55).sword([0.28, 1.62, 0.62], [-0.2, -0.1, 0.9], 1.0),
        1 => pose(0.5, 0.18, -0.85, 0.12, [1.2, 0.5, 0.0, 1.9], 0.85, -0.5, 0.74, 0.5).sword([0.34, 1.45, 0.62], [-0.5, 0.0, 0.85], 1.0),
        _ => pose(0.72, 0.30, -0.25, 0.20, [0.5, 0.9, 0.0, 1.2], 1.0, -0.6, 0.70, 0.6).sword([0.30, 1.48, 0.74], [-0.25, 0.05, 0.95], 1.0),
    }
}

fn rebound(style: usize) -> Pose {
    Pose::lerp(&strike(style), &guard(), 0.35).sword([0.30, 1.75, 0.55], [0.05, 0.85, 0.5], 0.0)
}

fn parry(style: usize) -> Pose {
    match style {
        0 => pose(0.0, 0.05, -0.1, -0.1, [1.95, 0.05, 0.0, 1.9], 0.5, -0.35, 0.86, 0.1).sword([0.25, 1.32, 0.55], [-0.3, 0.2, 0.85], 1.0),
        1 => pose(0.0, 0.06, 0.3, 0.0, [1.3, -0.35, 0.0, 2.1], 0.5, -0.36, 0.86, 0.1).sword([0.4, 1.3, 0.55], [-0.45, 0.2, 0.8], 1.0),
        _ => pose(0.0, 0.06, 0.1, 0.0, [1.1, -0.3, 0.0, 2.1], 0.5, -0.36, 0.86, 0.1).sword([0.45, 1.28, 0.5], [-0.5, 0.25, 0.8], 1.0),
    }
}

fn recoil(style: usize) -> Pose {
    let mut p = parry(style);
    p.0[FWD] = -0.42;
    p.0[T_PITCH] = -0.28;
    p.0[PELV_PITCH] = -0.1;
    p.0[T_YAW] += 0.25;
    p.0[CAPE] = 0.55;
    p.0[HEIGHT] = 0.9;
    p.sword([0.3, 1.55, 0.3], [0.0, 0.9, 0.3], 0.0)
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn seg(u: f32, a: f32, b: f32) -> f32 {
    ((u - a) / (b - a)).clamp(0.0, 1.0)
}

fn attacker_pose(u: f32, s: usize) -> Pose {
    let (g, w, st, rb) = (guard(), wind(s), strike(s), rebound(s));
    if u < 0.28 {
        Pose::lerp(&g, &w, ease(seg(u, 0.0, 0.28)))
    } else if u < 0.40 {
        Pose::lerp(&w, &Pose::lerp(&w, &st, 0.06), ease(seg(u, 0.28, 0.40)))
    } else if u < IMPACT_U {
        let k = seg(u, 0.40, IMPACT_U);
        Pose::lerp(&Pose::lerp(&w, &st, 0.06), &st, k * k * k)
    } else if u < 0.68 {
        Pose::lerp(&st, &rb, ease(seg(u, IMPACT_U, 0.68)))
    } else {
        Pose::lerp(&rb, &g, ease(seg(u, 0.68, 1.0)))
    }
}

fn defender_pose(u: f32, s: usize) -> Pose {
    let (g, pa, rc) = (guard(), parry(s), recoil(s));
    if u < 0.30 {
        g
    } else if u < IMPACT_U - 0.02 {
        Pose::lerp(&g, &pa, ease(seg(u, 0.30, IMPACT_U - 0.02)))
    } else if u < 0.62 {
        Pose::lerp(&pa, &rc, ease(seg(u, IMPACT_U - 0.02, 0.62)))
    } else if u < 0.82 {
        Pose::lerp(&rc, &pa, ease(seg(u, 0.62, 0.82)))
    } else {
        Pose::lerp(&pa, &g, ease(seg(u, 0.82, 1.0)))
    }
}

// ───────────────────────────── Spawning ─────────────────────────────

#[derive(Resource)]
pub struct KnightKit {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    cyl: Handle<Mesh>,
    cone: Handle<Mesh>,
    steel: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    leather: Handle<StandardMaterial>,
    blade: Handle<StandardMaterial>,
    cloth: [Handle<StandardMaterial>; 2],
    cloth_dark: [Handle<StandardMaterial>; 2],
    fur: Handle<StandardMaterial>,
}

impl KnightKit {
    pub fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        let mut m = |c: [f32; 3], metal: f32, rough: f32| mats.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), metallic: metal, perceptual_roughness: rough, ..default() });
        Self {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            sphere: meshes.add(Sphere::new(1.0).mesh().uv(28, 18)),
            cyl: meshes.add(Cylinder::new(1.0, 1.0).mesh().resolution(24)),
            cone: meshes.add(Cone::new(1.0, 1.0).mesh().resolution(16)),
            steel: m([0.66, 0.69, 0.76], 1.0, 0.30),
            dark: m([0.30, 0.31, 0.36], 0.9, 0.42),
            gold: m([1.0, 0.76, 0.30], 1.0, 0.28),
            leather: m([0.24, 0.14, 0.08], 0.0, 0.7),
            blade: m([0.88, 0.9, 0.95], 1.0, 0.12),
            cloth: [m([0.12, 0.28, 0.75], 0.0, 0.85), m([0.72, 0.09, 0.08], 0.0, 0.85)],
            cloth_dark: [m([0.05, 0.12, 0.36], 0.0, 0.9), m([0.33, 0.04, 0.05], 0.0, 0.9)],
            fur: m([0.92, 0.92, 0.9], 0.0, 0.95),
        }
    }
}

struct Spawner<'a, 'w, 's> {
    c: &'a mut Commands<'w, 's>,
    kit: &'a KnightKit,
}

impl Spawner<'_, '_, '_> {
    fn bone(&mut self, parent: Entity, t: Vec3) -> Entity {
        self.c.spawn((Bone, Transform::from_translation(t), Visibility::default(), ChildOf(parent))).id()
    }
    fn part(&mut self, parent: Entity, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Vec3, scale: Vec3, rot: Quat) {
        self.c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform { translation: t, rotation: rot, scale }, ChildOf(parent)));
    }
    fn cube(&mut self, p: Entity, mat: &Handle<StandardMaterial>, t: Vec3, size: Vec3) {
        let k = self.kit.cube.clone();
        self.part(p, &k, mat, t, size, Quat::IDENTITY);
    }
    fn ball(&mut self, p: Entity, mat: &Handle<StandardMaterial>, t: Vec3, radii: Vec3) {
        let k = self.kit.sphere.clone();
        self.part(p, &k, mat, t, radii, Quat::IDENTITY);
    }
    /// Cylinder along local Y.
    fn cyl(&mut self, p: Entity, mat: &Handle<StandardMaterial>, t: Vec3, r: f32, h: f32) {
        let k = self.kit.cyl.clone();
        self.part(p, &k, mat, t, Vec3::new(r, h, r), Quat::IDENTITY);
    }
    /// Cylinder along local Z.
    fn disc(&mut self, p: Entity, mat: &Handle<StandardMaterial>, t: Vec3, r: f32, thick: f32) {
        let k = self.kit.cyl.clone();
        self.part(p, &k, mat, t, Vec3::new(r, thick, r), Quat::from_rotation_x(FRAC_PI_2));
    }
}

pub fn spawn_knight(commands: &mut Commands, kit: &KnightKit, team: Team, duel: usize, side: u8, seed: f32) -> Entity {
    let ti = team as usize;
    let root = commands.spawn((Name::new(format!("Knight{duel}.{side}")), Fighter { duel, side, seed }, Transform::default(), Visibility::default())).id();
    let mut s = Spawner { c: commands, kit };
    let (steel, dark, gold, leather, blade) = (kit.steel.clone(), kit.dark.clone(), kit.gold.clone(), kit.leather.clone(), kit.blade.clone());
    let (cloth, cloth_d, fur) = (kit.cloth[ti].clone(), kit.cloth_dark[ti].clone(), kit.fur.clone());
    let v = Vec3::new;

    let pelvis = s.bone(root, v(0.0, 0.9, 0.0));
    // pelvis: hip plate, belt, tabard skirts
    s.cube(pelvis, &dark, v(0.0, -0.03, 0.0), v(0.34, 0.14, 0.22));
    s.cyl(pelvis, &leather, v(0.0, 0.07, 0.0), 0.185, 0.07);
    s.cube(pelvis, &gold, v(0.0, 0.07, 0.19), v(0.06, 0.06, 0.02));
    let skirt = s.c.spawn((Transform::from_xyz(0.0, 0.0, 0.0), Visibility::default(), ChildOf(pelvis))).id();
    s.part(skirt, &kit.cube.clone(), &cloth, v(0.0, -0.33, 0.125), v(0.23, 0.5, 0.018), Quat::from_rotation_x(0.07));
    s.part(skirt, &kit.cube.clone(), &cloth_d, v(0.0, -0.3, -0.125), v(0.23, 0.44, 0.018), Quat::from_rotation_x(-0.06));
    s.part(skirt, &kit.cube.clone(), &gold, v(0.0, -0.575, 0.134), v(0.235, 0.02, 0.02), Quat::from_rotation_x(0.07));
    for sx in [-1.0, 1.0] {
        s.part(skirt, &kit.cube.clone(), &steel, v(sx * 0.19, -0.14, 0.02), v(0.07, 0.22, 0.2), Quat::from_rotation_z(-sx * 0.12));
    }

    // torso
    let torso = s.bone(pelvis, v(0.0, 0.10, 0.0));
    s.cube(torso, &leather, v(0.0, 0.08, 0.0), v(0.28, 0.2, 0.2));
    s.ball(torso, &steel, v(0.0, 0.33, 0.01), v(0.215, 0.25, 0.15));
    s.cube(torso, &cloth, v(0.0, 0.25, 0.158), v(0.2, 0.6, 0.014));
    s.cube(torso, &gold, v(0.0, 0.52, 0.162), v(0.205, 0.03, 0.018));
    s.cube(torso, &gold, v(0.0, 0.2, 0.164), v(0.045, 0.45, 0.016));
    s.ball(torso, &dark, v(0.0, 0.34, -0.07), v(0.2, 0.24, 0.12));
    s.cyl(torso, &steel, v(0.0, 0.62, 0.0), 0.095, 0.1);
    for sx in [-1.0, 1.0] {
        s.ball(torso, &steel, v(sx * 0.295, 0.56, 0.0), v(0.125, 0.095, 0.13));
        s.ball(torso, &steel, v(sx * 0.31, 0.5, 0.0), v(0.115, 0.06, 0.125));
        s.ball(torso, &gold, v(sx * 0.3, 0.605, 0.0), v(0.05, 0.02, 0.05));
    }
    // fur mantle
    s.ball(torso, &fur, v(0.0, 0.6, -0.02), v(0.26, 0.07, 0.19));

    // head
    let head = s.bone(torso, v(0.0, 0.7, 0.0));
    s.ball(head, &steel, v(0.0, 0.1, 0.0), v(0.125, 0.145, 0.145));
    s.cube(head, &dark, v(0.0, 0.085, 0.118), v(0.19, 0.03, 0.05));
    s.cube(head, &dark, v(0.0, 0.01, 0.13), v(0.035, 0.1, 0.03));
    s.cyl(head, &steel, v(0.0, 0.0, 0.0), 0.11, 0.1);
    s.cube(head, &cloth, v(0.0, 0.27, -0.02), v(0.03, 0.09, 0.26));
    s.cube(head, &gold, v(0.0, 0.222, 0.0), v(0.036, 0.016, 0.16));

    // arms. sword arm = +X
    let mut arms = [Entity::PLACEHOLDER; 4];
    let mut wrist = Entity::PLACEHOLDER;
    for (i, sx) in [1.0_f32, -1.0].into_iter().enumerate() {
        let u = s.bone(torso, v(sx * 0.31, 0.55, 0.0));
        s.cyl(u, &dark, v(0.0, -0.15, 0.0), 0.052, 0.3);
        s.ball(u, &steel, v(0.0, -0.32, 0.0), v(0.065, 0.065, 0.065));
        let l = s.bone(u, v(0.0, -0.32, 0.0));
        s.cyl(l, &steel, v(0.0, -0.15, 0.0), 0.05, 0.28);
        s.cyl(l, &gold, v(0.0, -0.03, 0.0), 0.056, 0.03);
        s.ball(l, &dark, v(0.0, -0.31, 0.0), v(0.058, 0.07, 0.065));
        arms[i * 2] = u;
        arms[i * 2 + 1] = l;
        if sx > 0.0 {
            // longsword: held with the blade pointing along local +Z when the forearm hangs
            let sw = s.bone(l, v(0.0, -0.3, 0.0));
            wrist = sw;
            s.cyl(sw, &leather, v(0.0, 0.0, 0.0), 0.022, 0.17);
            s.ball(sw, &gold, v(0.0, -0.105, 0.0), v(0.034, 0.034, 0.034));
            s.cube(sw, &gold, v(0.0, 0.095, 0.0), v(0.26, 0.035, 0.05));
            s.cube(sw, &blade, v(0.0, 0.6, 0.0), v(0.058, 1.0, 0.014));
            s.cube(sw, &steel, v(0.0, 0.6, 0.0), v(0.016, 1.0, 0.02));
            s.part(sw, &kit.cone.clone(), &blade, v(0.0, 1.12, 0.0), v(0.029, 0.14, 0.007), Quat::IDENTITY);
        } else {
            // heater-style round shield strapped to the forearm
            s.disc(l, &gold, v(0.0, -0.15, -0.095), 0.355, 0.03);
            s.disc(l, &cloth, v(0.0, -0.15, -0.113), 0.325, 0.03);
            s.cube(l, &steel, v(0.0, -0.15, -0.125), v(0.06, 0.6, 0.012));
            s.cube(l, &steel, v(0.0, -0.15, -0.125), v(0.6, 0.06, 0.012));
            s.ball(l, &steel, v(0.0, -0.15, -0.125), v(0.085, 0.085, 0.05));
            s.disc(l, &dark, v(0.0, -0.15, -0.07), 0.3, 0.03);
        }
    }

    // legs
    let mut legs = [Entity::PLACEHOLDER; 6];
    for (i, sx) in [1.0_f32, -1.0].into_iter().enumerate() {
        let th = s.bone(pelvis, v(sx * 0.11, -0.02, 0.0));
        s.cyl(th, &cloth_d, v(0.0, -0.23, 0.0), 0.075, 0.46);
        s.cyl(th, &dark, v(0.0, -0.18, 0.0), 0.082, 0.3);
        s.ball(th, &steel, v(0.0, -0.46, 0.04), v(0.075, 0.075, 0.08));
        let sh = s.bone(th, v(0.0, -0.46, 0.0));
        s.cyl(sh, &steel, v(0.0, -0.22, 0.0), 0.062, 0.44);
        s.cube(sh, &dark, v(0.0, -0.2, 0.06), v(0.075, 0.34, 0.03));
        let foot = s.bone(sh, v(0.0, -0.46, 0.0));
        s.cube(foot, &leather, v(0.0, -0.04, 0.06), v(0.11, 0.09, 0.28));
        s.ball(foot, &dark, v(0.0, -0.035, 0.18), v(0.058, 0.045, 0.07));
        legs[i * 3] = th;
        legs[i * 3 + 1] = sh;
        legs[i * 3 + 2] = foot;
    }

    // cape
    let cape = s.bone(torso, v(0.0, 0.6, -0.17));
    s.cube(cape, &cloth_d, v(0.0, -0.46, 0.0), v(0.44, 0.92, 0.02));
    s.cube(cape, &gold, v(0.0, -0.93, 0.0), v(0.445, 0.02, 0.024));

    let rig = [pelvis, torso, head, arms[0], arms[1], arms[2], arms[3], legs[0], legs[1], legs[3], legs[4], legs[2], legs[5], cape, wrist];
    commands.entity(root).insert(Rig(rig));
    root
}

// ───────────────────────────── Animation ─────────────────────────────

fn rx(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn ry(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn rz(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

const L1: f32 = 0.46;
const FOOT: f32 = 0.09;
const SHOULDER: Vec3 = Vec3::new(0.31, 0.55, 0.0);
const UPPER: f32 = 0.32;
const FORE: f32 = 0.30;
/// Height of the point where the blades meet.
pub const CLASH_Y: f32 = 1.5;

/// Shin angle (absolute, forward positive) so that the foot sole touches the ground for hip height `h` and thigh angle `a`.
fn leg_ik(a: f32, h: f32) -> f32 {
    let c = ((h - 0.02 - FOOT) / L1 - a.cos()).clamp(-1.0, 1.0);
    -c.acos()
}

pub fn pose_for(duels: &[Duel], f: &Fighter, t: f32) -> Pose {
    let d = &duels[f.duel];
    let (att, u, style) = d.state(t);
    let mut p = if att == f.side { attacker_pose(u, style) } else { defender_pose(u, style) };
    let tt = t + f.seed * 7.0;
    p.0[HEIGHT] += 0.012 * (tt * 2.3).sin();
    p.0[T_PITCH] += 0.015 * (tt * 1.7).sin();
    p.0[H_PITCH] = -p.0[T_PITCH] * 0.7 - p.0[PELV_PITCH] * 0.5;
    p.0[H_YAW] = -p.0[T_YAW] * 0.8;
    p
}

/// Root position (xz) and yaw of one fighter.
fn root_of(d: &Duel, side: u8, fwd: f32) -> (Vec2, f32) {
    let dir = d.dir();
    let (sgn, yaw, facing) = if side == 0 { (-1.0, d.yaw, dir) } else { (1.0, d.yaw + PI, -dir) };
    (d.center + dir * (sgn * GAP * 0.5) + facing * fwd, yaw)
}

/// World point where both blades meet at time `t` (midpoint of the two knights).
pub fn clash_point(duels: &[Duel], di: usize, t: f32) -> Vec3 {
    let d = &duels[di];
    let mut mid = Vec2::ZERO;
    for side in 0..2u8 {
        let f = Fighter { duel: di, side, seed: 0.0 };
        mid += root_of(d, side, pose_for(duels, &f, t).0[FWD]).0 * 0.5;
    }
    Vec3::new(mid.x, super::PLATEAU + CLASH_Y, mid.y)
}

/// Two-bone IK for the sword arm in torso space: (upper-arm rotation, elbow flexion).
fn arm_ik(hand: Vec3) -> (Quat, f32) {
    let to = hand - SHOULDER;
    let d = to.length().clamp(0.12, UPPER + FORE - 0.005);
    let dir = to.normalize_or(Vec3::NEG_Y);
    let a = (UPPER * UPPER - FORE * FORE + d * d) / (2.0 * d);
    let h = (UPPER * UPPER - a * a).max(0.0).sqrt();
    let pole = Vec3::new(0.7, -0.8, -0.35);
    let perp = (pole - dir * pole.dot(dir)).normalize_or(Vec3::NEG_Y);
    let elbow = SHOULDER + dir * a + perp * h;
    let wrist = SHOULDER + dir * d;
    let du = (elbow - SHOULDER).normalize();
    let df = (wrist - elbow).normalize_or(du);
    let z = (df - du * df.dot(du)).normalize_or(-perp);
    let y = -du;
    let x = y.cross(z);
    (Quat::from_mat3(&Mat3::from_cols(x, y, z)), df.dot(du).clamp(-1.0, 1.0).acos())
}

#[allow(clippy::type_complexity)]
pub fn animate_knights(clock: Res<super::Clock>, duels: Res<super::Duels>, fighters: Query<(&Fighter, &Rig, &mut Transform)>, mut bones: Query<&mut Transform, (With<Bone>, Without<Fighter>)>) {
    let t = clock.t;
    for (f, rig, mut root) in fighters {
        let d = duels.0[f.duel];
        let p = pose_for(&duels.0, f, t);
        let (pos, yaw) = root_of(&d, f.side, p.0[FWD]);
        root.translation = Vec3::new(pos.x, super::PLATEAU, pos.y);
        root.rotation = ry(yaw);
        let mut set = |b: B, tr: Option<Vec3>, q: Quat| {
            if let Ok(mut x) = bones.get_mut(rig.0[b as usize]) {
                if let Some(tr) = tr {
                    x.translation = tr;
                }
                x.rotation = q;
            }
        };
        let g = |i: usize| p.0[i];
        let h = g(HEIGHT);
        let pp = g(PELV_PITCH);
        let q_pelvis = rx(pp);
        let q_torso = ry(g(T_YAW)) * rx(g(T_PITCH)) * rz(g(T_ROLL));
        set(B::Pelvis, Some(Vec3::new(0.0, h, 0.0)), q_pelvis);
        set(B::Torso, None, q_torso);
        set(B::Head, None, ry(g(H_YAW)) * rx(g(H_PITCH)));

        // sword arm: hand + blade direction given in root space, solved in torso space
        let q_chain = q_pelvis * q_torso;
        let torso_origin = Vec3::new(0.0, h, 0.0) + q_pelvis * Vec3::new(0.0, 0.10, 0.0);
        let inv = q_chain.inverse();
        let hand_root = Vec3::new(g(SWH), g(SWH + 1), g(SWH + 2));
        let c_world = clash_point(&duels.0, f.duel, t);
        let c_root = root.rotation.inverse() * (c_world - root.translation);
        let key_d = Vec3::new(g(SWD), g(SWD + 1), g(SWD + 2)).normalize_or(Vec3::Y);
        let aim_d = (c_root - hand_root).normalize_or(key_d);
        let dir_root = key_d.lerp(aim_d, g(AIM)).normalize_or(key_d);
        let (q_up, elbow) = arm_ik(inv * (hand_root - torso_origin));
        let q_fore = q_up * rx(-elbow);
        let dt = inv * dir_root;
        let refv = if dt.y.abs() < 0.9 { Vec3::Y } else { Vec3::Z };
        let xs = refv.cross(dt).normalize_or(Vec3::X);
        let q_sword = Quat::from_mat3(&Mat3::from_cols(xs, dt, xs.cross(dt)));
        set(B::ArmRU, None, q_up);
        set(B::ArmRL, None, rx(-elbow));
        set(B::WristR, None, q_fore.inverse() * q_sword);

        // shield arm
        let arm = |o: usize, side: f32| ry(side * g(o + 2)) * rx(-g(o)) * rz(side * g(o + 1));
        set(B::ArmLU, None, arm(L, -1.0));
        set(B::ArmLL, None, rx(-g(L + 3)));

        // legs: sword side (+X) is the back leg; both soles stay on the ground
        for (a, up, lo, ft) in [(g(BACK), B::LegRU, B::LegRL, B::FootR), (g(FRONT), B::LegLU, B::LegLL, B::FootL)] {
            let theta = leg_ik(a, h);
            set(up, None, rx(-(a - pp)));
            set(lo, None, rx(a - theta));
            set(ft, None, rx(theta));
        }
        set(B::Cape, None, rx(g(CAPE) + 0.08 * (t * 3.1 + f.seed * 5.0).sin()));
    }
}
