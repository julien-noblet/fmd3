use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};
use std::sync::{Arc, Mutex};

pub fn create_string_list_table(lua: &Lua) -> LuaResult<(Table, Arc<Mutex<Vec<String>>>)> {
    let items = Arc::new(Mutex::new(Vec::new()));
    let table = lua.create_table()?;

    // Add
    let items_add = items.clone();
    table.set(
        "Add",
        lua.create_function(move |_, args: MultiValue| {
            let val = extract_string_arg(&args, 0).unwrap_or_default();
            if let Ok(mut it) = items_add.lock() {
                it.push(val);
            }
            Ok(())
        })?,
    )?;

    // Clear
    let items_clear = items.clone();
    table.set(
        "Clear",
        lua.create_function(move |_, _: MultiValue| {
            if let Ok(mut it) = items_clear.lock() {
                it.clear();
            }
            Ok(())
        })?,
    )?;

    // Reverse
    let items_rev = items.clone();
    table.set(
        "Reverse",
        lua.create_function(move |_, _: MultiValue| {
            if let Ok(mut it) = items_rev.lock() {
                it.reverse();
            }
            Ok(())
        })?,
    )?;

    // Metatable
    let meta = lua.create_table()?;
    let items_index = items.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (tbl, key): (Table, Value)| {
            if let Value::String(s) = &key {
                if s.to_str()? == "Count" {
                    let count = items_index.lock().map(|i| i.len()).unwrap_or(0);
                    return Ok(Value::Integer(count as i64));
                }
            }
            if let Value::Integer(idx) = key {
                if let Ok(it) = items_index.lock() {
                    let uidx = idx as usize;
                    if let Some(item) = it.get(uidx) {
                        return Ok(Value::String(lua.create_string(item)?));
                    }
                }
            }
            tbl.raw_get(key)
        })?,
    )?;

    let items_newindex = items.clone();
    meta.set(
        "__newindex",
        lua.create_function(move |_, (tbl, key, val): (Table, Value, Value)| {
            if let Value::Integer(idx) = key {
                if let Ok(mut it) = items_newindex.lock() {
                    let s = match &val {
                        Value::String(st) => st.to_str().map(|s| s.to_string()).unwrap_or_default(),
                        _ => String::new(),
                    };
                    let uidx = idx as usize;
                    if uidx < it.len() {
                        it[uidx] = s;
                    } else {
                        while it.len() < uidx {
                            it.push(String::new());
                        }
                        it.push(s);
                    }
                }
            }
            tbl.raw_set(key, val)?;
            Ok(())
        })?,
    )?;
    table.set_metatable(Some(meta));

    Ok((table, items))
}

fn extract_string_arg(args: &MultiValue, default_idx: usize) -> Option<String> {
    if let Some(Value::String(s)) = args.get(default_idx) {
        return s.to_str().ok().map(|st| st.to_string());
    }
    if let Some(Value::String(s)) = args.get(default_idx + 1) {
        return s.to_str().ok().map(|st| st.to_string());
    }
    None
}

pub fn create_alternate_versions_table(
    lua: &Lua,
) -> LuaResult<(Table, Arc<Mutex<Vec<crate::mangainfo::AlternateVersion>>>)> {
    let items = Arc::new(Mutex::new(Vec::new()));
    let table = lua.create_table()?;
    let items_add = items.clone();
    table.set(
        "Add",
        lua.create_function(move |_, (lang, title, url): (String, String, String)| {
            if let Ok(mut it) = items_add.lock() {
                it.push(crate::mangainfo::AlternateVersion {
                    language: lang,
                    title,
                    url,
                });
            }
            Ok(())
        })?,
    )?;
    Ok((table, items))
}
