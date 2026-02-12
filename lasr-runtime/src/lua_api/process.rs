use asr::Process;
use tsuki::context::{Args, Context, Ret};

use crate::state::{Result, State};

pub async fn process<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    let arg = cx.arg(1);
    let process_name = arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| arg.error("processName is not valid UTF-8"))?;

    // FIXME: sort argument is currently ignored, but we parse it anyway to
    // match the behavior in the original C LASR.
    let sort_value = cx.arg(2);
    if let Some(sort_arg) = sort_value.to_nilable_str(false)? {
        let sort = sort_arg
            .as_utf8()
            .ok_or_else(|| sort_value.error("sort is not valid UTF-8"))?;

        if sort != "first" && sort != "last" {
            asr::print_message(
                "[process] Invalid sort argument. Use 'first' or 'last'. Falling back to first",
            );
        }
    }

    let process = Process::wait_attach(process_name).await;

    let base_address = process
        .get_module_address(process_name)
        .map_err(|_| "failed to get process base address")?;

    *cx.associated_data().process.borrow_mut() = Some(process);
    cx.associated_data().base_address.set(base_address);
    *cx.associated_data().process_name.borrow_mut() = Some(process_name.to_owned());
    *cx.associated_data().maps_cache.borrow_mut() = None;
    cx.associated_data().maps_cache_cycles.set(1);
    cx.associated_data().maps_cache_cycles_value.set(1);

    Ok(cx.into())
}
