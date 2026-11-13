use fmd_net::HttpClient;
use mlua::{Lua, MultiValue, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn register_http(lua: &Lua, client: HttpClient) -> LuaResult<()> {
    let http_table = lua.create_table()?;
    let raw_bytes = Arc::new(Mutex::new(Vec::<u8>::new()));
    let result_code = Arc::new(Mutex::new(200u16));
    let client = Arc::new(client);

    // Headers table
    let headers_table = lua.create_table()?;
    let values_table = lua.create_table()?;
    headers_table.set("Values", values_table.clone())?;
    http_table.set("Headers", headers_table.clone())?;

    // Cookies table
    let cookies_table = lua.create_table()?;
    http_table.set("Cookies", cookies_table)?;

    // MimeType string
    http_table.set("MimeType", "application/x-www-form-urlencoded")?;

    // Reset helper
    let headers_reset = headers_table.clone();
    http_table.set(
        "Reset",
        lua.create_function(move |lua, ()| {
            let values_tbl = lua.create_table()?;
            headers_reset.set("Values", values_tbl)?;
            Ok(())
        })?,
    )?;

    // GET helper
    let client_get = client.clone();
    let bytes_get = raw_bytes.clone();
    let code_get = result_code.clone();
    let headers_ref = headers_table.clone();
    let get_fn = lua.create_function(move |_, url: Option<String>| {
        let url = match url {
            Some(u) if !u.is_empty() => u,
            _ => return Ok(false),
        };

        // Extract custom headers from HTTP.Headers.Values
        let mut custom_headers = HashMap::new();
        if let Ok(values_tbl) = headers_ref.get::<Table>("Values") {
            for (k, v) in values_tbl.pairs::<String, String>().flatten() {
                custom_headers.insert(k, v);
            }
        }

        let client = client_get.clone();
        let public_only = client.is_public_only();
        let bytes_clone = bytes_get.clone();
        let code_clone = code_get.clone();

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| fmd_net::NetError::Other(e.to_string()))?;
            rt.block_on(async move {
                client
                    .get_bytes_and_status_with_headers(&url, &custom_headers)
                    .await
            })
        });

        match handle.join() {
            Ok(Ok((status, body_bytes))) => {
                if let Ok(mut bytes) = bytes_clone.lock() {
                    *bytes = body_bytes;
                }
                if let Ok(mut rc) = code_clone.lock() {
                    *rc = status;
                }
                Ok(status < 400)
            }
            result if public_only => Err(mlua::Error::RuntimeError(format!(
                "Scraper HTTP GET failed: {}",
                match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "network worker panicked".into(),
                    Ok(Ok(_)) => unreachable!(),
                }
            ))),
            _ => {
                if let Ok(mut rc) = code_clone.lock() {
                    *rc = 500;
                }
                Ok(false)
            }
        }
    })?;
    http_table.set("GET", get_fn)?;

    // POST helper
    let client_post = client.clone();
    let bytes_post = raw_bytes.clone();
    let code_post = result_code.clone();
    let headers_post_ref = headers_table.clone();
    let http_table_ref = http_table.clone();
    let post_fn = lua.create_function(move |_, (url, body): (String, String)| {
        let mut custom_headers = HashMap::new();
        if let Ok(values_tbl) = headers_post_ref.get::<Table>("Values") {
            for (k, v) in values_tbl.pairs::<String, String>().flatten() {
                custom_headers.insert(k, v);
            }
        }

        let mime: Option<String> = http_table_ref.get("MimeType").ok();
        let client = client_post.clone();
        let public_only = client.is_public_only();
        let bytes_clone = bytes_post.clone();
        let code_clone = code_post.clone();

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| fmd_net::NetError::Other(e.to_string()))?;
            rt.block_on(async move {
                client
                    .post_text_with_headers(&url, &body, mime.as_deref(), &custom_headers)
                    .await
            })
        });

        match handle.join() {
            Ok(Ok((status, resp))) => {
                if let Ok(mut bytes) = bytes_clone.lock() {
                    *bytes = resp.into_bytes();
                }
                if let Ok(mut rc) = code_clone.lock() {
                    *rc = status;
                }
                Ok(status < 400)
            }
            result if public_only => Err(mlua::Error::RuntimeError(format!(
                "Scraper HTTP POST failed: {}",
                match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "network worker panicked".into(),
                    Ok(Ok(_)) => unreachable!(),
                }
            ))),
            _ => {
                if let Ok(mut rc) = code_clone.lock() {
                    *rc = 500;
                }
                Ok(false)
            }
        }
    })?;
    http_table.set("POST", post_fn)?;

    // Metatable for HTTP: HTTP.Document and HTTP.ResultCode
    let meta = lua.create_table()?;
    let bytes_meta = raw_bytes.clone();
    let code_meta = result_code.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            if key == "Document" {
                let bytes = bytes_meta.lock().map(|d| d.clone()).unwrap_or_default();
                let doc_tbl = lua.create_table()?;
                let b1 = bytes.clone();
                doc_tbl.set(
                    "ToString",
                    lua.create_function(move |lua, _: MultiValue| lua.create_string(&b1))?,
                )?;
                let b_write = bytes_meta.clone();
                doc_tbl.set(
                    "WriteString",
                    lua.create_function(move |_, s: mlua::String| {
                        if let Ok(mut b) = b_write.lock() {
                            *b = s.as_bytes().to_vec();
                        }
                        Ok(())
                    })?,
                )?;
                let meta_doc = lua.create_table()?;
                let b2 = bytes.clone();
                meta_doc.set(
                    "__tostring",
                    lua.create_function(move |lua, _: MultiValue| lua.create_string(&b2))?,
                )?;
                let b3 = bytes.clone();
                meta_doc.set(
                    "__concat",
                    lua.create_function(move |lua, (a, b): (Value, Value)| {
                        let sa = match a {
                            Value::String(s) => s.as_bytes().to_vec(),
                            _ => b3.clone(),
                        };
                        let sb = match b {
                            Value::String(s) => s.as_bytes().to_vec(),
                            _ => b3.clone(),
                        };
                        let mut joined = sa;
                        joined.extend_from_slice(&sb);
                        lua.create_string(&joined)
                    })?,
                )?;
                doc_tbl.set_metatable(Some(meta_doc));
                Ok(Value::Table(doc_tbl))
            } else if key == "ResultCode" {
                let code = code_meta.lock().map(|c| *c as i64).unwrap_or(200);
                Ok(Value::Integer(code))
            } else {
                _table.raw_get(key)
            }
        })?,
    )?;

    http_table.set_metatable(Some(meta));
    lua.globals().set("HTTP", http_table)?;

    Ok(())
}
