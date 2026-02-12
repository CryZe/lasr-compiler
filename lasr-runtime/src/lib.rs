#![allow(clippy::await_holding_refcell_ref)]

use core::error::Error;
use std::{
    cell::{Cell, RefCell},
    pin::Pin,
    rc::Rc,
    str,
    string::String,
};

use asr::{
    Address, Address32, Address64, Process,
    future::next_tick,
    time::Duration,
    timer::{self, TimerState},
};
use tsuki::{
    Float, Lua, Ref, Thread, Value,
    builtin::{BaseLib, CoroLib, IoLib, MathLib, OsLib, StrLib, TableLib, Utf8Lib},
    context::{Args, Context, Ret},
    fp,
};

asr::async_main!(stable);

#[repr(C)]
pub struct ScriptSlice {
    ptr: *const u8,
    len: usize,
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn lasr_script() -> ScriptSlice {
    ScriptSlice {
        ptr: std::ptr::null(),
        len: 0,
    }
}

fn script_str() -> &'static str {
    let slice = unsafe {
        let ScriptSlice { ptr, len } = lasr_script();
        std::slice::from_raw_parts(ptr, len)
    };
    std::str::from_utf8(slice).expect("lasr_script returned non-UTF-8")
}

struct State {
    process: RefCell<Option<Process>>,
    base_address: Cell<Address>,
    process_name: RefCell<Option<String>>,
}

// FIXME: asr should pretend to call async main on non-wasm targets, so we don't
// have to silence the unused warning (we force it through no_mangle so unused
// warnings inside are still detected).
#[cfg_attr(not(target_family = "wasm"), unsafe(no_mangle))]
async fn main() {
    loop {
        let lua = Lua::new(State {
            process: RefCell::new(None),
            base_address: Cell::new(Address::NULL),
            process_name: RefCell::new(None),
        });

        lua.use_module(None, true, BaseLib).unwrap();
        lua.use_module(None, true, CoroLib).unwrap();
        lua.use_module(None, true, IoLib).unwrap();
        lua.use_module(None, true, MathLib).unwrap();
        lua.use_module(None, true, OsLib).unwrap();
        lua.use_module(None, true, StrLib).unwrap();
        lua.use_module(None, true, TableLib).unwrap();
        lua.use_module(None, true, Utf8Lib).unwrap();

        lua.global().set_str_key("process", fp!(process as async));
        lua.global().set_str_key("readAddress", fp!(read_address));
        lua.global().set_str_key("getPID", fp!(get_pid));
        lua.global().set_str_key("print", fp!(print));
        lua.global().set_str_key("sig_scan", fp!(sig_scan as async));
        lua.global()
            .set_str_key("getBaseAddress", fp!(get_base_address));
        lua.global().set_str_key("sizeOf", fp!(size_of));
        lua.global()
            .set_str_key("getModuleSize", fp!(get_module_size));
        lua.global().set_str_key("getMaps", fp!(get_maps));

        lua.global().set_str_key("setVariable", fp!(set_variable));

        let chunk = lua.load("script.lua", script_str()).unwrap();
        let td = lua.create_thread();

        () = td.async_call(&chunk, ()).await.unwrap();

        let mut use_game_time = false;

        let startup_fn = lua.global().get_str_key("startup");
        if let Value::LuaFn(func) = startup_fn {
            () = td.async_call(&func, ()).await.unwrap();

            match lua.global().get_str_key("refreshRate") {
                Value::Int(refresh_rate) => asr::set_tick_rate(refresh_rate as _),
                Value::Float(refresh_rate) => asr::set_tick_rate(refresh_rate.0),
                _ => {}
            }

            if let Value::True = lua.global().get_str_key("useGameTime") {
                use_game_time = true;
            }
        }

        while lua
            .associated_data()
            .process
            .borrow()
            .as_ref()
            .is_some_and(|p| p.is_open())
        {
            call_maybe(&lua, &td, "state").await;
            call_maybe(&lua, &td, "update").await;

            let timer_state = timer::state();

            if use_game_time && let TimerState::Running | TimerState::Paused = timer_state {
                let game_time_fn = lua.global().get_str_key("gameTime");
                if let Value::LuaFn(func) = game_time_fn {
                    match td.async_call(&func, ()).await.unwrap() {
                        Value::Int(millis) => timer::set_game_time(Duration::milliseconds(millis)),
                        Value::Float(Float(millis)) => {
                            timer::set_game_time(Duration::seconds_f64(millis * 0.001))
                        }
                        _ => {}
                    }
                }
            }

            if let TimerState::NotRunning = timer_state
                && let Some(true) = call_maybe_bool(&lua, &td, "start").await
            {
                timer::start();
            }

            if let TimerState::Running | TimerState::Paused = timer_state
                && let Some(true) = call_maybe_bool(&lua, &td, "split").await
            {
                timer::split();
            }

            match call_maybe_bool(&lua, &td, "isLoading").await {
                Some(true) => timer::pause_game_time(),
                Some(false) => timer::resume_game_time(),
                None => {}
            }

            // I feel like this should also check the timer state.
            if let Some(true) = call_maybe_bool(&lua, &td, "reset").await {
                timer::reset();
            }

            next_tick().await;
        }
    }
}

