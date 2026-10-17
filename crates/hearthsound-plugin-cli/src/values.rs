//! Conversion between JSON documents and component-model values.
//!
//! Inputs are JSON shaped by the WIT types of the function being called.
//! Record fields, variant cases, enum cases and flags use snake_case names in
//! JSON; the kebab-case WIT names are accepted as well. Outputs use the same
//! snake_case names, so the `not-found` provider error prints as `not_found`.

use serde_json::{Map, Value, json};
use wasmtime::component::{Type, Val};

/// Converts a WIT identifier to its JSON spelling: kebab-case becomes
/// snake_case.
///
/// # Parameters
///
/// - `name`: the WIT identifier, for example `get-track`.
///
/// # Returns
///
/// The snake_case spelling, for example `get_track`.
///
/// # Example
///
/// ```ignore
/// assert_eq!(snake("not-found"), "not_found");
/// ```
pub fn snake(name: &str) -> String {
    name.replace('-', "_")
}

/// Looks up a JSON member by WIT name, trying the snake_case spelling first.
fn member<'a>(object: &'a Map<String, Value>, wit_name: &str) -> Option<&'a Value> {
    object
        .get(&snake(wit_name))
        .or_else(|| object.get(wit_name))
}

/// Builds the argument list for a function from the JSON input.
///
/// A function with no parameters takes no input. A function with exactly one
/// parameter takes the input as the bare value. A function with several
/// parameters takes a JSON object with one member per parameter.
///
/// # Parameters
///
/// - `params`: the parameter names and types, in declaration order.
/// - `input`: the parsed `--input` document, if one was given.
///
/// # Returns
///
/// The converted arguments, ready for a component call.
///
/// # Errors
///
/// Returns a message when the input is missing, unexpected, or does not match
/// the parameter types.
pub fn arguments(params: &[(String, Type)], input: Option<Value>) -> Result<Vec<Val>, String> {
    match (params, input) {
        ([], None) => Ok(Vec::new()),
        ([], Some(_)) => Err("this function takes no input; omit --input".into()),
        (_, None) => Err("this function requires --input with its arguments".into()),
        ([(_, ty)], Some(value)) => Ok(vec![json_to_val(&value, ty)?]),
        (params, Some(value)) => {
            let object = value
                .as_object()
                .ok_or_else(|| "expected a JSON object with one member per argument".to_string())?;
            params
                .iter()
                .map(|(name, ty)| {
                    let item = member(object, name)
                        .ok_or_else(|| format!("missing argument `{}`", snake(name)))?;
                    json_to_val(item, ty)
                })
                .collect()
        }
    }
}

/// Converts one JSON value into a component value of the given type.
///
/// # Parameters
///
/// - `value`: the JSON value to convert.
/// - `ty`: the WIT type the value must have.
///
/// # Returns
///
/// The component value.
///
/// # Errors
///
/// Returns a message naming the first mismatch between the JSON and the type.
pub fn json_to_val(value: &Value, ty: &Type) -> Result<Val, String> {
    Ok(match ty {
        Type::Bool => Val::Bool(value.as_bool().ok_or_else(|| expected("a boolean"))?),
        Type::S8 => Val::S8(signed(value, "an 8-bit signed integer")?),
        Type::U8 => Val::U8(unsigned(value, "an 8-bit unsigned integer")?),
        Type::S16 => Val::S16(signed(value, "a 16-bit signed integer")?),
        Type::U16 => Val::U16(unsigned(value, "a 16-bit unsigned integer")?),
        Type::S32 => Val::S32(signed(value, "a 32-bit signed integer")?),
        Type::U32 => Val::U32(unsigned(value, "a 32-bit unsigned integer")?),
        Type::S64 => Val::S64(signed(value, "a 64-bit signed integer")?),
        Type::U64 => Val::U64(unsigned(value, "a 64-bit unsigned integer")?),
        Type::Float32 => Val::Float32(float(value)? as f32),
        Type::Float64 => Val::Float64(float(value)?),
        Type::Char => {
            let text = string(value)?;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Val::Char(c),
                _ => return Err("expected a single character".into()),
            }
        }
        Type::String => Val::String(string(value)?.to_string()),
        Type::List(list) => {
            let element = list.ty();
            Val::List(
                array(value)?
                    .iter()
                    .map(|item| json_to_val(item, &element))
                    .collect::<Result<_, _>>()?,
            )
        }
        Type::Record(record) => {
            let object = object(value)?;
            let mut fields = Vec::new();
            for field in record.fields() {
                let item = member(object, field.name)
                    .ok_or_else(|| format!("missing field `{}`", snake(field.name)))?;
                fields.push((field.name.to_string(), json_to_val(item, &field.ty)?));
            }
            Val::Record(fields)
        }
        Type::Tuple(tuple) => {
            let types: Vec<Type> = tuple.types().collect();
            let items = array(value)?;
            if items.len() != types.len() {
                return Err(format!("expected a tuple of {} members", types.len()));
            }
            Val::Tuple(
                items
                    .iter()
                    .zip(&types)
                    .map(|(item, ty)| json_to_val(item, ty))
                    .collect::<Result<_, _>>()?,
            )
        }
        Type::Variant(variant) => variant_value(value, variant)?,
        Type::Enum(enumeration) => {
            let name = string(value)?;
            let case = enumeration
                .names()
                .find(|case| snake(case) == name || *case == name)
                .ok_or_else(|| format!("unknown case `{name}`"))?;
            Val::Enum(case.to_string())
        }
        Type::Option(option) => {
            if value.is_null() {
                Val::Option(None)
            } else {
                Val::Option(Some(Box::new(json_to_val(value, &option.ty())?)))
            }
        }
        Type::Result(result) => {
            let object = object(value)?;
            if let Some(ok) = object.get("ok") {
                Val::Result(Ok(payload(ok, result.ok())?))
            } else if let Some(err) = object.get("error") {
                Val::Result(Err(payload(err, result.err())?))
            } else {
                return Err("expected an object with an `ok` or `error` member".into());
            }
        }
        Type::Flags(flags) => {
            let mut names = Vec::new();
            for item in array(value)? {
                let name = string(item)?;
                let flag = flags
                    .names()
                    .find(|flag| snake(flag) == name || *flag == name)
                    .ok_or_else(|| format!("unknown flag `{name}`"))?;
                names.push(flag.to_string());
            }
            Val::Flags(names)
        }
        _ => return Err("this value type cannot be supplied as JSON".into()),
    })
}

