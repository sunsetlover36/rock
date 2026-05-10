use color_eyre::eyre::{self, Context};
use mlua::Lua;
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use crate::runtime::{app_data, utils::LuaResultExt};

#[derive(Debug)]
struct ScriptAsset {
    path: PathBuf,
    contents: String,
}

#[derive(Debug)]
pub struct Geode {
    name: String,
    glyphs: Vec<ScriptAsset>,
    blueprints: Vec<ScriptAsset>,
    systems: Vec<ScriptAsset>,
    assets: HashMap<String, PathBuf>,
}

fn load_scripts_from_dir(path: &Path) -> Vec<ScriptAsset> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension() == Some(OsStr::new("lua")))
        .filter_map(|e| {
            let path = e.path().to_path_buf();
            match std::fs::read_to_string(&path) {
                Ok(contents) => Some(ScriptAsset { path, contents }),
                Err(err) => {
                    eprintln!("Failed to read script {:?}: {}", path, err);
                    None
                }
            }
        })
        .collect()
}
pub fn scan_geodes() -> eyre::Result<Vec<Geode>> {
    let mut paths: Vec<PathBuf> = vec![];

    if !Path::new("geodes").is_dir() {
        return Ok(Vec::new());
    }

    for entry in WalkDir::new("geodes").min_depth(1).max_depth(1) {
        let entry = entry.wrap_err("Failed to parse a file")?;
        for geode in WalkDir::new(entry.path()).min_depth(1).max_depth(1) {
            let geode = geode.wrap_err("Failed to parse a file")?;
            if geode.file_name() == "geode.toml" {
                paths.push(entry.path().to_path_buf());
            }
        }
    }

    let geode_roots: Vec<PathBuf> = WalkDir::new("geodes")
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.into_path())
        .filter(|p| p.join("geode.toml").exists())
        .collect();

    let geodes: eyre::Result<Vec<Geode>> = geode_roots
        .par_iter()
        .map(|root| -> eyre::Result<Geode> {
            let name = root
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| eyre::eyre!("Invalid geode path: {}", root.display()))?
                .to_owned();

            let glyphs = load_scripts_from_dir(&root.join("glyphs"));
            let blueprints = load_scripts_from_dir(&root.join("blueprints"));
            let systems = load_scripts_from_dir(&root.join("systems"));

            let mut assets = HashMap::new();
            let assets_root = root.join("assets");
            if assets_root.exists() {
                for entry in WalkDir::new(&assets_root)
                    .into_iter()
                    .filter_map(Result::ok)
                {
                    if entry.file_type().is_file() {
                        let asset_path = entry.path();
                        if let Ok(relative_path) = asset_path.strip_prefix(&assets_root) {
                            let key = relative_path.to_string_lossy().replace("\\", "/");
                            assets.insert(key, asset_path.to_path_buf());
                        }
                    }
                }
            }

            Ok(Geode {
                name,
                glyphs,
                blueprints,
                systems,
                assets,
            })
        })
        .collect();
    let mut geodes = geodes?;

    geodes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(geodes)
}

pub fn inject_geodes(lua: &Lua, geodes: &[Geode]) -> eyre::Result<()> {
    if geodes.is_empty() {
        lua.set_app_data::<app_data::RuntimePhase>(app_data::RuntimePhase::Gamemode);
        return Ok(());
    }

    for geode in geodes {
        lua.set_app_data::<app_data::RuntimePhase>(app_data::RuntimePhase::Glyphs);
        for glyph in &geode.glyphs {
            let path = glyph.path.to_string_lossy().to_string();
            lua.load(&glyph.contents)
                .set_name(&path)
                .exec()
                .wrap_err(&format!("Failed to load a glyph at path {}", &path))?;
        }
    }
    for geode in geodes {
        lua.set_app_data::<app_data::RuntimePhase>(app_data::RuntimePhase::Blueprints);
        for bp in &geode.blueprints {
            let path = bp.path.to_string_lossy().to_string();
            lua.load(&bp.contents)
                .set_name(&path)
                .exec()
                .wrap_err(&format!(
                    "Failed to load a blueprint script at path {}",
                    &path
                ))?;
        }
    }
    for geode in geodes {
        lua.set_app_data::<app_data::RuntimePhase>(app_data::RuntimePhase::Systems);
        for system in &geode.systems {
            let path = system.path.to_string_lossy().to_string();
            lua.load(&system.contents)
                .set_name(&path)
                .exec()
                .wrap_err(&format!("Failed to load a system script at path {}", &path))?;
        }
    }

    lua.set_app_data::<app_data::RuntimePhase>(app_data::RuntimePhase::Gamemode);
    Ok(())
}
