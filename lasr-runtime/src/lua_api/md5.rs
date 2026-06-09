use std::fs;

use tsuki::{
    Value,
    context::{Args, Context, Ret},
};

use crate::state::{Result, State};

/// Takes a path, reads the file and calculates an MD5 hash of it.
pub fn md5sum(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() != 1 {
        asr::print_message(
            "[md5sum] This function requires the path to a file to calculate the MD5 checksum",
        );
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    }

    let arg = cx.arg(1);
    let Some(file_path_value) = arg.as_str(false) else {
        asr::print_message("[md5sum] The argument must be a string");
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    };

    let file_path = file_path_value
        .as_utf8()
        .ok_or_else(|| arg.error("string argument is not valid UTF-8"))?;

    let bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            asr::print_message(&format!("[md5sum] Error while reading file: {e}"));
            cx.push(Value::Nil)?;
            return Ok(cx.into());
        }
    };

    let digest = md5::compute(bytes);
    let hex_digest = format!("{:x}", digest);

    cx.push(Value::Str(cx.create_str(hex_digest.as_str())))?;
    Ok(cx.into())
}
