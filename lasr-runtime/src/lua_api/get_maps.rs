use tsuki::{
    Value,
    context::{Args, Context, Ret},
};

use crate::state::{MapRange, Result, State};

pub fn get_maps(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() != 0 {
        cx.push(Value::Nil)?;
        return Ok(cx.into());
    }

    if cx.associated_data().maps_cache.borrow().is_none() {
        let mut maps = Vec::new();
        {
            let process_ref = cx.associated_data().process.borrow();
            let process = process_ref.as_ref().ok_or("no process attached")?;

            for range in process.memory_ranges() {
                let (base, size) = match range.range() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                maps.push(MapRange {
                    start: base.value(),
                    end: base.value() + size,
                    size,
                });
            }
        }

        *cx.associated_data().maps_cache.borrow_mut() = Some(maps);
    }

    let table = cx.create_table();
    if let Some(maps) = cx.associated_data().maps_cache.borrow().as_ref() {
        for (i, map) in maps.iter().enumerate() {
            let entry = cx.create_table();
            // FIXME: name is unavailable in asr.
            entry.set_str_key("name", cx.create_str(""));
            entry.set_str_key("start", map.start as i64);
            entry.set_str_key("end", map.end as i64);
            entry.set_str_key("size", map.size as i64);

            table.set((i + 1) as i64, entry).unwrap();
        }
    }

    cx.push(Value::Table(table))?;
    Ok(cx.into())
}
