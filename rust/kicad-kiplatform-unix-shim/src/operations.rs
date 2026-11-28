// SPDX-License-Identifier: GPL-3.0-or-later
//! Operation decoding and execution: the counterpart of `run` in
//! `port/oracle/shims/kiplatform_unix_ui_p001.cpp`, field for field, with the same validation
//! order and messages.

use kicad_kiplatform::app::AppPlatform;
use kicad_kiplatform::environment::{EnvPlatform, ProxyConfig};
use kicad_kiplatform::io::{IoPlatform, MappedData, TargetAttrs};
use kicad_kiplatform::printing::PrintingPlatform;
use kicad_kiplatform::secrets::{GetDone, SecretsPlatform, StoreDone};
use kicad_kiplatform_unix::environment::REFERENCE_BUILD;
use kicad_kiplatform_unix::platform::UnixPlatform;
use kicad_kiplatform_unix::printing::{PrintNativeData, WxPrintData};
use kicad_kiplatform_unix::{Ptr, UnixError};
use kicad_map_helpers::WxString;
use serde_json::{json, Map, Value};

use crate::host::{read_settings, ScriptedHost};
use crate::json::{
    check_keys, hex, member, read_bool, read_hex, read_integer, read_text, read_units, refuse,
    units, Refusal, Result,
};

/// What a run of the port ended with, other than a value.
fn port_error(error: UnixError) -> Refusal {
    match error {
        UnixError::Host(message) => Refusal::Input(message.to_owned()),
        UnixError::Undefined(why) => Refusal::Input(format!("undefined: {why}")),
        UnixError::Unported { upstream, unit } => {
            Refusal::Execution(format!("unported\u{0}{upstream}\u{0}{unit}"))
        }
        UnixError::BadFunctionCall => Refusal::Execution("bad_function_call".into()),
        UnixError::Wx(_) | UnixError::Alloc => Refusal::Execution("allocation refused".into()),
    }
}

/// A deferral, recognised by [`crate::dispatch`].
pub fn as_unported(refusal: &Refusal) -> Option<(String, String)> {
    match refusal {
        Refusal::Execution(message) => {
            let mut parts = message.split('\u{0}');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("unported"), Some(upstream), Some(unit)) => {
                    Some((upstream.to_owned(), unit.to_owned()))
                }
                _ => None,
            }
        }
        Refusal::Input(_) => None,
    }
}

type Platform = UnixPlatform<ScriptedHost>;

fn result(platform: &Platform, mut extra: Map<String, Value>) -> Value {
    extra.insert(
        "calls".into(),
        Value::from(platform.host.calls.borrow().clone()),
    );
    Value::Object(extra)
}

fn returned(platform: &Platform, value: Value) -> Value {
    let mut extra = Map::new();
    extra.insert("returned".into(), value);
    result(platform, extra)
}

fn arg_units(args: &Value, name: &str) -> Result<WxString> {
    read_units(member(args, name)?, name)
}

fn platform(host: ScriptedHost) -> Platform {
    UnixPlatform::new(host)
}

