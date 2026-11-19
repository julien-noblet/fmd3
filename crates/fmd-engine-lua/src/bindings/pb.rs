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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pb_module_registered_and_loadfile_disabled() {
        let lua = Lua::new();
        register_pb(&lua).unwrap();

        let (loadfile_is_nil, hex_ok, roundtrip_ok): (bool, bool, bool) = lua
            .load(
                r#"
                local pb = require('pb')
                local raw = pb.fromhex('08 96 01')
                local hex = pb.tohex(raw)
                return pb.loadfile == nil, hex == '08 96 01', #raw == 3
                "#,
            )
            .eval()
            .unwrap();

        assert!(loadfile_is_nil);
        assert!(hex_ok);
        assert!(roundtrip_ok);
    }

    #[test]
    fn test_pb_encode_decode_with_protoc_schema() {
        let lua = Lua::new();
        register_pb(&lua).unwrap();

        let (name, id): (String, i64) = lua
            .load(
                r#"
                package.path = 'lua/?.lua;../../lua/?.lua;' .. package.path
                local protoc = require('utils.protoc')
                local pb = require('pb')
                protoc:load([[
                    syntax = "proto2";
                    message Manga {
                        required string name = 1;
                        optional int32 id = 2;
                    }
                ]])
                local encoded = pb.encode('Manga', { name = 'One Piece', id = 1180 })
                local decoded = pb.decode('Manga', encoded)
                pb.clear()
                return decoded.name, decoded.id
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(name, "One Piece");
        assert_eq!(id, 1180);
    }
}
