use color_eyre::eyre::{self, Context};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

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

    let mut geodes: Vec<Geode> = geode_roots
        .par_iter()
        .map(|root| {
            let name = root.file_name().unwrap().to_string_lossy().to_string();

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

            Geode {
                name,
                glyphs,
                blueprints,
                systems,
                assets,
            }
        })
        .collect();

    geodes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(geodes)
}

struct ScriptAssetCompositionParams<'a> {
    script_string: &'a mut String,
    script_asset: &'a ScriptAsset,
    display_name: &'a str,
    scoped: bool,
}
fn compose_script_asset(params: ScriptAssetCompositionParams) {
    let ScriptAssetCompositionParams {
        script_string,
        script_asset,
        display_name,
        scoped,
    } = params;
    script_string.push_str(&format!("-- {}: {:?}\n", display_name, script_asset.path));

    if scoped {
        script_string.push_str("do\n");
    }
    script_string.push_str(&script_asset.contents);
    if scoped {
        script_string.push_str("end\n");
    }
    script_string.push_str("\n");
}
pub fn compose_geodes(geodes: Vec<Geode>) -> eyre::Result<String> {
    let mut script = String::new();
    if geodes.is_empty() {
        return Ok(script);
    }

    for geode in geodes {
        script.push_str(&format!("-- [GEODE: {}] --\n", geode.name));

        for glyph in &geode.glyphs {
            compose_script_asset(ScriptAssetCompositionParams {
                script_string: &mut script,
                script_asset: glyph,
                display_name: "Glyph",
                scoped: false,
            });
        }
        for bp in &geode.blueprints {
            compose_script_asset(ScriptAssetCompositionParams {
                script_string: &mut script,
                script_asset: bp,
                display_name: "Blueprint",
                scoped: true,
            });
        }
        for system in &geode.systems {
            compose_script_asset(ScriptAssetCompositionParams {
                script_string: &mut script,
                script_asset: system,
                display_name: "System",
                scoped: true,
            });
        }
    }

    Ok(script)
}