async fn call_maybe(lua: &Pin<Rc<Lua<State>>>, td: &Ref<'_, Thread<State>>, name: &str) {
    let func = lua.global().get_str_key(name);
    if let Value::LuaFn(func) = func {
        () = td.async_call(&func, ()).await.unwrap();
    }
}

async fn call_maybe_bool(
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

type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

async fn process<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    let arg = cx.arg(1);
    let process_name = arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| arg.error("processName is not valid UTF-8"))?;

    let process = Process::wait_attach(process_name).await;

    let base_address = process
        .get_module_address(process_name)
        .map_err(|_| "failed to get process base address")?;

    *cx.associated_data().process.borrow_mut() = Some(process);
    cx.associated_data().base_address.set(base_address);
    *cx.associated_data().process_name.borrow_mut() = Some(process_name.to_owned());

    Ok(cx.into())
}

fn set_variable(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
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

fn read_address(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let ty_arg = cx.arg(1);
    let ty = ty_arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| ty_arg.error("type is not valid UTF-8"))?;

    {
        let process = &*cx.associated_data().process.borrow();
        let process = process.as_ref().ok_or("no process attached")?;

        let module_or_addr = cx.arg(2);

        let (start_offsets, mut address) = if let Some(module) = module_or_addr.as_str(false) {
            let module = module
                .as_utf8()
                .ok_or_else(|| module_or_addr.error("module name is not valid UTF-8"))?;

            (
                4,
                process
                    .get_module_address(module)
                    .map_err(|_| module_or_addr.error("module not found"))?
                    + cx.arg(3).to_int()? as u64,
            )
        } else {
            (
                3,
                cx.associated_data().base_address.get() + module_or_addr.to_int()? as u64,
            )
        };

        for i in start_offsets..=cx.args() {
            if address.value() <= u32::MAX as u64 {
                address = process.read::<Address32>(address).unwrap().into();
            } else {
                address = process.read::<Address64>(address).unwrap().into();
            }
            address = address + cx.arg(i).to_int()? as u64;
        }

        let value = match ty {
            "sbyte" => Value::Int(process.read::<i8>(address).unwrap() as _),
            "byte" => Value::Int(process.read::<u8>(address).unwrap() as _),
            "short" => Value::Int(process.read::<i16>(address).unwrap() as _),
            "ushort" => Value::Int(process.read::<u16>(address).unwrap() as _),
            "int" => Value::Int(process.read::<i32>(address).unwrap() as _),
            "uint" => Value::Int(process.read::<u32>(address).unwrap() as _),
            "long" => Value::Int(process.read::<i64>(address).unwrap()),
            "ulong" => Value::Int(process.read::<u64>(address).unwrap() as _),
            "float" => Value::Float(Float(process.read::<f32>(address).unwrap() as _)),
            "double" => Value::Float(Float(process.read::<f64>(address).unwrap())),
            "bool" => match process.read::<u8>(address).unwrap() {
                0 => Value::False,
                _ => Value::True,
            },
            _ => {
                if let Some(rem) = ty.strip_prefix("string")
                    && let Ok(byte_count) = rem.parse::<usize>()
                {
                    let mut buf = vec![0; byte_count];
                    process.read_into_buf(address, &mut buf).unwrap();
                    let len = buf.iter().position(|&b| b == 0).unwrap_or(byte_count);
                    Value::Str(cx.create_str(str::from_utf8(&buf[..len]).unwrap()))
                } else if let Some(rem) = ty.strip_prefix("byte")
                    && !rem.is_empty()
                    && let Ok(byte_count) = rem.parse::<usize>()
                {
                    let mut buf = vec![0u8; byte_count];
                    process.read_into_buf(address, &mut buf).unwrap();

                    let table = cx.create_table();
                    for (i, byte) in buf.into_iter().enumerate() {
                        table.set((i + 1) as i64, byte as i64).unwrap();
                    }

                    Value::Table(table)
                } else {
                    return Err(ty_arg.error("unsupported type"));
                }
            }
        };

        cx.push(value)?;
    }
    Ok(cx.into())
}

