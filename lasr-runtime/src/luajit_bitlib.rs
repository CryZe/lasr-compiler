use tsuki::{
    Lua, Module, Ref, Table, Value,
    context::{Args, Context, Ret},
    fp,
};

use crate::state::{Result, State};

pub struct LuaJitBitLib;

impl Module<State> for LuaJitBitLib {
    const NAME: &str = "bit";

    type Inst<'a>
        = Ref<'a, Table<State>>
    where
        State: 'a;

    fn open(self, lua: &Lua<State>) -> Result<Self::Inst<'_>> {
        let m = lua.create_table();

        m.set_str_key("tobit", fp!(tobit));
        m.set_str_key("band", fp!(band));
        m.set_str_key("bor", fp!(bor));
        m.set_str_key("bxor", fp!(bxor));
        m.set_str_key("bnot", fp!(bnot));
        m.set_str_key("lshift", fp!(lshift));
        m.set_str_key("rshift", fp!(rshift));
        m.set_str_key("arshift", fp!(arshift));
        m.set_str_key("rol", fp!(rol));
        m.set_str_key("ror", fp!(ror));
        m.set_str_key("tohex", fp!(tohex));
        m.set_str_key("bswap", fp!(bswap));

        Ok(m)
    }
}

fn arg_i32(cx: &Context<State, Args>, index: usize) -> Result<i32> {
    Ok(cx.arg(index).to_int()? as i32)
}

fn arg_shift(cx: &Context<State, Args>, index: usize) -> Result<u32> {
    Ok((cx.arg(index).to_int()? as u32) & 31)
}

pub fn tobit(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)?;
    cx.push(x as i64)?;
    Ok(cx.into())
}

pub fn band(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let mut value: u32 = u32::MAX;
    for i in 1..=cx.args() {
        value &= arg_i32(&cx, i)? as u32;
    }
    cx.push(value as i32 as i64)?;
    Ok(cx.into())
}

pub fn bor(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let mut value: u32 = 0;
    for i in 1..=cx.args() {
        value |= arg_i32(&cx, i)? as u32;
    }
    cx.push(value as i32 as i64)?;
    Ok(cx.into())
}

pub fn bxor(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let mut value: u32 = 0;
    for i in 1..=cx.args() {
        value ^= arg_i32(&cx, i)? as u32;
    }
    cx.push(value as i32 as i64)?;
    Ok(cx.into())
}

pub fn bnot(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)?;
    cx.push((!x) as i64)?;
    Ok(cx.into())
}

pub fn lshift(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    let n = arg_shift(&cx, 2)?;
    cx.push((x.wrapping_shl(n) as i32) as i64)?;
    Ok(cx.into())
}

pub fn rshift(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    let n = arg_shift(&cx, 2)?;
    cx.push((x.wrapping_shr(n) as i32) as i64)?;
    Ok(cx.into())
}

pub fn arshift(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)?;
    let n = arg_shift(&cx, 2)?;
    cx.push(x.wrapping_shr(n) as i64)?;
    Ok(cx.into())
}

pub fn rol(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    let n = arg_shift(&cx, 2)?;
    cx.push((x.rotate_left(n) as i32) as i64)?;
    Ok(cx.into())
}

pub fn ror(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    let n = arg_shift(&cx, 2)?;
    cx.push((x.rotate_right(n) as i32) as i64)?;
    Ok(cx.into())
}

pub fn tohex(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    let digits = cx.arg(2).to_nilable_int(false)?.unwrap_or(8);

    let uppercase = digits < 0;
    let width = (digits.unsigned_abs() as usize).clamp(1, 8);

    let full = if uppercase {
        format!("{x:08X}")
    } else {
        format!("{x:08x}")
    };
    let out = &full[(8 - width)..];

    cx.push(Value::Str(cx.create_str(out)))?;
    Ok(cx.into())
}

pub fn bswap(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let x = arg_i32(&cx, 1)? as u32;
    cx.push((x.swap_bytes() as i32) as i64)?;
    Ok(cx.into())
}
