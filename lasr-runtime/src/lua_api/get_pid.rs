use tsuki::context::{Args, Context, Ret};

use crate::state::{Result, State};

pub fn get_pid(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    // FIXME: pid is unavailable in asr, so we return a dummy value for now.
    cx.push(0)?;
    Ok(cx.into())
}
