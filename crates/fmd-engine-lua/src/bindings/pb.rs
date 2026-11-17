use mlua::{Lua, Result as LuaResult, Table};

extern "C" {
    fn luaopen_pb(l: *mut mlua::lua_State) -> std::os::raw::c_int;
}

unsafe extern "C-unwind" fn c_luaopen_pb(l: *mut mlua::lua_State) -> std::os::raw::c_int {
    luaopen_pb(l)
}

pub fn register_pb(lua: &Lua) -> LuaResult<()> {
    unsafe {
        let open_fn = lua.create_c_function(c_luaopen_pb)?;
        let pb_table: Table = open_fn.call(())?;
        // Remove raw C fopen helper (Lpb_loadfile) so file reads cannot bypass the Lua io sandbox.
        pb_table.set("loadfile", mlua::Value::Nil)?;

        let package: Table = lua.globals().get("package")?;
        let preload: Table = package.get("preload")?;
        let loaded: Table = package.get("loaded")?;

        let pb_clone = pb_table.clone();
        let loader = lua.create_function(move |_, ()| Ok(pb_clone.clone()))?;

        preload.set("pb", loader)?;
        loaded.set("pb", pb_table)?;
    }

    Ok(())
}
