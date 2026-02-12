use tsuki::{Value, context::{Args, Context, Ret}};

use crate::state::{Result, State};

pub fn get_module_size(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
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
