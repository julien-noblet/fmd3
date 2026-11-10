use mlua::{Lua, MultiValue, ObjectLike, Result as LuaResult, Table, Value};
use regex::Regex;
use scraper::{Html, Selector};
use skyscraper::xpath::grammar::data_model::{AnyAtomicType, XpathItem};
use skyscraper::xpath::grammar::XpathItemTree;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct XPathNode {
    pub attributes: HashMap<String, String>,
    pub text: String,
    pub inner_html: String,
    pub json_value: Option<serde_json::Value>,
}

pub fn create_xpath_node_table(lua: &Lua, node: XPathNode) -> LuaResult<Table> {
    let table = lua.create_table()?;
    let node_rc = Arc::new(node);

    let n = node_rc.clone();
    table.set(
        "GetAttribute",
        lua.create_function(move |_, args: MultiValue| {
            let attr = extract_string_arg(&args, 0).unwrap_or_default();
            let attr_clean = attr.trim_start_matches('@');
            Ok(n.attributes.get(attr_clean).cloned().unwrap_or_default())
        })?,
    )?;

    let n = node_rc.clone();
    table.set(
        "GetProperty",
        lua.create_function(move |lua, args: MultiValue| {
            let prop = extract_string_arg(&args, 0).unwrap_or_default();
            if let Some(ref jv) = n.json_value {
                if let Some(v) = jv.get(&prop) {
                    let sub_node = XPathNode {
                        attributes: HashMap::new(),
                        text: match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        },
                        inner_html: v.to_string(),
                        json_value: Some(v.clone()),
                    };
                    return Ok(Value::Table(create_xpath_node_table(lua, sub_node)?));
                }
            }
            let attr_val = n.attributes.get(&prop).cloned().unwrap_or_default();
            let sub_node = XPathNode {
                attributes: HashMap::new(),
                text: attr_val.clone(),
                inner_html: attr_val,
                json_value: None,
            };
            Ok(Value::Table(create_xpath_node_table(lua, sub_node)?))
        })?,
    )?;

    let n = node_rc.clone();
    table.set(
        "Get",
        lua.create_function(move |lua, args: MultiValue| {
            if let Some(ref jv) = n.json_value {
                if let Some(arr) = jv.as_array() {
                    let idx = match args.front() {
                        Some(Value::Integer(i)) => *i as usize,
                        _ => match args.get(1) {
                            Some(Value::Integer(i)) => *i as usize,
                            _ => 0,
                        },
                    };
                    let zero_idx = if idx > 0 && idx <= arr.len() {
                        idx - 1
                    } else {
                        idx
                    };
                    if let Some(elem) = arr.get(zero_idx) {
                        let sub_node = XPathNode {
                            attributes: HashMap::new(),
                            text: match elem {
                                serde_json::Value::String(s) => s.clone(),
                                _ => elem.to_string(),
                            },
                            inner_html: elem.to_string(),
                            json_value: Some(elem.clone()),
                        };
                        return Ok(Value::Table(create_xpath_node_table(lua, sub_node)?));
                    }
                }
            }
            Ok(Value::Nil)
        })?,
    )?;

    let n = node_rc.clone();
    table.set(
        "ToString",
        lua.create_function(move |_, _: MultiValue| Ok(n.text.clone()))?,
    )?;

    let n = node_rc.clone();
    table.set(
        "InnerText",
        lua.create_function(move |_, _: MultiValue| Ok(n.text.clone()))?,
    )?;

    let n = node_rc.clone();
    table.set(
        "GetText",
        lua.create_function(move |_, _: MultiValue| Ok(n.text.clone()))?,
    )?;

    let n = node_rc.clone();
    table.set("_node_html", n.inner_html.clone())?;
    table.set("_node_text", n.text.clone())?;

    let n = node_rc.clone();
    table.set(
        "InnerHTML",
        lua.create_function(move |_, _: MultiValue| Ok(n.inner_html.clone()))?,
    )?;

    let n = node_rc.clone();
    let meta = lua.create_table()?;
    meta.set(
        "__tostring",
        lua.create_function(move |_, _: MultiValue| Ok(n.text.clone()))?,
    )?;
    table.set_metatable(Some(meta));

    Ok(table)
}

