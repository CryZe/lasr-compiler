use std::fmt::Write;

use tsuki::{
    Value,
    context::{Args, Context, Ret},
    fp,
};

use crate::{
    state::{Result, State},
    utils::{DisplayValue, next_pair},
};

pub fn print_tbl(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() == 0 || cx.arg(1).as_table().is_none() {
        asr::print_message("[print_tbl] Argument is not a table or no argument passed.");
        return Ok(cx.into());
    }

    if cx.args() > 1 {
        asr::print_message("[print_tbl] Too many arguments passed, only pass a single table");
        return Ok(cx.into());
    }

    let table = cx.arg(1).get_table()?;
    let td = cx.create_thread();

    let mut key = Value::Nil;
    let mut buf = String::new();
    loop {
        let mut pair: Vec<Value<State>> = td.call(fp!(next_pair), (table, &key))?;
        if pair.len() != 2 {
            break;
        }

        let next_value = pair.pop().unwrap();
        let next_key = pair.pop().unwrap();

        let key_text = DisplayValue(&next_key);
        let value_text = DisplayValue(&next_value);

        buf.clear();
        let _ = write!(&mut buf, "{key_text}: {value_text}");
        asr::print_message(&buf);
        key = next_key;
    }

    Ok(cx.into())
}
