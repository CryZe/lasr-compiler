use std::string::String;

use tsuki::context::{Args, Context, Ret};

use crate::state::{Result, State};

pub fn print(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let mut output = match cx.args() {
        0 => {
            asr::print_message("");

            return Ok(cx.into());
        }
        1 => {
            let arg = cx.arg(1);
            let v = arg.display()?;
            asr::print_message(
                v.as_utf8()
                    .ok_or_else(|| arg.error("value is not valid UTF-8"))?,
            );

            return Ok(cx.into());
        }
        n => String::with_capacity(n * 8),
    };

    for i in 1..=cx.args() {
        if i > 1 {
            output.push('\t');
        }
        let arg = cx.arg(i);
        output.push_str(
            arg.display()?
                .as_utf8()
                .ok_or_else(|| arg.error("value is not valid UTF-8"))?,
        );
    }

    asr::print_message(&output);

    Ok(cx.into())
}