pub fn create_xpath_nodelist_table(lua: &Lua, nodes: Vec<XPathNode>) -> LuaResult<Table> {
    let table = lua.create_table()?;
    let nodes_rc = Arc::new(nodes);

    let n = nodes_rc.clone();
    table.set(
        "Get",
        lua.create_function(move |lua, args: MultiValue| {
            // Check if Get(index) was called
            if let Some(Value::Integer(idx)) = args.front().or_else(|| args.get(1)) {
                let uidx = *idx as usize;
                let zero_idx = if uidx > 0 && uidx <= n.len() {
                    uidx - 1
                } else {
                    uidx
                };
                if let Some(node) = n.get(zero_idx) {
                    return Ok(Value::Table(create_xpath_node_table(lua, node.clone())?));
                }
                return Ok(Value::Nil);
            }

            // Iterator for `for node in x.XPath(...).Get() do`
            let nodes = n.clone();
            let mut i = 0;
            let iter = lua.create_function_mut(move |lua, _: MultiValue| {
                if i < nodes.len() {
                    let node = nodes[i].clone();
                    i += 1;
                    Ok(Value::Table(create_xpath_node_table(lua, node)?))
                } else {
                    Ok(Value::Nil)
                }
            })?;
            Ok(Value::Function(iter))
        })?,
    )?;

    // Metatable for indexing list[0] or list[i] and Count property
    let n_meta = nodes_rc.clone();
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (tbl, key): (Table, Value)| {
            if let Value::String(s) = &key {
                if s.to_str()? == "Count" {
                    return Ok(Value::Integer(n_meta.len() as i64));
                }
            }
            if let Value::Integer(idx) = key {
                let uidx = idx as usize;
                let zero_idx = if uidx > 0 && uidx <= n_meta.len() {
                    uidx - 1
                } else {
                    uidx
                };
                if let Some(node) = n_meta.get(zero_idx) {
                    return Ok(Value::Table(create_xpath_node_table(lua, node.clone())?));
                }
            }
            tbl.raw_get(key)
        })?,
    )?;
    table.set_metatable(Some(meta));

    Ok(table)
}

pub struct TXQueryState {
    pub html: String,
}

fn extract_context_html(args: &MultiValue, fallback_html: &str) -> String {
    for arg in args.iter() {
        if let Value::Table(tbl) = arg {
            if let Ok(h) = tbl.get::<String>("_node_html") {
                if !h.is_empty() {
                    return h;
                }
            }
        }
    }
    fallback_html.to_string()
}

