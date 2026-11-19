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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_list_add_index_reverse_and_clear() {
        let lua = Lua::new();
        let (tbl, items) = create_string_list_table(&lua).unwrap();
        lua.globals().set("LIST", tbl).unwrap();

        let (count_before, first, second, count_after_clear): (i64, String, String, i64) = lua
            .load(
                r#"
                LIST.Add('alpha')
                LIST.Add('beta')
                LIST[3] = 'delta'
                LIST.Reverse()
                local c1 = LIST.Count
                local i0 = LIST[0]
                local i1 = LIST[1]
                LIST.Clear()
                return c1, i0, i1, LIST.Count
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(count_before, 4); // ['alpha', 'beta', '', 'delta'] -> reversed
        assert_eq!(first, "delta");
        assert_eq!(second, "");
        assert_eq!(count_after_clear, 0);
        assert!(items.lock().unwrap().is_empty());
    }

    #[test]
    fn test_alternate_versions_table() {
        let lua = Lua::new();
        let (tbl, versions) = create_alternate_versions_table(&lua).unwrap();
        lua.globals().set("ALTS", tbl).unwrap();

        lua.load("ALTS.Add('fr', 'One Piece FR', 'https://example.com/fr')")
            .exec()
            .unwrap();

        let guard = versions.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].language, "fr");
        assert_eq!(guard[0].title, "One Piece FR");
        assert_eq!(guard[0].url, "https://example.com/fr");
    }
}
