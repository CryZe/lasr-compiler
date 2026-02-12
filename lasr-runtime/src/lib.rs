#![allow(clippy::await_holding_refcell_ref)]

use std::{
    cell::{Cell, RefCell},
    pin::Pin,
    rc::Rc,
};

use asr::{
    Address,
    future::next_tick,
    time::Duration,
    timer::{self, TimerState},
};
use tsuki::{
    Float, Lua, Ref, Thread, Value,
    builtin::{BaseLib, CoroLib, IoLib, MathLib, OsLib, StrLib, TableLib, Utf8Lib},
    fp,
};

mod lua_api;
mod luajit_bitlib;
mod script;
mod state;
mod utils;

use lua_api::{
    get_base_address, get_maps, get_module_size, get_pid, print, print_tbl, process, read_address,
    set_variable, shallow_copy_tbl, sig_scan, size_of,
};
use luajit_bitlib::LuaJitBitLib;
use script::script_str;
use state::State;
use utils::{call_maybe, call_maybe_bool};

asr::async_main!(stable);

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
            maps_cache: RefCell::new(None),
            maps_cache_cycles: Cell::new(1),
            maps_cache_cycles_value: Cell::new(1),
        });

        lua.use_module(None, true, BaseLib).unwrap();
        lua.use_module(None, true, CoroLib).unwrap();
        lua.use_module(None, true, IoLib).unwrap();
        lua.use_module(None, true, MathLib).unwrap();
        lua.use_module(None, true, LuaJitBitLib).unwrap();
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
        lua.global().set_str_key("print_tbl", fp!(print_tbl));
        lua.global()
            .set_str_key("shallow_copy_tbl", fp!(shallow_copy_tbl));

        lua.global().set_str_key("setVariable", fp!(set_variable));

        let td = lua.create_thread();

        let chunk = lua.load("script.lua", script_str()).unwrap();
        () = td.async_call(&chunk, ()).await.unwrap();

        let use_game_time = startup(&lua, &td).await;

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

            let next_cycles = lua.associated_data().maps_cache_cycles_value.get() - 1;

            lua.associated_data()
                .maps_cache_cycles_value
                .set(next_cycles);

            if next_cycles < 1 {
                *lua.associated_data().maps_cache.borrow_mut() = None;
                lua.associated_data()
                    .maps_cache_cycles_value
                    .set(lua.associated_data().maps_cache_cycles.get());
            }

            next_tick().await;
        }
    }
}

async fn startup(lua: &Pin<Rc<Lua<State>>>, td: &Ref<'_, Thread<State>>) -> bool {
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

        match lua.global().get_str_key("mapsCacheCycles") {
            Value::Int(cycles) => {
                let cycles = cycles.max(0);
                lua.associated_data().maps_cache_cycles.set(cycles);
                lua.associated_data().maps_cache_cycles_value.set(cycles);
            }
            Value::Float(Float(cycles)) => {
                let cycles = (cycles as i64).max(0);
                lua.associated_data().maps_cache_cycles.set(cycles);
                lua.associated_data().maps_cache_cycles_value.set(cycles);
            }
            _ => {}
        }
    }

    use_game_time
}