pub fn create_txquery_table(lua: &Lua, html_content: String) -> LuaResult<Table> {
    let state = Arc::new(Mutex::new(TXQueryState { html: html_content }));
    let table = lua.create_table()?;

    // ParseHTML
    let state_parse = state.clone();
    table.set(
        "ParseHTML",
        lua.create_function(move |_, args: MultiValue| {
            let html_str = extract_string_arg(&args, 0).unwrap_or_default();
            if let Ok(mut st) = state_parse.lock() {
                st.html = html_str;
            }
            Ok(())
        })?,
    )?;

    // XPath
    let state_xpath = state.clone();
    table.set(
        "XPath",
        lua.create_function(move |lua, args: MultiValue| {
            let query = extract_string_arg(&args, 0).unwrap_or_default();
            let fallback = state_xpath
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let html = extract_context_html(&args, &fallback);
            let nodes = eval_xpath_nodes(&html, &query);
            create_xpath_nodelist_table(lua, nodes)
        })?,
    )?;

    // XPathString
    let state_xpath_str = state.clone();
    table.set(
        "XPathString",
        lua.create_function(move |_lua, args: MultiValue| {
            let query = extract_string_arg(&args, 0).unwrap_or_default();
            let fallback = state_xpath_str
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let html = extract_context_html(&args, &fallback);
            Ok(eval_xpath_string(&html, &query))
        })?,
    )?;

    // XPathCount
    let state_count = state.clone();
    table.set(
        "XPathCount",
        lua.create_function(move |_lua, args: MultiValue| {
            let query = extract_string_arg(&args, 0).unwrap_or_default();
            let fallback = state_count
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let html = extract_context_html(&args, &fallback);
            let count = eval_xpath_count(&html, &query);
            Ok(count as i64)
        })?,
    )?;

    // XPathStringAll
    let state_str_all = state.clone();
    table.set(
        "XPathStringAll",
        lua.create_function(move |lua, args: MultiValue| {
            let query = extract_string_arg(&args, 0).unwrap_or_default();
            let mut target_val = None;
            for arg in args.iter() {
                if let Value::Table(tbl) = arg {
                    if tbl.get::<String>("_node_html").is_err() {
                        target_val = Some(arg);
                        break;
                    }
                } else if let Value::UserData(_) = arg {
                    target_val = Some(arg);
                    break;
                }
            }

            let fallback = state_str_all
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let html = extract_context_html(&args, &fallback);
            let results = eval_xpath_string_all(&html, &query);

            if let Some(target) = target_val {
                for (idx, val) in results.iter().enumerate() {
                    insert_string_value(target, idx, val.clone());
                }
                Ok(Value::Nil)
            } else {
                Ok(Value::String(lua.create_string(results.join(", "))?))
            }
        })?,
    )?;

    // XPathHREFAll
    let state_href_all = state.clone();
    table.set(
        "XPathHREFAll",
        lua.create_function(move |_lua, args: MultiValue| {
            let (query, links_val, names_val) = extract_href_args(&args);
            let html = state_href_all
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let pairs = eval_xpath_href_all(&html, &query);

            for (idx, (href, text)) in pairs.iter().enumerate() {
                if let Some(links) = &links_val {
                    insert_string_value(links, idx, href.clone());
                }
                if let Some(names) = &names_val {
                    insert_string_value(names, idx, text.clone());
                }
            }
            Ok(())
        })?,
    )?;

    // XPathHREFTitleAll
    let state_title_all = state.clone();
    table.set(
        "XPathHREFTitleAll",
        lua.create_function(move |_lua, args: MultiValue| {
            let (query, links_val, names_val) = extract_href_args(&args);
            let html = state_title_all
                .lock()
                .map(|st| st.html.clone())
                .unwrap_or_default();
            let pairs = eval_xpath_href_title_all(&html, &query);

            for (idx, (href, title)) in pairs.iter().enumerate() {
                if let Some(links) = &links_val {
                    insert_string_value(links, idx, href.clone());
                }
                if let Some(names) = &names_val {
                    insert_string_value(names, idx, title.clone());
                }
            }
            Ok(())
        })?,
    )?;

    Ok(table)
}

// ==============================================================================
// Evaluator functions: Skyscraper XPath + JSON Navigation + Scraper CSS Fallback
// ==============================================================================

