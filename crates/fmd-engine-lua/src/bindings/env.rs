use mlua::{Lua, Result as LuaResult, Table};

pub fn register_env(lua: &Lua) -> LuaResult<()> {
    let env_table = lua.create_table()?;
    env_table.set("Directory", "./")?;
    env_table.set("ExeName", "fmd")?;
    env_table.set("Version", env!("CARGO_PKG_VERSION"))?;
    env_table.set("Revision", "1")?;
    env_table.set("LuaDirectory", "lua")?;
    env_table.set("SelectedLanguage", "en")?;

    // Register in package.preload["fmd.env"] and package.loaded["fmd.env"]
    let package: Table = lua.globals().get("package")?;
    let preload: Table = package.get("preload")?;
    let loaded: Table = package.get("loaded")?;

    let env_table_clone = env_table.clone();
    let loader = lua.create_function(move |_, ()| Ok(env_table_clone.clone()))?;

    preload.set("fmd.env", loader)?;
    loaded.set("fmd.env", env_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_env_exposes_expected_fields() {
        let lua = Lua::new();
        register_env(&lua).unwrap();

        let (exe, lua_dir, lang, version): (String, String, String, String) = lua
            .load(
                r#"
                local e = require('fmd.env')
                return e.ExeName, e.LuaDirectory, e.SelectedLanguage, e.Version
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(exe, "fmd");
        assert_eq!(lua_dir, "lua");
        assert_eq!(lang, "en");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
