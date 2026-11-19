use boa_engine::{Context, Source};
use mlua::{Lua, Result as LuaResult, Table};

pub fn register_duktape(lua: &Lua) -> LuaResult<()> {
    let duktape_table = lua.create_table()?;

    let exec_js = lua.create_function(|_, code: String| {
        let mut context = Context::default();
        {
            let limits = context.runtime_limits_mut();
            limits.set_loop_iteration_limit(1_000_000);
            limits.set_recursion_limit(256);
            limits.set_stack_size_limit(8 * 1024);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duktape_exec_js_and_limits() {
        let lua = Lua::new();
        let package: Table = lua.globals().get("package").unwrap();
        let _ = package.get::<Table>("loaded").unwrap();
        register_duktape(&lua).unwrap();

        let (sum, via_require, err_syntax, err_loop): (String, String, String, String) = lua
            .load(
                r#"
                local d = require('duktape')
                return duktape.ExecJS("'a' + 'b' + (2 + 3)"),
                       d.ExecJS("6 * 7"),
                       d.ExecJS("function("),
                       d.ExecJS("while (true) {}")
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(sum, "ab5");
        assert_eq!(via_require, "42");
        assert!(err_syntax.starts_with("Error:"));
        assert!(err_loop.starts_with("Error:"));
    }
}
