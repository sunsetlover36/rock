use std::path::{Path, PathBuf};

use super::geode::{Geode, inject_geodes};

pub(crate) fn boot_gamemode(lua: &mlua::Lua, name: &str, geodes: &[Geode]) -> mlua::Result<()> {
    // Configure package path
    let server_root = PathBuf::from(".");
    let gamemodes_dir = server_root.join("gamemodes");
    configure_package_path(lua, &gamemodes_dir)?;

    // Geodes injection
    inject_geodes(lua, geodes)?;

    // Boot gamemode
    let gamemode_path = gamemodes_dir.join(format!("{name}.lua"));
    let gamemode = std::fs::read_to_string(&gamemode_path)?;
    lua.load(&gamemode).set_name(name).exec()
}

fn configure_package_path(lua: &mlua::Lua, gamemodes_dir: &Path) -> mlua::Result<()> {
    let package: mlua::Table = lua.globals().get("package")?;
    let old_path: String = package.get("path")?;

    let new_path = format!(
        "{};{};{}",
        old_path,
        gamemodes_dir.join("?.lua").display(),
        gamemodes_dir.join("?/init.lua").display()
    );
    package.set("path", new_path)?;

    Ok(())
}