fn trivial(args: &Value, host: ScriptedHost) -> Result<Value> {
    let function = read_text(member(args, "function")?, "function")?;
    let mut p = platform(host);
    let window = |args: &Value| -> Result<Option<Ptr>> {
        Ok(read_bool(member(args, "window")?, "window")?.then_some(Ptr(0)))
    };
    let value = match function {
        "enable_dark_mode" => {
            check_keys(args, &["function", "flag"], "args")?;
            let flag = read_bool(&args["flag"], "flag")?;
            p.enable_dark_mode(flag).map_err(port_error)?;
            Value::Null
        }
        "attach_console" => {
            check_keys(args, &["function", "flag"], "args")?;
            let flag = read_bool(&args["flag"], "flag")?;
            Value::from(p.attach_console(flag).map_err(port_error)?)
        }
        "is_operating_system_unsupported"
        | "unregister_application_restart"
        | "supports_shutdown_block_reason"
        | "force_timer_messages_to_be_created_if_necessary"
        | "get_app_user_model_id"
        | "get_command_line_str" => {
            check_keys(args, &["function"], "args")?;
            match function {
                "is_operating_system_unsupported" => {
                    Value::from(p.is_operating_system_unsupported().map_err(port_error)?)
                }
                "unregister_application_restart" => {
                    Value::from(p.unregister_application_restart().map_err(port_error)?)
                }
                "supports_shutdown_block_reason" => {
                    Value::from(p.supports_shutdown_block_reason().map_err(port_error)?)
                }
                "force_timer_messages_to_be_created_if_necessary" => {
                    p.force_timer_messages_to_be_created_if_necessary()
                        .map_err(port_error)?;
                    Value::Null
                }
                "get_app_user_model_id" => units(&p.get_app_user_model_id().map_err(port_error)?),
                _ => units(&p.get_command_line_str().map_err(port_error)?),
            }
        }
        "register_application_restart"
        | "add_dynamic_library_search_path"
        | "is_network_path"
        | "verify_file_signature"
        | "add_to_recent_docs" => {
            check_keys(args, &["function", "string"], "args")?;
            let text = arg_units(args, "string")?;
            match function {
                "register_application_restart" => {
                    Value::from(p.register_application_restart(&text).map_err(port_error)?)
                }
                "add_dynamic_library_search_path" => {
                    p.add_dynamic_library_search_path(&text)
                        .map_err(port_error)?;
                    Value::Null
                }
                "is_network_path" => Value::from(p.is_network_path(&text).map_err(port_error)?),
                "verify_file_signature" => {
                    Value::from(p.verify_file_signature(&text).map_err(port_error)?)
                }
                _ => {
                    p.add_to_recent_docs(&text).map_err(port_error)?;
                    Value::Null
                }
            }
        }
        "remove_shutdown_block_reason" => {
            check_keys(args, &["function", "window"], "args")?;
            let win = window(args)?;
            p.remove_shutdown_block_reason(win.as_ref())
                .map_err(port_error)?;
            Value::Null
        }
        "set_shutdown_block_reason" => {
            check_keys(args, &["function", "window", "string"], "args")?;
            let win = window(args)?;
            let text = arg_units(args, "string")?;
            p.set_shutdown_block_reason(win.as_ref(), &text)
                .map_err(port_error)?;
            Value::Null
        }
        "set_app_details_for_window" => {
            check_keys(args, &["function", "window", "string", "name"], "args")?;
            let win = window(args)?;
            let command = arg_units(args, "string")?;
            let name = arg_units(args, "name")?;
            p.set_app_details_for_window(win.as_ref(), &command, &name)
                .map_err(port_error)?;
            Value::Null
        }
        "capture_target_attributes" => {
            check_keys(args, &["function", "string"], "args")?;
            let text = arg_units(args, "string")?;
            let attrs = p.capture_target_attributes(&text).map_err(port_error)?;
            json!({"value": attrs.value, "captured": attrs.captured})
        }
        "apply_target_attributes" => {
            check_keys(args, &["function", "string", "value", "captured"], "args")?;
            let text = arg_units(args, "string")?;
            let attrs = TargetAttrs {
                value: read_integer(&args["value"], "value", 0, i64::from(u32::MAX))? as u32,
                captured: read_bool(&args["captured"], "captured")?,
            };
            Value::from(
                p.apply_target_attributes(&text, &attrs)
                    .map_err(port_error)?,
            )
        }
        "long_path_adjustment" => {
            check_keys(args, &["function", "string"], "args")?;
            let mut name = arg_units(args, "string")?;
            let before = name.units().to_vec();
            p.long_path_adjustment(&mut name).map_err(port_error)?;
            let mut extra = Map::new();
            extra.insert("returned".into(), Value::Null);
            extra.insert(
                "changed".into(),
                Value::from(name.units() != before.as_slice()),
            );
            return Ok(result(&p, extra));
        }
        _ => return refuse("unknown trivial function"),
    };
    Ok(returned(&p, value))
}

fn proxy_config(args: &Value, host: ScriptedHost) -> Result<Value> {
    check_keys(args, &["url", "host", "username", "password"], "args")?;
    let url = arg_units(args, "url")?;
    let mut cfg = ProxyConfig {
        host: arg_units(args, "host")?,
        username: arg_units(args, "username")?,
        password: arg_units(args, "password")?,
    };
    let mut p = platform(host);
    let value = p
        .get_system_proxy_config(&url, &mut cfg)
        .map_err(port_error)?;
    let mut extra = Map::new();
    extra.insert("returned".into(), Value::from(value));
    extra.insert("host".into(), units(&cfg.host));
    extra.insert("username".into(), units(&cfg.username));
    extra.insert("password".into(), units(&cfg.password));
    Ok(result(&p, extra))
}