fn get_pid(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    // FIXME: pid is unavailable in asr, so we return a dummy value for now.
    cx.push(0)?;
    Ok(cx.into())
}

fn get_base_address(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let module_arg = cx.arg(1);

    let address = {
        let process_ref = cx.associated_data().process.borrow();
        let process = process_ref.as_ref().ok_or("no process attached")?;

        if let Some(module) = module_arg.to_nilable_str(false)? {
            let module = module
                .as_utf8()
                .ok_or_else(|| module_arg.error("module name is not valid UTF-8"))?;
            process
                .get_module_address(module)
                .map_err(|_| module_arg.error("module not found"))?
        } else {
            cx.associated_data().base_address.get()
        }
    };

    cx.push(Value::Int(address.value() as i64))?;
    Ok(cx.into())
}

fn get_module_size(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let module_arg = cx.arg(1);

    let size = {
        let process_ref = cx.associated_data().process.borrow();
        let process = process_ref.as_ref().ok_or("no process attached")?;

        if let Some(module) = module_arg.to_nilable_str(false)? {
            let module = module
                .as_utf8()
                .ok_or_else(|| module_arg.error("module name is not valid UTF-8"))?;
            process
                .get_module_size(module)
                .map_err(|_| module_arg.error("module not found"))?
        } else {
            let name_ref = cx.associated_data().process_name.borrow();
            let name = name_ref.as_ref().ok_or("no process name available")?;
            process
                .get_module_size(name)
                .map_err(|_| module_arg.error("module not found"))?
        }
    };

    cx.push(Value::Int(size as i64))?;
    Ok(cx.into())
}

fn size_of(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
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

fn get_maps(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let table = cx.create_table();
    {
        let process_ref = cx.associated_data().process.borrow();
        let process = process_ref.as_ref().ok_or("no process attached")?;

        let mut index: i64 = 1;
        for range in process.memory_ranges() {
            let (base, size) = match range.range() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let entry = cx.create_table();
            // FIXME: name is unavailable in asr.
            entry.set_str_key("name", cx.create_str(""));
            entry.set_str_key("start", base.value() as i64);
            entry.set_str_key("end", (base.value() + size) as i64);
            entry.set_str_key("size", size as i64);

            table.set(index, entry).unwrap();
            index += 1;
        }
    }

    cx.push(Value::Table(table))?;
    Ok(cx.into())
}

#[derive(Copy, Clone)]
struct SigByte {
    value: u8,
    mask: u8,
}

fn parse_sig_token(token: &str) -> Result<SigByte, &'static str> {
    if token == "?" || token == "??" {
        return Ok(SigByte { value: 0, mask: 0 });
    }

    let bytes = token.as_bytes();
    if bytes.len() != 2 {
        return Err("signature token must be 2 hex chars or '?' wildcards");
    }

    let (hi_val, hi_mask) = match bytes[0] {
        b'?' => (0, 0),
        c => (hex_nibble(c)?, 0xF),
    };
    let (lo_val, lo_mask) = match bytes[1] {
        b'?' => (0, 0),
        c => (hex_nibble(c)?, 0xF),
    };

    Ok(SigByte {
        value: (hi_val << 4) | lo_val,
        mask: (hi_mask << 4) | lo_mask,
    })
}

