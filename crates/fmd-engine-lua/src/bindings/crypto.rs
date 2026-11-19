use base64::prelude::*;
use md5::{Digest, Md5};
use mlua::{Lua, Result as LuaResult, Table};

pub fn register_crypto(lua: &Lua) -> LuaResult<()> {
    let crypto_table = lua.create_table()?;

    // Base64 encode
    let encode_base64 =
        lua.create_function(|_, data: String| Ok(BASE64_STANDARD.encode(data.as_bytes())))?;

    // Base64 decode
    let decode_base64 = lua.create_function(|lua, data: String| {
        // Strip any whitespace
        let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
        match BASE64_STANDARD.decode(cleaned.as_bytes()) {
            Ok(bytes) => Ok(lua.create_string(&bytes)?),
            Err(e) => Err(mlua::Error::RuntimeError(format!(
                "Base64 decode error: {}",
                e
            ))),
        }
    })?;

    // MD5
    let md5_fn = lua.create_function(|_, s: mlua::String| {
        let mut hasher = Md5::new();
        hasher.update(s.as_bytes());
        Ok(hex::encode(hasher.finalize()))
    })?;

    // HexToStr (binary bytes from hex string)
    let hex_to_str = lua.create_function(|lua, hex: String| {
        let mut bytes = Vec::new();
        let chars: Vec<char> = hex.chars().filter(|c| !c.is_whitespace()).collect();
        for chunk in chars.chunks(2) {
            if chunk.len() == 2 {
                let s: String = chunk.iter().collect();
                if let Ok(b) = u8::from_str_radix(&s, 16) {
                    bytes.push(b);
                }
            }
        }
        lua.create_string(&bytes)
    })?;

    // StrToHex (hex string from binary bytes)
    let str_to_hex = lua.create_function(|_, data: mlua::String| {
        let hex: String = data
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        Ok(hex)
    })?;

    crypto_table.set("MD5", md5_fn)?;
    crypto_table.set("EncodeBase64", encode_base64)?;
    crypto_table.set("DecodeBase64", decode_base64)?;
    crypto_table.set("HexToStr", hex_to_str)?;
    crypto_table.set("StrToHex", str_to_hex)?;

    let package: Table = lua.globals().get("package")?;
    let preload: Table = package.get("preload")?;
    let loaded: Table = package.get("loaded")?;

    let crypto_table_clone = crypto_table.clone();
    let loader = lua.create_function(move |_, ()| Ok(crypto_table_clone.clone()))?;

    preload.set("fmd.crypto", loader.clone())?;
    preload.set("crypto", loader)?;
    loaded.set("fmd.crypto", crypto_table.clone())?;
    loaded.set("crypto", crypto_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua_with_crypto() -> Lua {
        let lua = Lua::new();
        register_crypto(&lua).unwrap();
        lua
    }

    #[test]
    fn test_md5_rfc1321_vectors() {
        let lua = setup_lua_with_crypto();
        let empty: String = lua
            .load("return require('fmd.crypto').MD5('')")
            .eval()
            .unwrap();
        assert_eq!(empty, "d41d8cd98f00b204e9800998ecf8427e");

        let abc: String = lua
            .load("return require('crypto').MD5('abc')")
            .eval()
            .unwrap();
        assert_eq!(abc, "900150983cd24fb0d6963f7d28e17f72");

        let msg: String = lua
            .load("return require('fmd.crypto').MD5('message digest')")
            .eval()
            .unwrap();
        assert_eq!(msg, "f96b697d7cb7938d525a2f31aaf161d0");
    }

    #[test]
    fn test_base64_and_hex_helpers() {
        let lua = setup_lua_with_crypto();
        let encoded: String = lua
            .load("return require('fmd.crypto').EncodeBase64('hello fmd3')")
            .eval()
            .unwrap();
        assert_eq!(encoded, "aGVsbG8gZm1kMw==");

        let decoded: String = lua
            .load("return require('fmd.crypto').DecodeBase64('aGVs\\n bG8g Zm1kMw==')")
            .eval()
            .unwrap();
        assert_eq!(decoded, "hello fmd3");

        let invalid: mlua::Result<mlua::Value> = lua
            .load("return require('fmd.crypto').DecodeBase64('not!valid!base64')")
            .eval();
        assert!(invalid.is_err());

        let hex_roundtrip: bool = lua
            .load(
                r#"
                local c = require('crypto')
                local bin = c.HexToStr('00 ff 41 42')
                return #bin == 4 and c.StrToHex(bin) == '00ff4142'
                "#,
            )
            .eval()
            .unwrap();
        assert!(hex_roundtrip);
    }
}
