use std::path::Path;

use asr::{Process, future::next_tick};
use tsuki::context::{Args, Context, Ret};

use crate::state::{Result, State};

#[derive(Clone, Copy)]
enum SortOrder {
    First,
    Last,
}

fn actual_process_name(process: &Process, requested_name: &str) -> String {
    process
        .get_path()
        .ok()
        .and_then(|path| {
            Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| requested_name.to_owned())
}

fn best_effort_cmdline_process_name(command_line: &str) -> &str {
    let trimmed = command_line.trim();
    if trimmed.is_empty() {
        return trimmed;
    }

    let executable = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next().unwrap_or(rest)
    } else {
        trimmed.split_whitespace().next().unwrap_or(trimmed)
    };

    Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(executable)
}

fn parse_sort(cx: &Context<State, Args>) -> Result<SortOrder> {
    let sort_value = cx.arg(2);

    let Some(sort_arg) = sort_value.to_nilable_str(false)? else {
        return Ok(SortOrder::First);
    };

    let sort = sort_arg
        .as_utf8()
        .ok_or_else(|| sort_value.error("sort is not valid UTF-8"))?;

    match sort {
        "first" => Ok(SortOrder::First),
        "last" => Ok(SortOrder::Last),
        _ => {
            asr::print_message(&format!(
                "[process] Invalid sort argument '{sort}'. Use 'first' or 'last'. Falling back to first"
            ));
            Ok(SortOrder::First)
        }
    }
}

fn attach_with_sort(process_name: &str, sort: SortOrder) -> Option<Process> {
    let mut pids = Process::list_by_name(process_name)?;
    pids.sort_unstable_by_key(|pid| pid.0);

    match sort {
        SortOrder::First => pids.into_iter().find_map(Process::attach_by_pid),
        SortOrder::Last => pids.into_iter().rev().find_map(Process::attach_by_pid),
    }
}

async fn wait_attach_with_sort(process_name: &str, sort: SortOrder) -> Process {
    loop {
        if let Some(process) = attach_with_sort(process_name, sort) {
            return process;
        }

        if let Some(process) = Process::attach(process_name) {
            return process;
        }

        next_tick().await;
    }
}

async fn attach_process<'a>(
    cx: Context<'a, State, Args>,
    fallback_cmdline_match: bool,
) -> Result<Context<'a, State, Ret>> {
    let arg = cx.arg(1);
    let process_name = arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| arg.error("processName is not valid UTF-8"))?;

    let attach_name = if fallback_cmdline_match {
        best_effort_cmdline_process_name(process_name)
    } else {
        process_name
    };

    if fallback_cmdline_match {
        asr::print_message(
            "[cmdline] Full command-line matching is not supported by the asr runtime. Falling back to process-name matching.",
        );
    }

    let sort = parse_sort(&cx)?;
    let process = wait_attach_with_sort(attach_name, sort).await;
    let actual_name = actual_process_name(&process, attach_name);

    let base_address = process
        .get_module_address(&actual_name)
        .or_else(|_| process.get_module_address(attach_name))
        .map_err(|_| "failed to get process base address")?;

    *cx.associated_data().process.borrow_mut() = Some(process);
    cx.associated_data().base_address.set(base_address);
    *cx.associated_data().process_name.borrow_mut() = Some(actual_name);
    *cx.associated_data().maps_cache.borrow_mut() = None;
    cx.associated_data().maps_cache_cycles.set(1);
    cx.associated_data().maps_cache_cycles_value.set(1);

    Ok(cx.into())
}

pub async fn process<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    attach_process(cx, false).await
}

pub async fn cmdline<'a>(cx: Context<'a, State, Args>) -> Result<Context<'a, State, Ret>> {
    attach_process(cx, true).await
}
