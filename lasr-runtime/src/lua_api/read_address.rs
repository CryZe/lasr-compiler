use std::str;

use asr::{Address, Address32, Address64};
use tsuki::{
    Float, Value,
    context::{Args, Context, Ret},
};

use crate::state::{Result, State};

pub fn read_address(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    let ty_arg = cx.arg(1);
    let ty = ty_arg
        .to_str()?
        .as_utf8()
        .ok_or_else(|| ty_arg.error("type is not valid UTF-8"))?;

    let value = {
        let process = &*cx.associated_data().process.borrow();
        let process = process.as_ref().ok_or("no process attached")?;

        let module_or_addr = cx.arg(2);

        if matches!(module_or_addr.get(), Some(Value::Nil)) {
            asr::print_message(
                "[readAddress] The address argument cannot be nil. Check your auto splitter code.",
            );
            Value::Nil
        } else {
            let (start_offsets, mut address) = if let Some(module) = module_or_addr.as_str(false) {
                let module = module
                    .as_utf8()
                    .ok_or_else(|| module_or_addr.error("module name is not valid UTF-8"))?;

                let base = process.get_module_address(module).unwrap_or(Address::NULL);

                (4, base + cx.arg(3).to_int()? as u64)
            } else {
                (
                    3,
                    cx.associated_data().base_address.get() + module_or_addr.to_int()? as u64,
                )
            };

            let mut memory_error = false;

            for i in start_offsets..=cx.args() {
                if address.value() <= u32::MAX as u64 {
                    address = match process.read::<Address32>(address) {
                        Ok(next) => next.into(),
                        Err(_) => {
                            memory_error = true;
                            break;
                        }
                    };
                } else {
                    address = match process.read::<Address64>(address) {
                        Ok(next) => next.into(),
                        Err(_) => {
                            memory_error = true;
                            break;
                        }
                    };
                }
                address = address + cx.arg(i).to_int()? as u64;
            }

            if memory_error {
                asr::print_message("[readAddress] Failed to read process memory");
                Value::Nil
            } else {
                let mut suppress_memory_error = false;
                let value = match ty {
                    "sbyte" => match process.read::<i8>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "byte" => match process.read::<u8>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "short" => match process.read::<i16>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "ushort" => match process.read::<u16>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "int" => match process.read::<i32>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "uint" => match process.read::<u32>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "long" => match process.read::<i64>(address) {
                        Ok(v) => Value::Int(v),
                        Err(_) => Value::Nil,
                    },
                    "ulong" => match process.read::<u64>(address) {
                        Ok(v) => Value::Int(v as _),
                        Err(_) => Value::Nil,
                    },
                    "float" => match process.read::<f32>(address) {
                        Ok(v) => Value::Float(Float(v as _)),
                        Err(_) => Value::Nil,
                    },
                    "double" => match process.read::<f64>(address) {
                        Ok(v) => Value::Float(Float(v)),
                        Err(_) => Value::Nil,
                    },
                    "bool" => match process.read::<u8>(address) {
                        Ok(v) => {
                            if v == 0 {
                                Value::False
                            } else {
                                Value::True
                            }
                        }
                        Err(_) => Value::Nil,
                    },
                    _ => {
                        if let Some(rem) = ty.strip_prefix("string") {
                            match rem.parse::<usize>() {
                                Ok(byte_count) if byte_count >= 2 => {
                                    let mut buf = vec![0; byte_count];
                                    if process.read_into_buf(address, &mut buf).is_err() {
                                        asr::print_message(
                                            "[readAddress] Failed to read process memory",
                                        );
                                        Value::Nil
                                    } else {
                                        let len =
                                            buf.iter().position(|&b| b == 0).unwrap_or(byte_count);
                                        match str::from_utf8(&buf[..len]) {
                                            Ok(s) => Value::Str(cx.create_str(s)),
                                            Err(_) => Value::Nil,
                                        }
                                    }
                                }
                                _ => {
                                    asr::print_message(
                                        "[readAddress] Invalid string size, please read documentation",
                                    );
                                    suppress_memory_error = true;
                                    Value::Nil
                                }
                            }
                        } else if let Some(rem) = ty.strip_prefix("byte") {
                            match rem.parse::<usize>() {
                                Ok(byte_count) if byte_count >= 1 => {
                                    let mut buf = vec![0u8; byte_count];
                                    if process.read_into_buf(address, &mut buf).is_err() {
                                        asr::print_message(
                                            "[readAddress] Failed to read process memory",
                                        );
                                        Value::Nil
                                    } else {
                                        let table = cx.create_table();
                                        for (i, byte) in buf.into_iter().enumerate() {
                                            table.set((i + 1) as i64, byte as i64).unwrap();
                                        }

                                        Value::Table(table)
                                    }
                                }
                                _ => {
                                    asr::print_message(
                                        "[readAddress] Invalid byte array size, please read documentation",
                                    );
                                    suppress_memory_error = true;
                                    Value::Nil
                                }
                            }
                        } else {
                            asr::print_message(&format!("[readAddress] Invalid value type: {ty}"));
                            suppress_memory_error = true;
                            Value::Nil
                        }
                    }
                };

                if matches!(value, Value::Nil) && !suppress_memory_error {
                    asr::print_message("[readAddress] Failed to read process memory");
                }

                value
            }
        }
    };

    cx.push(value)?;
    Ok(cx.into())
}
