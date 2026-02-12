use core::error::Error;
use std::{
    cell::{Cell, RefCell},
    string::String,
};

use asr::{Address, Process};

pub type Result<T, E = Box<dyn Error>> = std::result::Result<T, E>;

pub struct State {
    pub process: RefCell<Option<Process>>,
    pub base_address: Cell<Address>,
    pub process_name: RefCell<Option<String>>,
    pub maps_cache: RefCell<Option<Vec<MapRange>>>,
    pub maps_cache_cycles: Cell<i64>,
    pub maps_cache_cycles_value: Cell<i64>,
}

#[derive(Clone, Copy)]
pub struct MapRange {
    pub start: u64,
    pub end: u64,
    pub size: u64,
}