pub fn eval_xpath_nodes(html: &str, query: &str) -> Vec<XPathNode> {
    if query.starts_with("json(") {
        return eval_json_nodes(html, query);
    }

    // 1. Try CSS Scraper first to preserve outer HTML and attributes on element nodes
    let css_selector = convert_xpath_to_css(query);
    if let Ok(sel) = Selector::parse(&css_selector) {
        let document = Html::parse_document(html);
        let mut nodes = Vec::new();
        for el in document.select(&sel) {
            let mut attributes = HashMap::new();
            for (name, val) in el.value().attrs() {
                attributes.insert(name.to_string(), val.to_string());
            }
            let text: String = el.text().collect();
            let outer_html = el.html();

            nodes.push(XPathNode {
                attributes,
                text: text.trim().to_string(),
                inner_html: outer_html,
                json_value: None,
            });
        }
        if !nodes.is_empty() {
            return nodes;
        }
    }

    // 2. Fallback to Skyscraper XPath engine
    if let Ok(tree) = skyscraper::html::parse(html) {
        if let Ok(xpath) = skyscraper::xpath::parse(query) {
            if let Ok(items) = xpath.apply(&tree) {
                let mut nodes = Vec::new();
                for item in items.iter() {
                    let text = extract_xpath_item_text(item, &tree);
                    let attributes = extract_xpath_item_attrs(item, &tree);
                    nodes.push(XPathNode {
                        attributes,
                        text: text.clone(),
                        inner_html: text,
                        json_value: None,
                    });
                }
                if !nodes.is_empty() {
                    return nodes;
                }
            }
        }
    }

    Vec::new()
}

