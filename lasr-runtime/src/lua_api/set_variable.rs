use asr::timer;
use tsuki::context::{Args, Context, Ret};

use crate::state::{Result, State};

pub fn set_variable(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let key = cx.arg(1);
    let key = key
        .to_str()?
        .as_utf8()
        .ok_or_else(|| key.error("key is not valid UTF-8"))?;

    let value = cx.arg(2);
    let value = value
        .to_str()?
        .as_utf8()
        .ok_or_else(|| value.error("value is not valid UTF-8"))?;

    timer::set_variable(key, value);

    Ok(cx.into())
}
