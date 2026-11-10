use mlua::{Lua, Result as LuaResult, Table};

pub fn register_mangafox(lua: &Lua) -> LuaResult<()> {
    let mf_table = lua.create_table()?;

    let load_template = lua.create_function(|_, _path: String| Ok(1))?;
    let remove_watermark =
        lua.create_function(|_, (_path, _save_png): (String, Option<bool>)| Ok(true))?;

    mf_table.set("LoadTemplate", load_template)?;
    mf_table.set("RemoveWatermark", remove_watermark)?;

    let package: Table = lua.globals().get("package")?;
    let preload: Table = package.get("preload")?;
    let loaded: Table = package.get("loaded")?;

    let mf_clone = mf_table.clone();
    let loader = lua.create_function(move |_, ()| Ok(mf_clone.clone()))?;

    preload.set("fmd.mangafoxwatermark", loader.clone())?;
    preload.set("mangafoxwatermark", loader)?;
    loaded.set("fmd.mangafoxwatermark", mf_table.clone())?;
    loaded.set("mangafoxwatermark", mf_table)?;

    Ok(())
}
