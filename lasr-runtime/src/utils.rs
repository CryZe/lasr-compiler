use std::{fmt, pin::Pin, rc::Rc};

use tsuki::{
    Lua, Ref, Thread, Value,
    context::{Args, Context, Ret},
};

use crate::state::{Result, State};

pub fn next_pair(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let table = cx.arg(1).get_table()?;
    let key = cx.arg(2).get().unwrap_or(Value::Nil);

    // FIXME: Tsuki doesn't expose a direct public table iterator on `Table`, so we must route
    // iteration through `Context::push_next` inside a Rust callback and call it via a thread.
    cx.push_next(table, key)?;
    Ok(cx.into())
}

pub struct DisplayValue<'a, 'b, S>(pub &'a Value<'b, S>);

impl fmt::Display for DisplayValue<'_, '_, State> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Value::Str(v) => fmt::Display::fmt(v.as_utf8().unwrap_or("??"), f),
            Value::Int(v) => fmt::Display::fmt(v, f),
            Value::Float(v) => fmt::Display::fmt(&v.0, f),
            Value::True => f.write_str("true"),
            Value::False => f.write_str("false"),
            Value::Nil => f.write_str("nil"),
            _ => f.write_str("??"),
        }
    }
}

pub async fn call_maybe(lua: &Pin<Rc<Lua<State>>>, td: &Ref<'_, Thread<State>>, name: &str) {
    let func = lua.global().get_str_key(name);
    if let Value::LuaFn(func) = func {
        () = td.async_call(&func, ()).await.unwrap();
    }
}

pub async fn call_maybe_bool(
    lua: &Pin<Rc<Lua<State>>>,
    td: &Ref<'_, Thread<State>>,
    name: &str,
) -> Option<bool> {
    let func = lua.global().get_str_key(name);
    if let Value::LuaFn(func) = func {
        match td.async_call(&func, ()).await.unwrap() {
            Value::True => Some(true),
            Value::False => Some(false),
            _ => None,
        }
    } else {
        None
    }
}
