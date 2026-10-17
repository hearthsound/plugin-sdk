//! Shared helpers for conformance checks: calling exports, reading provider
//! results and matching ids. Every helper returns a failure detail as a
//! `String`, which the [`Report`](super::Report) records as the check's error.

use serde_json::Value;

use crate::run::Session;

/// The outcome of one provider call: its success value or its provider error.
pub(super) enum Reply {
    /// The function returned its success value.
    Ok(Value),
    /// The function returned a provider error, carried as its variant.
    Err(Value),
}

/// Calls one export and splits its result into success and provider error.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `layer`: the layer interface the function belongs to.
/// - `function`: the function name in WIT kebab-case.
/// - `input`: the argument, or `None` for a function without arguments.
///
/// # Errors
///
/// Returns the mock host's message when the call traps or the export is
/// missing, and a message when the reply has neither an `ok` nor an `error`
/// member.
pub(super) fn call(
    session: &mut Session,
    layer: &str,
    function: &str,
    input: Option<Value>,
) -> Result<Reply, String> {
    let reply = session.call(layer, function, input)?;
    if let Some(value) = reply.get("ok") {
        return Ok(Reply::Ok(value.clone()));
    }
    if let Some(error) = reply.get("error") {
        return Ok(Reply::Err(error.clone()));
    }
    Err(format!(
        "`{function}` produced an unexpected reply: {reply}"
    ))
}

/// Calls one export and returns its success value.
///
/// # Errors
///
/// Returns a message naming the provider error when the function returned one,
/// or the error from [`call`].
pub(super) fn expect_ok(
    session: &mut Session,
    layer: &str,
    function: &str,
    input: Option<Value>,
) -> Result<Value, String> {
    match call(session, layer, function, input)? {
        Reply::Ok(value) => Ok(value),
        Reply::Err(error) => Err(format!(
            "`{function}` returned provider error {}",
            error_name(&error)
        )),
    }
}

/// Names a provider error: the variant of a unit variant, or the key of a
/// variant that carries a payload.
pub(super) fn error_name(error: &Value) -> String {
    match error {
        Value::String(name) => name.clone(),
        Value::Object(map) => map
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| error.to_string()),
        other => other.to_string(),
    }
}

/// Accepts a provider error whose variant is one of `allowed`.
///
/// # Errors
///
/// Returns a message naming the error when its variant is not allowed.
pub(super) fn named_error(error: &Value, allowed: &[&str]) -> Result<(), String> {
    let name = error_name(error);
    if allowed.contains(&name.as_str()) {
        Ok(())
    } else {
        Err(format!(
            "provider error `{name}` is not one of `{}`",
            allowed.join("`, `")
        ))
    }
}

/// Accepts a list reply, or a provider error whose variant is one of `allowed`.
///
/// # Errors
///
/// Returns a message when the reply is neither a list nor an allowed error.
pub(super) fn list_or_error(reply: Reply, allowed: &[&str]) -> Result<(), String> {
    match reply {
        Reply::Ok(value) => items(&value).map(|_| ()),
        Reply::Err(error) => named_error(&error, allowed),
    }
}

/// Returns the elements of a JSON array.
///
/// # Errors
///
/// Returns a message when the value is not an array.
pub(super) fn items(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("expected a list, got {value}"))
}

/// Returns the array stored under a member of a JSON object.
///
/// # Errors
///
/// Returns a message when the member is missing or is not an array.
pub(super) fn field<'a>(value: &'a Value, name: &str) -> Result<&'a [Value], String> {
    value
        .get(name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("`{name}` is not a list"))
}

/// Collects the `id` member of each record in a list.
///
/// # Errors
///
/// Returns a message naming the record when one has no string `id`.
pub(super) fn ids(records: &[Value]) -> Result<Vec<String>, String> {
    records
        .iter()
        .map(|record| {
            record
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("record has no id: {record}"))
        })
        .collect()
}

/// Checks that a record's `id` member is the id that was requested.
///
/// # Errors
///
/// Returns a message when the record has no id or a different one.
pub(super) fn expect_id(record: &Value, id: &str) -> Result<(), String> {
    match record.get("id").and_then(Value::as_str) {
        Some(found) if found == id => Ok(()),
        Some(found) => Err(format!("asked for `{id}`, got `{found}`")),
        None => Err(format!("record for `{id}` has no id")),
    }
}

/// Returns the first id in a list, failing when the list is empty.
///
/// # Errors
///
/// Returns a message naming `what` when `found` is empty.
pub(super) fn first<'a>(found: &'a [String], what: &str) -> Result<&'a str, String> {
    found
        .first()
        .map(String::as_str)
        .ok_or_else(|| format!("search returned no {what}"))
}
