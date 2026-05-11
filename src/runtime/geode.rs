use color_eyre::eyre::{self, Context};
use mlua::Lua;
use rayon::prelude::*;
use std::{
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
    root: PathBuf,
    glyphs: Vec<ScriptAsset>,
    systems: Vec<ScriptAsset>,
}

fn module_name(geode_name: &str, root: &Path, path: &Path) -> mlua::Result<String> {
    let rel = path.strip_prefix(root).map_err(mlua::Error::runtime)?;
    let mut parts: Vec<String> = rel
        .with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();

    // glyphs/grid.lua -> grid
    if parts.first().map(|s| s.as_str()) == Some("glyphs") {
        parts.remove(0);
    }

    // init.lua -> geode name
    if parts.last().map(|s| s.as_str()) == Some("init") {
        parts.pop();
    }

    if parts.is_empty() {
        Ok(geode_name.to_string())
    } else {
        Ok(format!("{}.{}", geode_name, parts.join(".")))
    }
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
            let systems = load_scripts_from_dir(&root.join("systems"));

            Ok(Geode {
                name,
                root: root.clone(),
                glyphs,
                systems,
            })
        })
        .collect();
    let mut geodes = geodes?;

    geodes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(geodes)
}

pub fn inject_geodes(lua: &Lua, geodes: &[Geode]) -> mlua::Result<()> {
    if geodes.is_empty() {
        return Ok(());
    }

    let package: mlua::Table = lua.globals().get("package")?;
    let preload: mlua::Table = package.get("preload")?;

    for geode in geodes {
        for glyph in &geode.glyphs {
            let contents = glyph.contents.clone();
            let chunk_name = format!("@{}", glyph.path.display());

            let name = module_name(&geode.name, &geode.root, &glyph.path)?;
            let loader = lua.create_function(move |lua, ()| {
                lua.load(&contents)
                    .set_name(&chunk_name)
                    .eval::<mlua::Value>()
            })?;

            preload.set(name, loader)?;
        }

        for system in &geode.systems {
            let path = system.path.to_string_lossy();
            lua.load(&system.contents).set_name(path.as_ref()).exec()?;
        }
    }

    Ok(())
}
