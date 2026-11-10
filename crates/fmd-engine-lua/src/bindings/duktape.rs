use boa_engine::{Context, Source};
use mlua::{Lua, Result as LuaResult, Table};

pub fn register_duktape(lua: &Lua) -> LuaResult<()> {
    let duktape_table = lua.create_table()?;

    let exec_js = lua.create_function(|_, code: String| {
        let mut context = Context::default();
        match context.eval(Source::from_bytes(&code)) {
            Ok(val) => {
                let s = val
                    .to_string(&mut context)
                    .map(|js_str| js_str.to_std_string_escaped())
                    .unwrap_or_else(|_| "undefined".to_string());
                Ok(s)
            }
            Err(e) => Ok(format!("Error: {}", e)),
        }
    })?;

    duktape_table.set("ExecJS", exec_js)?;
    lua.globals().set("duktape", duktape_table)?;

    // Also register in package.loaded["duktape"]
    let package: Table = lua.globals().get("package")?;
    let loaded: Table = package.get("loaded")?;
    let duktape_table: Table = lua.globals().get("duktape")?;
    loaded.set("duktape", duktape_table)?;

    Ok(())
}