pub fn eval_xpath_string(html: &str, query: &str) -> String {
    if query.starts_with("json(") {
        return eval_json_string(html, query);
    }

    // 1. Try Skyscraper XPath engine
    if let Ok(tree) = skyscraper::html::parse(html) {
        if let Ok(xpath) = skyscraper::xpath::parse(query) {
            if let Ok(items) = xpath.apply(&tree) {
                if let Some(item) = items.iter().next() {
                    let text = extract_xpath_item_text(item, &tree);
                    if !text.is_empty() {
                        return text;
                    }
                }
            }
        }
        // If relative XPath, also try with // prefix
        if !query.starts_with('/') {
            if let Ok(xpath) = skyscraper::xpath::parse(&format!("//{}", query)) {
                if let Ok(items) = xpath.apply(&tree) {
                    if let Some(item) = items.iter().next() {
                        let text = extract_xpath_item_text(item, &tree);
                        if !text.is_empty() {
                            return text;
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to CSS Scraper
    let re_attr = Regex::new(r"/@([a-zA-Z0-9_-]+)\s*$").unwrap();
    let attr_name = re_attr
        .captures(query)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let css_selector = convert_xpath_to_css(query);
    if let Ok(sel) = Selector::parse(&css_selector) {
        let document = Html::parse_document(html);
        if let Some(el) = document.select(&sel).next() {
            if let Some(attr) = attr_name {
                if let Some(val) = el.value().attr(&attr) {
                    return val.to_string();
                }
            }
            let text: String = el.text().collect();
            return text.trim().to_string();
        }
    }

    String::new()
}

pub fn eval_xpath_count(html: &str, query: &str) -> usize {
    eval_xpath_nodes(html, query).len()
}

pub fn eval_xpath_string_all(html: &str, query: &str) -> Vec<String> {
    if query.starts_with("json(") {
        return eval_json_string_all(html, query);
    }

    // 1. Try Skyscraper XPath
    if let Ok(tree) = skyscraper::html::parse(html) {
        if let Ok(xpath) = skyscraper::xpath::parse(query) {
            if let Ok(items) = xpath.apply(&tree) {
                let mut results = Vec::new();
                for item in items.iter() {
                    let text = extract_xpath_item_text(item, &tree);
                    if !text.is_empty() {
                        results.push(text);
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }
    }

    // 2. Fallback to CSS Scraper
    let re_attr = Regex::new(r"/@([a-zA-Z0-9_-]+)\s*$").unwrap();
    let attr_name = re_attr
        .captures(query)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());
    let css_selector = convert_xpath_to_css(query);
    let mut results = Vec::new();

    if let Ok(sel) = Selector::parse(&css_selector) {
        let document = Html::parse_document(html);
        for el in document.select(&sel) {
            let val = if let Some(ref attr) = attr_name {
                el.value().attr(attr).unwrap_or_default().to_string()
            } else {
                let text: String = el.text().collect();
                text.trim().to_string()
            };
            if !val.is_empty() {
                results.push(val);
            }
        }
    }
    results
}

pub fn eval_xpath_href_all(html: &str, query: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    // 1. Try Skyscraper XPath
    if let Ok(tree) = skyscraper::html::parse(html) {
        if let Ok(xpath) = skyscraper::xpath::parse(query) {
            if let Ok(items) = xpath.apply(&tree) {
                for item in items.iter() {
                    if let Ok(node) = item.as_node() {
                        if let Ok(elem) = node.as_element_node() {
                            let href = elem
                                .get_attribute(&tree, "href")
                                .unwrap_or_default()
                                .to_string();
                            let text = elem.text_content(&tree).trim().to_string();
                            pairs.push((href, text));
                        }
                    }
                }
                if !pairs.is_empty() {
                    return pairs;
                }
            }
        }
    }

    // 2. Fallback to CSS Scraper
    let css_selector = convert_xpath_to_css(query);
    if let Ok(sel) = Selector::parse(&css_selector) {
        let document = Html::parse_document(html);
        for el in document.select(&sel) {
            let href = el.value().attr("href").unwrap_or_default().to_string();
            let text: String = el.text().collect();
            pairs.push((href, text.trim().to_string()));
        }
    }
    pairs
}

pub fn eval_xpath_href_title_all(html: &str, query: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    // 1. Try Skyscraper XPath
    if let Ok(tree) = skyscraper::html::parse(html) {
        if let Ok(xpath) = skyscraper::xpath::parse(query) {
            if let Ok(items) = xpath.apply(&tree) {
                for item in items.iter() {
                    if let Ok(node) = item.as_node() {
                        if let Ok(elem) = node.as_element_node() {
                            let href = elem
                                .get_attribute(&tree, "href")
                                .unwrap_or_default()
                                .to_string();
                            let title = elem
                                .get_attribute(&tree, "title")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| elem.text_content(&tree).trim().to_string());
                            pairs.push((href, title));
                        }
                    }
                }
                if !pairs.is_empty() {
                    return pairs;
                }
            }
        }
    }

    // 2. Fallback to CSS Scraper
    let css_selector = convert_xpath_to_css(query);
    if let Ok(sel) = Selector::parse(&css_selector) {
        let document = Html::parse_document(html);
        for el in document.select(&sel) {
            let href = el.value().attr("href").unwrap_or_default().to_string();
            let title = el
                .value()
                .attr("title")
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let t: String = el.text().collect();
                    t.trim().to_string()
                });
            pairs.push((href, title));
        }
    }
    pairs
}

fn extract_xpath_item_text(item: &XpathItem<'_>, tree: &XpathItemTree) -> String {
    match item {
        XpathItem::Node(node) => {
            if let Ok(attr) = node.as_attribute_node() {
                attr.value.clone()
            } else if let Ok(text_node) = node.as_text_node() {
                text_node.content.clone()
            } else if let Ok(elem) = node.as_element_node() {
                elem.text_content(tree).trim().to_string()
            } else {
                node.text_content(tree).trim().to_string()
            }
        }
        XpathItem::AnyAtomicType(atom) => match atom {
            AnyAtomicType::String(s) => s.clone(),
            AnyAtomicType::Integer(i) => i.to_string(),
            AnyAtomicType::Boolean(b) => b.to_string(),
            AnyAtomicType::Float(f) => f.to_string(),
            AnyAtomicType::Double(d) => d.to_string(),
            AnyAtomicType::QName { local_name, .. } => local_name.clone(),
        },
        _ => String::new(),
    }
}

fn extract_xpath_item_attrs(item: &XpathItem<'_>, tree: &XpathItemTree) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    if let XpathItem::Node(node) = item {
        if let Ok(elem) = node.as_element_node() {
            for attr in elem.attributes(tree) {
                attrs.insert(attr.name.clone(), attr.value.clone());
            }
        } else if let Ok(attr) = node.as_attribute_node() {
            attrs.insert(attr.name.clone(), attr.value.clone());
        }
    }
    attrs
}

// ==============================================================================
// JSON expressions handling: json(*).prop, json(//script).data.items()
// ==============================================================================

fn parse_json_root(html: &str, query: &str) -> Option<(serde_json::Value, String)> {
    if !query.starts_with("json(") {
        return None;
    }

    // Find closing parenthesis of json(...)
    let mut depth = 0;
    let mut close_idx = None;
    for (idx, ch) in query.char_indices() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                close_idx = Some(idx);
                break;
            }
        }
    }

    let close_idx = close_idx?;
    let inner_expr = query[5..close_idx].trim();
    let remainder = query[close_idx + 1..].trim_start_matches('.');

    let json_text = if inner_expr == "*" {
        html.to_string()
    } else {
        // Evaluate inner xpath on html
        let extracted = eval_xpath_string(html, inner_expr);
        if extracted.is_empty() {
            html.to_string()
        } else {
            extracted
        }
    };

    let parsed: serde_json::Value = serde_json::from_str(&json_text).ok()?;
    Some((parsed, remainder.to_string()))
}

fn navigate_json<'a>(val: &'a serde_json::Value, path: &str) -> Vec<&'a serde_json::Value> {
    if path.is_empty() || path == "()" {
        if let Some(arr) = val.as_array() {
            return arr.iter().collect();
        }
        return vec![val];
    }

    let mut current = vec![val];
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();

    for part in parts {
        let mut next = Vec::new();
        let clean_part = part.trim_end_matches("()");

        let (field_name, index) = if let Some(open) = clean_part.find('[') {
            if let Some(close) = clean_part.find(']') {
                let name = clean_part[..open].trim_end_matches("()");
                let idx: Option<usize> = clean_part[open + 1..close].parse().ok();
                (name, idx)
            } else {
                (clean_part, None)
            }
        } else {
            (clean_part, None)
        };

        for item in current {
            let target_val = if field_name.is_empty() || field_name == "*" {
                Some(item)
            } else {
                item.get(field_name)
            };

            if let Some(v) = target_val {
                if let Some(idx) = index {
                    if let Some(arr) = v.as_array() {
                        let zero_idx = if idx > 0 && idx <= arr.len() {
                            idx - 1
                        } else {
                            idx
                        };
                        if let Some(el) = arr.get(zero_idx) {
                            next.push(el);
                        }
                    }
                } else if let Some(arr) = v.as_array() {
                    for el in arr {
                        next.push(el);
                    }
                } else {
                    next.push(v);
                }
            }
        }
        current = next;
    }
    current
}

