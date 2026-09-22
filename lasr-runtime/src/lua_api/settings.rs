use asr::settings::{Map, gui};
use tsuki::{
    Float, Value,
    context::{Args, Context, Ret},
};

use crate::state::{Result, Setting, State};

fn string_field(value: &Value<'_, State>, operation: &str, field: &str) -> Result<String> {
    let Value::Str(value) = value else {
        return Err(format!("[settings.{operation}] {field} must be a string").into());
    };
    let value = value
        .as_utf8()
        .ok_or_else(|| format!("[settings.{operation}] {field} must be valid UTF-8"))?;
    if value.contains('\0') {
        return Err(format!("[settings.{operation}] {field} must not contain NUL bytes").into());
    }
    Ok(value.to_owned())
}

pub fn define(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if !cx.associated_data().defining_settings.get() {
        return Err("[settings.define] only available during define_settings".into());
    }
    if cx.args() != 2 {
        return Err("[settings.define] two arguments are required".into());
    }

    let key = string_field(
        &cx.arg(1).get().ok_or("[settings.define] key is required")?,
        "define",
        "key",
    )?;
    if key.is_empty() {
        return Err("[settings.define] key must not be empty".into());
    }
    if cx.associated_data().settings.borrow().contains_key(&key) {
        return Err(format!("[settings.define] setting {key:?} is already defined").into());
    }

    let definition = cx.arg(2).get_table()?;
    let name = string_field(&definition.get_str_key("name"), "define", "name")?;
    if name.is_empty() {
        return Err("[settings.define] name must not be empty".into());
    }
    let description = match definition.get_str_key("desc") {
        Value::Nil => None,
        value => Some(string_field(&value, "define", "desc")?),
    };
    let setting = match definition.get_str_key("type") {
        Value::Int(0) => match definition.get_str_key("default") {
            Value::True => Setting::Boolean(true),
            Value::False => Setting::Boolean(false),
            _ => return Err("[settings.define] boolean default must be a boolean".into()),
        },
        Value::Int(1) => match definition.get_str_key("default") {
            Value::Int(value) => Setting::Integer(value),
            Value::Float(value)
                if value.0.is_finite()
                    && value.0.fract() == 0.0
                    && value.0 >= i64::MIN as f64
                    && value.0 < i64::MAX as f64 =>
            {
                Setting::Integer(value.0 as i64)
            }
            _ => return Err("[settings.define] integer default must be an integer".into()),
        },
        Value::Int(2) => match definition.get_str_key("default") {
            Value::Int(value) => Setting::Number(value as f64),
            Value::Float(value) => Setting::Number(value.0),
            _ => return Err("[settings.define] number default must be numeric".into()),
        },
        Value::Int(3) => Setting::String(string_field(
            &definition.get_str_key("default"),
            "define",
            "default",
        )?),
        _ => return Err("[settings.define] invalid setting type".into()),
    };

    match &setting {
        Setting::Boolean(default) => {
            gui::add_bool(&key, &name, *default);
        }
        Setting::Integer(default) => gui::add_text_input(&key, &name, &default.to_string()),
        Setting::Number(default) => gui::add_text_input(&key, &name, &default.to_string()),
        Setting::String(default) => gui::add_text_input(&key, &name, default),
    }
    if let Some(description) = description {
        gui::set_tooltip(&key, &description);
    }
    cx.associated_data()
        .settings
        .borrow_mut()
        .insert(key, setting);
    Ok(cx.into())
}

pub fn get(cx: Context<State, Args>) -> Result<Context<State, Ret>> {
    if cx.args() != 1 {
        return Err("[settings.get] one argument is required".into());
    }
    let key = string_field(
        &cx.arg(1).get().ok_or("[settings.get] key is required")?,
        "get",
        "key",
    )?;
    let setting = cx.associated_data().settings.borrow().get(&key).cloned();
    let value = setting.and_then(|default| {
        let configured = Map::load().get(&key);
        match default {
            Setting::Boolean(default) => Some(
                if configured.and_then(|v| v.get_bool()).unwrap_or(default) {
                    Value::True
                } else {
                    Value::False
                },
            ),
            Setting::Integer(default) => {
                let value = configured
                    .and_then(|v| {
                        v.get_i64()
                            .or_else(|| v.get_string().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(default);
                Some(Value::Int(value))
            }
            Setting::Number(default) => {
                let value = configured
                    .and_then(|v| {
                        v.get_f64()
                            .or_else(|| v.get_i64().map(|n| n as f64))
                            .or_else(|| v.get_string().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(default);
                Some(Value::Float(Float(value)))
            }
            Setting::String(default) => {
                let value = configured.and_then(|v| v.get_string()).unwrap_or(default);
                Some(Value::Str(cx.create_str(value)))
            }
        }
    });
    cx.push(value.unwrap_or(Value::Nil))?;
    Ok(cx.into())
}
