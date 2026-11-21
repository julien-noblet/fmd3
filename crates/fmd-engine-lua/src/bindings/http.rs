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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lua_http_get_post_headers_reset_and_document() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/ok"))
            .and(header("X-Test", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello-get"))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/not-found"))
            .respond_with(ResponseTemplate::new(404).set_body_string("missing"))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/submit"))
            .and(header("content-type", "application/json"))
            .and(body_string(r#"{"k":1}"#))
            .respond_with(ResponseTemplate::new(201).set_body_string("created"))
            .mount(&server)
            .await;

        let lua = Lua::new();
        let client = HttpClient::new().unwrap();
        register_http(&lua, client).unwrap();

        let base = server.uri();
        let (
            empty_ok,
            get_ok,
            code_ok,
            doc_str,
            doc_concat,
            doc_override,
            not_found_ok,
            code_404,
            post_ok,
            post_code,
            post_body,
        ): (
            bool,
            bool,
            i64,
            String,
            String,
            String,
            bool,
            i64,
            bool,
            i64,
            String,
        ) = lua
            .load(format!(
                r#"
                local empty_ok = HTTP.GET('')
                HTTP.Headers.Values['X-Test'] = 'abc'
                local get_ok = HTTP.GET('{base}/ok')
                local code_ok = HTTP.ResultCode
                local doc_str = HTTP.Document.ToString()
                local doc_concat = HTTP.Document .. '!'
                HTTP.Document.WriteString('override')
                local doc_override = tostring(HTTP.Document)

                HTTP.Reset()
                local not_found_ok = HTTP.GET('{base}/not-found')
                local code_404 = HTTP.ResultCode

                HTTP.MimeType = 'application/json'
                local post_ok = HTTP.POST('{base}/submit', '{{"k":1}}')
                local post_code = HTTP.ResultCode
                local post_body = HTTP.Document.ToString()
                return empty_ok, get_ok, code_ok, doc_str, doc_concat, doc_override,
                       not_found_ok, code_404, post_ok, post_code, post_body
                "#
            ))
            .eval()
            .unwrap();

        assert!(!empty_ok);
        assert!(get_ok);
        assert_eq!(code_ok, 200);
        assert_eq!(doc_str, "hello-get");
        assert_eq!(doc_concat, "hello-get!");
        assert_eq!(doc_override, "override");
        assert!(!not_found_ok);
        assert_eq!(code_404, 404);
        assert!(post_ok);
        assert_eq!(post_code, 201);
        assert_eq!(post_body, "created");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lua_http_public_only_blocks_loopback_with_error() {
        let lua = Lua::new();
        let client = HttpClient::new_public_only().unwrap();
        register_http(&lua, client).unwrap();

        let get_err: mlua::Result<bool> =
            lua.load("return HTTP.GET('http://127.0.0.1:8080/')").eval();
        assert!(get_err.is_err());

        let post_err: mlua::Result<bool> = lua
            .load("return HTTP.POST('http://127.0.0.1:8080/', 'a=1')")
            .eval();
        assert!(post_err.is_err());
    }
}
