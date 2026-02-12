use tsuki::{
    Value,
    context::{Args, Context, Ret},
    fp,
};

use crate::{
    state::{Result, State},
    utils::next_pair,
};

pub fn shallow_copy_tbl(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() == 0 || cx.arg(1).as_table().is_none() {
        asr::print_message("[shallow_copy_tbl] Argument is not a table or no argument passed.");
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    }

    if cx.args() > 1 {
        asr::print_message(
            "[shallow_copy_tbl] Too many arguments passed, only pass a single table",
        );
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    }

    let source = cx.arg(1).get_table()?;
    let out = cx.create_table();
    let td = cx.create_thread();

    let mut key = Value::Nil;
    loop {
        let mut pair: Vec<Value<State>> = td.call(fp!(next_pair), (source, &key))?;
        if pair.len() != 2 {
            break;
        }

        let next_value = pair.pop().unwrap();
        let next_key = pair.pop().unwrap();
        out.set(&next_key, &next_value)?;
        key = next_key;
    }

    cx.push(Value::Table(out))?;
    Ok(cx.into())
}
