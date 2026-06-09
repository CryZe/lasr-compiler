use std::fmt::Write;

use tsuki::{Value, context::{Args, Context, Ret}};

use crate::state::{Result, State};

pub fn str2ida(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() == 0 {
        asr::print_message("[str2ida] One argument is required: the string to translate");
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    }

    let arg = cx.arg(1);
    let Some(input) = arg.as_str(false) else {
        asr::print_message("[str2ida] The first argument must be a string.");
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    };

    let input = input
        .as_utf8()
        .ok_or_else(|| arg.error("string argument is not valid UTF-8"))?;

    let mut output = String::with_capacity(input.len().saturating_mul(3));
    for byte in input.bytes() {
        let _ = write!(output, "{byte:02X} ");
    }

    cx.push(Value::Str(cx.create_str(output.as_str())))?;
    Ok(cx.into())
}