fn hex_nibble(c: u8) -> Result<u8, &'static str> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("signature contains non-hex characters"),
    }
}

fn parse_signature(pattern: &str) -> Result<Vec<SigByte>, &'static str> {
    let mut out = Vec::new();
    for token in pattern.split_whitespace() {
        out.push(parse_sig_token(token)?);
    }
    if out.is_empty() {
        return Err("signature is empty");
    }
    Ok(out)
}

async fn sig_scan<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    let signature = {
        let pattern_arg = cx.arg(1);
        let pattern = pattern_arg
            .to_str()?
            .as_utf8()
            .ok_or_else(|| pattern_arg.error("signature is not valid UTF-8"))?
            .to_owned();
        parse_signature(&pattern).map_err(|msg| pattern_arg.error(msg))?
    };

    let offset = {
        let offset_arg = cx.arg(2);
        offset_arg.to_int()?
    };

    let found = {
        let process_ref = cx.associated_data().process.borrow();
        let process = process_ref.as_ref().ok_or("no process attached")?;

        scan_signature(process, &signature, offset).await?
    };

    let base_address = cx.associated_data().base_address.get().value() as i64;

    cx.push(if let Some(address) = found {
        Value::Int(address.wrapping_sub(base_address))
    } else {
        Value::Nil
    })?;
    Ok(cx.into())
}

async fn scan_signature(
    process: &Process,
    signature: &[SigByte],
    offset: i64,
) -> Result<Option<i64>, &'static str> {
    let sig_len = signature.len();
    let chunk_size: usize = 0x10000;
    let mut buf = vec![0u8; chunk_size];
    let lps = build_lps(signature);

    let mut chunk_counter: u32 = 0;
    for range in process.memory_ranges() {
        let (base, range_size) = range.range().map_err(|_| "failed to query memory range")?;

        if range_size == 0 {
            continue;
        }
        let mut offset_bytes: u64 = 0;
        let mut matched: usize = 0;
        while offset_bytes < range_size {
            let remaining = (range_size - offset_bytes) as usize;
            let read_len = remaining.min(chunk_size);
            let buf_slice = &mut buf[..read_len];

            if process
                .read_into_buf(base + offset_bytes, buf_slice)
                .is_err()
            {
                break;
            }

            for (i, &byte) in buf_slice.iter().enumerate() {
                while matched > 0 && !sig_byte_matches(signature[matched], byte) {
                    matched = lps[matched - 1];
                }
                if sig_byte_matches(signature[matched], byte) {
                    matched += 1;
                    if matched == sig_len {
                        let found = offset_bytes + i as u64 + 1 - sig_len as u64;
                        let address = base.value() as i64 + found as i64 + offset;
                        return Ok(Some(address));
                    }
                }
            }

            offset_bytes += read_len as u64;
            chunk_counter = chunk_counter.wrapping_add(1);
            if chunk_counter.is_multiple_of(64) {
                next_tick().await;
            }
        }
    }

    Ok(None)
}

fn build_lps(signature: &[SigByte]) -> Vec<usize> {
    let mut lps = vec![0usize; signature.len()];
    let mut len = 0;

    for i in 1..signature.len() {
        while len > 0 && !sig_byte_eq(signature[i], signature[len]) {
            len = lps[len - 1];
        }
        if sig_byte_eq(signature[i], signature[len]) {
            len += 1;
            lps[i] = len;
        }
    }

    lps
}

fn sig_byte_matches(sig: SigByte, byte: u8) -> bool {
    (byte & sig.mask) == sig.value
}

fn sig_byte_eq(left: SigByte, right: SigByte) -> bool {
    left.value == right.value && left.mask == right.mask
}

fn print(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    // Fast path for zero or one argument.
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

    // We can't print while converting the arguments to string since it can call into arbitrary
    // function, which may lock stdout.
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