fn eval_json_nodes(html: &str, query: &str) -> Vec<XPathNode> {
    if let Some((json, path)) = parse_json_root(html, query) {
        let items = navigate_json(&json, &path);
        return items
            .into_iter()
            .map(|v| XPathNode {
                attributes: HashMap::new(),
                text: match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                },
                inner_html: v.to_string(),
                json_value: Some(v.clone()),
            })
            .collect();
    }
    Vec::new()
}

fn eval_json_string(html: &str, query: &str) -> String {
    if let Some((json, path)) = parse_json_root(html, query) {
        let items = navigate_json(&json, &path);
        if let Some(v) = items.first() {
            return match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => v.to_string(),
            };
        }
    }
    String::new()
}

fn eval_json_string_all(html: &str, query: &str) -> Vec<String> {
    if let Some((json, path)) = parse_json_root(html, query) {
        let items = navigate_json(&json, &path);
        return items
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => v.to_string(),
            })
            .collect();
    }
    Vec::new()
}

// ==============================================================================
// Helper functions
// ==============================================================================

fn extract_string_arg(args: &MultiValue, default_idx: usize) -> Option<String> {
    match args.front() {
        Some(Value::Table(_)) | Some(Value::UserData(_)) => {
            if let Some(Value::String(s)) = args.get(default_idx + 1) {
                return s.to_str().ok().map(|st| st.to_string());
            }
        }
        _ => {
            if let Some(Value::String(s)) = args.get(default_idx) {
                return s.to_str().ok().map(|st| st.to_string());
            }
        }
    }
    None
}