/// Converts a component value into JSON using snake_case names.
///
/// # Parameters
///
/// - `val`: the component value to convert.
///
/// # Returns
///
/// The JSON document. Values that have no JSON form, such as resources, become
/// `null`.
pub fn val_to_json(val: &Val) -> Value {
    match val {
        Val::Bool(value) => Value::Bool(*value),
        Val::S8(value) => Value::from(*value),
        Val::U8(value) => Value::from(*value),
        Val::S16(value) => Value::from(*value),
        Val::U16(value) => Value::from(*value),
        Val::S32(value) => Value::from(*value),
        Val::U32(value) => Value::from(*value),
        Val::S64(value) => Value::from(*value),
        Val::U64(value) => Value::from(*value),
        Val::Float32(value) => float_value(f64::from(*value)),
        Val::Float64(value) => float_value(*value),
        Val::Char(value) => Value::String(value.to_string()),
        Val::String(value) => Value::String(value.clone()),
        Val::List(items) | Val::Tuple(items) => {
            Value::Array(items.iter().map(val_to_json).collect())
        }
        Val::Record(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (snake(name), val_to_json(value)))
                .collect(),
        ),
        Val::Variant(case, None) => Value::String(snake(case)),
        Val::Variant(case, Some(payload)) => json!({ snake(case): val_to_json(payload) }),
        Val::Enum(case) => Value::String(snake(case)),
        Val::Option(None) => Value::Null,
        Val::Option(Some(value)) => val_to_json(value),
        Val::Result(Ok(value)) => {
            json!({ "ok": value.as_deref().map_or(Value::Null, val_to_json) })
        }
        Val::Result(Err(value)) => {
            json!({ "error": value.as_deref().map_or(Value::Null, val_to_json) })
        }
        Val::Flags(names) => Value::Array(
            names
                .iter()
                .map(|name| Value::String(snake(name)))
                .collect(),
        ),
        _ => Value::Null,
    }
}

/// Converts a variant from its JSON form: a bare case name for a unit case, or
/// a single-member object for a case with a payload.
fn variant_value(
    value: &Value,
    variant: &wasmtime::component::types::Variant,
) -> Result<Val, String> {
    let (name, body): (&str, Option<&Value>) = match value {
        Value::String(name) => (name.as_str(), None),
        Value::Object(object) if object.len() == 1 => match object.iter().next() {
            Some((name, body)) => (name.as_str(), Some(body)),
            None => return Err("expected a single-member object for a variant".into()),
        },
        _ => return Err("expected a case name or a single-member object".into()),
    };
    let case = variant
        .cases()
        .find(|case| snake(case.name) == name || case.name == name)
        .ok_or_else(|| format!("unknown case `{name}`"))?;
    let payload = match (case.ty, body) {
        (None, None) => None,
        (Some(ty), Some(body)) => Some(Box::new(json_to_val(body, &ty)?)),
        (None, Some(_)) => return Err(format!("case `{}` takes no payload", snake(case.name))),
        (Some(_), None) => return Err(format!("case `{}` requires a payload", snake(case.name))),
    };
    Ok(Val::Variant(case.name.to_string(), payload))
}

/// Converts the payload of a `result` arm, which may be absent for `_` types.
fn payload(value: &Value, ty: Option<Type>) -> Result<Option<Box<Val>>, String> {
    match ty {
        None if value.is_null() => Ok(None),
        None => Err("this result arm carries no value; use null".into()),
        Some(ty) => Ok(Some(Box::new(json_to_val(value, &ty)?))),
    }
}

/// Builds the error for a JSON value of the wrong kind.
fn expected(what: &str) -> String {
    format!("expected {what}")
}

/// Reads a signed integer that fits the target type.
fn signed<T: TryFrom<i64>>(value: &Value, what: &str) -> Result<T, String> {
    value
        .as_i64()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| expected(what))
}

/// Reads an unsigned integer that fits the target type.
fn unsigned<T: TryFrom<u64>>(value: &Value, what: &str) -> Result<T, String> {
    value
        .as_u64()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| expected(what))
}

/// Reads a floating-point number.
fn float(value: &Value) -> Result<f64, String> {
    value.as_f64().ok_or_else(|| expected("a number"))
}

/// Reads a JSON string.
fn string(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| expected("a string"))
}

/// Reads a JSON array.
fn array(value: &Value) -> Result<&Vec<Value>, String> {
    value.as_array().ok_or_else(|| expected("an array"))
}

/// Reads a JSON object.
fn object(value: &Value) -> Result<&Map<String, Value>, String> {
    value.as_object().ok_or_else(|| expected("an object"))
}

/// Converts a float to JSON; non-finite values have no JSON form and become
/// `null`.
fn float_value(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}
