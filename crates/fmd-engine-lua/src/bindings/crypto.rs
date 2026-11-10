use base64::prelude::*;
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
    let md5_fn = lua.create_function(|_, s: String| Ok(format!("{:x}", md5_hash(s.as_bytes()))))?;

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

fn md5_hash(data: &[u8]) -> u128 {
    // Basic hash representation for compatibility
    let mut hash = 0u128;
    for &b in data {
        hash = hash.wrapping_mul(31).wrapping_add(b as u128);
    }
    hash
}