fn extract_href_args(args: &MultiValue) -> (String, Option<Value>, Option<Value>) {
    let (q_idx, l_idx, n_idx) = match args.front() {
        Some(Value::Table(_)) | Some(Value::UserData(_)) => (1, 2, 3),
        _ => (0, 1, 2),
    };

    let q = match args.get(q_idx) {
        Some(Value::String(s)) => s.to_str().map(|st| st.to_string()).unwrap_or_default(),
        _ => String::new(),
    };
    (q, args.get(l_idx).cloned(), args.get(n_idx).cloned())
}

fn insert_string_value(target: &Value, idx: usize, val: String) {
    match target {
        Value::Table(t) => {
            if let Ok(add_fn) = t.get::<mlua::Function>("Add") {
                let _ = add_fn.call::<()>(val.clone());
            }
            let _ = t.raw_set(idx, val);
        }
        Value::UserData(ud) => {
            if let Ok(method) = ud.get::<mlua::Function>("Add") {
                let _ = method.call::<()>(val);
            }
        }
        _ => {}
    }
}

fn convert_xpath_to_css(xpath: &str) -> String {
    let mut s = xpath.trim().to_string();
    if s.starts_with(".//") {
        s = s[3..].to_string();
    } else if s.starts_with("./") {
        s = s[2..].to_string();
    }
    if let Some(pos) = s.rfind("/@") {
        s = s[..pos].to_string();
    }
    if let Some(pos) = s.rfind("/text()") {
        s = s[..pos].to_string();
    }

    // Convert xpath [last()] to :last-of-type, [1] to :first-of-type, [n] to :nth-of-type(n)
    let re_last = Regex::new(r"(\([^\)]+\)|[a-zA-Z0-9_-]+)\[last\(\)\]").unwrap();
    s = re_last.replace_all(&s, "$1:last-of-type").to_string();
    let re_first = Regex::new(r"(\([^\)]+\)|[a-zA-Z0-9_-]+)\[1\]").unwrap();
    s = re_first.replace_all(&s, "$1:first-of-type").to_string();
    let re_nth = Regex::new(r"(\([^\)]+\)|[a-zA-Z0-9_-]+)\[(\d+)\]").unwrap();
    s = re_nth.replace_all(&s, "$1:nth-of-type($2)").to_string();

    s = s.replace("//", " ");
    let re_class = Regex::new(r#"\[@class="([^"]+)"\]"#).unwrap();
    s = re_class.replace_all(&s, ".$1").to_string();
    let re_id = Regex::new(r#"\[@id="([^"]+)"\]"#).unwrap();
    s = re_id.replace_all(&s, "#$1").to_string();
    let re_contains = Regex::new(r#"\[contains\(@class,\s*"([^"]+)"\)\]"#).unwrap();
    s = re_contains.replace_all(&s, ".$1").to_string();
    s = s.replace('/', " ");
    s = s.replace(['(', ')'], "");
    s.trim().to_string()
}

pub fn register_xquery(lua: &Lua) -> LuaResult<()> {
    let create_txquery = lua.create_function(|lua, doc: Option<Value>| {
        let content = match doc {
            Some(Value::String(s)) => s.to_str().map(|b| b.to_string()).unwrap_or_default(),
            Some(Value::Table(t)) => {
                if let Ok(to_str) = t.get::<mlua::Function>("ToString") {
                    to_str.call::<String>(()).unwrap_or_default()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };
        create_txquery_table(lua, content)
    })?;

    lua.globals().set("CreateTXQuery", create_txquery)?;
    Ok(())
}
