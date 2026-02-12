use tsuki::{Value, context::{Args, Context, Ret}};

use crate::state::{Result, State};

pub fn size_of(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let ty_arg = cx.arg(1);
    let ty = ty_arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| ty_arg.error("type is not valid UTF-8"))?;

    let size = match ty {
        "sbyte" | "byte" | "bool" => 1,
        "short" | "ushort" => 2,
        "int" | "uint" | "float" => 4,
        "long" | "ulong" | "double" => 8,
        _ => {
            if let Some(rem) = ty.strip_prefix("string")
                && let Ok(byte_count) = rem.parse::<usize>()
            {
                byte_count
            } else if let Some(rem) = ty.strip_prefix("byte")
                && !rem.is_empty()
                && let Ok(byte_count) = rem.parse::<usize>()
            {
                byte_count
            } else {
                return Err(ty_arg.error("unsupported type"));
            }
        }
    };

    cx.push(Value::Int(size as i64))?;
    Ok(cx.into())
}
