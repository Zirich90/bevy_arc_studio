//! Per-asset import settings: a `<asset>.arcmeta.json` sidecar next to a model (not Bevy's own `.meta`, which the
//! AssetServer would try to parse). `SpawnObject` of a `model:` kind applies it to the new object's transform.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::props::assets_dir;

pub const META_EXT: &str = ".arcmeta.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetMeta {
    /// Uniform scale multiplied into the spawned root.
    pub scale: f32,
    /// Extra yaw (degrees) so the model faces +Z / -Z the way the editor expects.
    pub yaw_deg: f32,
    /// Vertical shift in metres (pivot not at the feet).
    pub y_offset: f32,
}

impl Default for AssetMeta {
    fn default() -> Self {
        Self { scale: 1.0, yaw_deg: 0.0, y_offset: 0.0 }
    }
}

impl AssetMeta {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Fold the settings into a spawn transform.
    pub fn apply(&self, tf: &mut Transform) {
        tf.scale *= self.scale.clamp(0.001, 1000.0);
        tf.translation.y += self.y_offset;
        tf.rotation *= Quat::from_rotation_y(self.yaw_deg.to_radians());
    }
}

pub fn meta_path(base: &Path, rel: &str) -> PathBuf {
    base.join(format!("{rel}{META_EXT}"))
}

pub fn load_from(base: &Path, rel: &str) -> AssetMeta {
    std::fs::read_to_string(meta_path(base, rel)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Writes the sidecar; default settings remove it instead of leaving a no-op file.
pub fn save_to(base: &Path, rel: &str, m: &AssetMeta) -> std::io::Result<()> {
    let p = meta_path(base, rel);
    if m.is_default() {
        return match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::write(p, serde_json::to_string_pretty(m).unwrap_or_default())
}

/// `rel` is the path under `assets/` (`models/Duck.glb`).
pub fn load(rel: &str) -> AssetMeta {
    load_from(&assets_dir(), rel)
}

pub fn save(rel: &str, m: &AssetMeta) -> std::io::Result<()> {
    save_to(&assets_dir(), rel, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_round_trips_and_default_removes_it() {
        let base = std::env::temp_dir().join(format!("arc_meta_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("models")).unwrap();
        assert!(load_from(&base, "models/a.glb").is_default());
        let m = AssetMeta { scale: 2.5, yaw_deg: 90.0, y_offset: -0.5 };
        save_to(&base, "models/a.glb", &m).unwrap();
        assert_eq!(load_from(&base, "models/a.glb"), m);
        std::fs::write(meta_path(&base, "models/b.glb"), r#"{"scale":3.0}"#).unwrap();
        assert_eq!(load_from(&base, "models/b.glb"), AssetMeta { scale: 3.0, ..Default::default() });
        save_to(&base, "models/a.glb", &AssetMeta::default()).unwrap();
        assert!(!meta_path(&base, "models/a.glb").exists());
        let mut tf = Transform::from_xyz(1.0, 2.0, 3.0);
        m.apply(&mut tf);
        assert_eq!((tf.scale, tf.translation.y), (Vec3::splat(2.5), 1.5));
        let _ = std::fs::remove_dir_all(&base);
    }
}