fn mapped_file(args: &Value, host: ScriptedHost) -> Result<Value> {
    check_keys(args, &["name"], "args")?;
    let name = arg_units(args, "name")?;
    let mut p = platform(host);
    let mut extra = Map::new();
    match p.mapped_file_open(&name).map_err(port_error)? {
        Ok(file) => {
            let data = file
                .data()
                .map(|bytes| hex(bytes.get(..file.size()).unwrap_or(bytes)));
            let points_to = match file.data_member() {
                MappedData::Mapping { address, .. } => p.host.name(Some(Ptr(*address))),
                MappedData::FallbackBuffer if file.data().is_some() => {
                    Value::from("fallback_buffer")
                }
                _ => Value::Null,
            };
            extra.insert("data".into(), data.map_or(Value::Null, Value::from));
            extra.insert("size".into(), Value::from(file.size()));
            extra.insert("is_mapped".into(), Value::from(file.is_mapped()));
            extra.insert("points_to".into(), points_to);
            extra.insert("fallback".into(), Value::from(hex(file.fallback_buffer())));
            p.mapped_file_close(file).map_err(port_error)?;
        }
        Err(error) => {
            extra.insert("exception".into(), Value::from(hex(&error.what)));
        }
    }
    Ok(result(&p, extra))
}

fn secrets(op: &str, args: &Value, host: ScriptedHost) -> Result<Value> {
    match op {
        "secrets_store" => {
            check_keys(args, &["service", "key", "secret"], "args")?;
            let (service, key, secret) = (
                arg_units(args, "service")?,
                arg_units(args, "key")?,
                arg_units(args, "secret")?,
            );
            let mut p = platform(host);
            let value = p
                .store_secret(&service, &key, &secret)
                .map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "secrets_store_async" => {
            check_keys(
                args,
                &["service", "key", "secret", "done", "complete"],
                "args",
            )?;
            let (service, key, secret) = (
                arg_units(args, "service")?,
                arg_units(args, "key")?,
                arg_units(args, "secret")?,
            );
            let has_done = read_bool(&args["done"], "done")?;
            let complete = read_bool(&args["complete"], "complete")?;
            let mut p = platform(host);
            let calls = p.host.calls.clone();
            let done: StoreDone = has_done.then(|| {
                Box::new(move |stored: bool| {
                    calls
                        .borrow_mut()
                        .push(json!({"fn": "done", "stored": stored}));
                }) as Box<dyn FnMut(bool)>
            });
            p.store_secret_async(&service, &key, &secret, done)
                .map_err(port_error)?;
            if complete {
                p.host.complete_pending(1).map_err(port_error)?;
            }
            Ok(returned(&p, Value::Null))
        }
        "secrets_get" | "secrets_get_no_prompt" => {
            check_keys(args, &["service", "key", "secret"], "args")?;
            let (service, key) = (arg_units(args, "service")?, arg_units(args, "key")?);
            let mut secret = arg_units(args, "secret")?;
            let mut p = platform(host);
            let value = if op == "secrets_get" {
                p.get_secret(&service, &key, &mut secret)
            } else {
                p.get_secret_no_prompt(&service, &key, &mut secret)
            }
            .map_err(port_error)?;
            let mut extra = Map::new();
            extra.insert("returned".into(), Value::from(value));
            extra.insert("secret".into(), units(&secret));
            Ok(result(&p, extra))
        }
        "secrets_get_async" => {
            check_keys(args, &["service", "key", "done", "complete"], "args")?;
            let (service, key) = (arg_units(args, "service")?, arg_units(args, "key")?);
            let has_done = read_bool(&args["done"], "done")?;
            let complete = read_bool(&args["complete"], "complete")?;
            let mut p = platform(host);
            let calls = p.host.calls.clone();
            let done: GetDone = has_done.then(|| {
                Box::new(move |found: bool, value: &WxString| {
                    calls
                        .borrow_mut()
                        .push(json!({"fn": "done", "found": found, "value": units(value)}));
                }) as Box<dyn FnMut(bool, &WxString)>
            });
            p.get_secret_async(&service, &key, done)
                .map_err(port_error)?;
            if complete {
                match p.host.complete_pending(1) {
                    Ok(()) => {}
                    Err(UnixError::BadFunctionCall) => {
                        let mut extra = Map::new();
                        extra.insert("returned".into(), Value::Null);
                        extra.insert("exception".into(), Value::from("bad_function_call"));
                        return Ok(result(&p, extra));
                    }
                    Err(error) => return Err(port_error(error)),
                }
            }
            Ok(returned(&p, Value::Null))
        }
        _ => {
            check_keys(args, &["service", "key"], "args")?;
            let (service, key) = (arg_units(args, "service")?, arg_units(args, "key")?);
            let mut p = platform(host);
            let value = p.delete_secret(&service, &key).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
    }
}

struct PrintCall {
    file: Vec<u8>,
    page: Option<i32>,
    fit: Option<bool>,
}

fn print_pdf(args: &Value, host: ScriptedHost) -> Result<Value> {
    check_keys(args, &["calls"], "args")?;
    let Some(calls) = args["calls"]
        .as_array()
        .filter(|c| !c.is_empty() && c.len() <= 8)
    else {
        return refuse("expected 1 to 8 print calls");
    };
    let mut parsed = Vec::new();
    for call in calls {
        check_keys(call, &["file", "current_page", "fit"], "print call")?;
        let file = read_hex(&call["file"], "file")?;
        let page = if call["current_page"].is_null() {
            None
        } else {
            Some(read_integer(
                &call["current_page"],
                "current_page",
                i64::from(i32::MIN),
                i64::from(i32::MAX),
            )? as i32)
        };
        let fit = if call["fit"].is_null() {
            None
        } else {
            Some(read_bool(&call["fit"], "fit")?)
        };
        if fit.is_some() && page.is_none() {
            return refuse("the fit overload has no default page");
        }
        parsed.push(PrintCall { file, page, fit });
    }
    // printing.cpp's s_lastSettings starts null: a fresh platform.
    let mut p = platform(host);
    let mut values = Vec::new();
    for call in &parsed {
        p.host.current_fit = call.fit.unwrap_or(true);
        let value = match (call.fit, call.page) {
            (Some(fit), Some(page)) => p.print_pdf_fit(&call.file, fit, page),
            (None, Some(page)) => p.print_pdf(&call.file, page),
            _ => p.print_pdf_without_current_page(&call.file),
        }
        .map_err(port_error)?;
        values.push(Value::from(value.value()));
    }
    let mut settings: Vec<(String, Value)> = p
        .host
        .settings
        .keys()
        .map(|&id| {
            let label = p.host.name(Some(Ptr(id)));
            (
                label.as_str().unwrap_or_default().to_owned(),
                p.host.settings_json(Ptr(id)),
            )
        })
        .collect();
    settings.sort_by(|a, b| a.0.cmp(&b.0));
    let mut extra = Map::new();
    extra.insert("returned".into(), Value::from(values));
    extra.insert(
        "settings".into(),
        Value::from(
            settings
                .into_iter()
                .map(|(label, entries)| json!([label, entries]))
                .collect::<Vec<_>>(),
        ),
    );
    Ok(result(&p, extra))
}

fn reset_print_to_file_path(args: &Value, host: ScriptedHost) -> Result<Value> {
    check_keys(args, &["filename", "native", "settings"], "args")?;
    let filename = arg_units(args, "filename")?;
    let native = read_text(&args["native"], "native")?;
    if native != "gtk" && native != "other" {
        return refuse("unknown native data");
    }
    let entries = read_settings(&args["settings"], "settings")?;
    let mut p = platform(host);
    let config = if native == "gtk" {
        Some(p.host.new_settings(entries).map_err(port_error)?)
    } else {
        None
    };
    let mut data = WxPrintData {
        filename,
        native: match config {
            Some(config) => PrintNativeData::Gtk {
                config: Some(config),
            },
            None => PrintNativeData::Other,
        },
    };
    p.reset_print_to_file_path(&mut data).map_err(port_error)?;
    let mut extra = Map::new();
    extra.insert("returned".into(), Value::Null);
    extra.insert("filename".into(), units(&data.filename));
    extra.insert(
        "settings".into(),
        config.map_or(Value::Null, |c| p.host.settings_json(c)),
    );
    Ok(result(&p, extra))
}

/// Runs `TerminateWithCode( code )` in a child process and returns its exit status, or why it
/// cannot (`None`: no child process is available, as in wasm32).
pub type Terminate<'a> = Option<&'a dyn Fn(i32) -> core::result::Result<i32, String>>;

/// `TerminateWithCode` in a child process: its exit status.
fn terminate_with_code(args: &Value, terminate: Terminate<'_>) -> Result<Value> {
    check_keys(args, &["code"], "args")?;
    let code = read_integer(
        &args["code"],
        "code",
        i64::from(i32::MIN),
        i64::from(i32::MAX),
    )? as i32;
    let Some(terminate) = terminate else {
        return Err(Refusal::Execution(
            "process termination is observed only in a native child process".into(),
        ));
    };
    let status = terminate(code).map_err(Refusal::Execution)?;
    Ok(json!({"calls": [], "exit_status": status}))
}

/// Answer one request's input.
pub fn run(input: &Value, terminate: Terminate<'_>) -> Result<Value> {
    check_keys(input, &["op", "args", "host"], "input")?;
    let op = read_text(&input["op"], "op")?;
    let args = &input["args"];
    let host = ScriptedHost::new(&input["host"])?;
    let simple = |args: &Value, fields: &[&str]| check_keys(args, fields, "args");
    match op {
        "trivial" => trivial(args, host),
        "app_init" => {
            simple(args, &[])?;
            let mut p = platform(host);
            let value = AppPlatform::init(&mut p).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "app_set_current_thread_priority" => {
            simple(args, &["raise"])?;
            let raise = read_bool(&args["raise"], "raise")?;
            let mut p = platform(host);
            let value = p.set_current_thread_priority(raise).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "app_terminate_with_code" => terminate_with_code(args, terminate),
        "build_config" => {
            simple(args, &[])?;
            Ok(json!({
                "calls": [],
                "wx_at_least_3_3_2": REFERENCE_BUILD.wx_at_least_3_3_2,
                "wx_has_egl": REFERENCE_BUILD.wx_has_egl,
                "wx_use_glcanvas_egl": REFERENCE_BUILD.wx_use_glcanvas_egl,
            }))
        }
        "env_init" => {
            simple(args, &[])?;
            let mut p = platform(host);
            EnvPlatform::init(&mut p).map_err(port_error)?;
            Ok(returned(&p, Value::Null))
        }
        "env_move_to_trash" => {
            simple(args, &["path", "error"])?;
            let path = arg_units(args, "path")?;
            let mut error = arg_units(args, "error")?;
            let mut p = platform(host);
            let value = p.move_to_trash(&path, &mut error).map_err(port_error)?;
            let mut extra = Map::new();
            extra.insert("returned".into(), Value::from(value));
            extra.insert("error".into(), units(&error));
            Ok(result(&p, extra))
        }
        "env_is_removable_path" => {
            simple(args, &["path"])?;
            let path = arg_units(args, "path")?;
            let mut p = platform(host);
            let value = p.is_removable_path(&path).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "env_get_documents_path"
        | "env_get_user_config_path"
        | "env_get_user_data_path"
        | "env_get_user_local_data_path"
        | "env_get_user_cache_path" => {
            simple(args, &[])?;
            let mut p = platform(host);
            let value = match op {
                "env_get_documents_path" => p.get_documents_path(),
                "env_get_user_config_path" => p.get_user_config_path(),
                "env_get_user_data_path" => p.get_user_data_path(),
                "env_get_user_local_data_path" => p.get_user_local_data_path(),
                _ => p.get_user_cache_path(),
            }
            .map_err(port_error)?;
            Ok(returned(&p, units(&value)))
        }
        "env_get_system_proxy_config" => proxy_config(args, host),
        "io_seq_f_open" => {
            simple(args, &["path", "mode"])?;
            let path = arg_units(args, "path")?;
            let mode = arg_units(args, "mode")?;
            let mut p = platform(host);
            let fp = p.seq_f_open(&path, &mode).map_err(port_error)?;
            let name = p.host.name(fp);
            Ok(returned(&p, name))
        }
        "io_duplicate_permissions" => {
            simple(args, &["src", "dest"])?;
            let src = arg_units(args, "src")?;
            let dest = arg_units(args, "dest")?;
            let mut p = platform(host);
            let value = p.duplicate_permissions(&src, &dest).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "io_make_writeable" | "io_is_file_hidden" => {
            simple(args, &["path"])?;
            let path = arg_units(args, "path")?;
            let mut p = platform(host);
            let value = if op == "io_make_writeable" {
                p.make_writeable(&path)
            } else {
                p.is_file_hidden(&path)
            }
            .map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "io_timestamp_dir" => {
            simple(args, &["dir", "spec"])?;
            let dir = arg_units(args, "dir")?;
            let spec = arg_units(args, "spec")?;
            let mut p = platform(host);
            let value = p.timestamp_dir(&dir, &spec).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "io_flush_to_disk" => {
            simple(args, &["null"])?;
            let null = read_bool(&args["null"], "null")?;
            let mut p = platform(host);
            let mut fp = if null {
                None
            } else {
                Some(p.host.new_handle("FILE").map_err(port_error)?)
            };
            let value = p.flush_to_disk(fp.as_mut()).map_err(port_error)?;
            Ok(returned(&p, Value::from(value)))
        }
        "io_mapped_file" => mapped_file(args, host),
        "secrets_store"
        | "secrets_store_async"
        | "secrets_get"
        | "secrets_get_async"
        | "secrets_get_no_prompt"
        | "secrets_delete" => secrets(op, args, host),
        "printing_print_pdf" => print_pdf(args, host),
        "printing_reset" => reset_print_to_file_path(args, host),
        _ => refuse("unknown operation"),
    }
}
