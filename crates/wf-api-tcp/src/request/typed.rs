//! Typed-map request bodies: `FLOW.CREATE_MANY`, `FLOW.CREATE` and
//! `FLOW.COMPLETE`.

use crate::codec::Value;

use super::{Complete, Create, CreateItem, CreateMany, RequestError};

const CREATE_MANY_KEYS: &[&[u8]] = &[
    b"type",
    b"state",
    b"now_ms",
    b"run_at_ms",
    b"priority",
    b"independent",
    b"return",
    b"items",
    b"partition_key",
];
const CREATE_KEYS: &[&[u8]] = &[
    b"id",
    b"type",
    b"state",
    b"now_ms",
    b"payload",
    b"run_at_ms",
    b"partition_key",
    b"priority",
];
const COMPLETE_KEYS: &[&[u8]] = &[
    b"id",
    b"lease_token",
    b"fencing_token",
    b"now_ms",
    b"partition_key",
];

/// The entries of a top-level request map whose keys were all checked.
struct Fields {
    command: &'static str,
    entries: Vec<(Vec<u8>, Value)>,
}

impl Fields {
    fn decode(command: &'static str, body: &[u8], allowed: &[&[u8]]) -> Result<Self, RequestError> {
        let value = Value::decode_exact(body).map_err(|error| {
            RequestError::new(format!("ERR native {command} payload is invalid: {error}"))
        })?;
        let Value::Map(entries) = value else {
            return Err(RequestError::new(format!(
                "ERR native {command} payload must be a map"
            )));
        };
        if let Some((key, _)) = entries
            .iter()
            .find(|(key, _)| !allowed.contains(&key.as_slice()))
        {
            return Err(RequestError::new(format!(
                "ERR {command} option {} is not supported by this server",
                String::from_utf8_lossy(key)
            )));
        }
        Ok(Self { command, entries })
    }

    fn get(&self, key: &[u8]) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(name, _)| name.as_slice() == key)
            .map(|(_, value)| value)
            .filter(|value| **value != Value::Nil)
    }

    fn wrong_type(&self, key: &[u8], expected: &str) -> RequestError {
        RequestError::new(format!(
            "ERR {} {} must be {expected}",
            self.command,
            String::from_utf8_lossy(key)
        ))
    }

    fn bytes(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RequestError> {
        self.get(key)
            .map(|value| {
                value
                    .as_bytes()
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| self.wrong_type(key, "binary"))
            })
            .transpose()
    }

    fn required_bytes(&self, key: &[u8]) -> Result<Vec<u8>, RequestError> {
        self.bytes(key)?.ok_or_else(|| {
            RequestError::new(format!(
                "ERR {} requires {}",
                self.command,
                String::from_utf8_lossy(key)
            ))
        })
    }

    fn integer(&self, key: &[u8]) -> Result<Option<i64>, RequestError> {
        self.get(key)
            .map(|value| {
                value
                    .as_i64()
                    .ok_or_else(|| self.wrong_type(key, "an integer"))
            })
            .transpose()
    }

    fn flag(&self, key: &[u8]) -> Result<Option<bool>, RequestError> {
        self.get(key)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| self.wrong_type(key, "a boolean"))
            })
            .transpose()
    }
}

/// Parses a `FLOW.CREATE_MANY` body.
///
/// # Errors
/// Refuses a malformed body, an unsupported option or return mode, a missing
/// type, state or items list, and a field of the wrong type.
pub fn create_many(body: &[u8]) -> Result<CreateMany, RequestError> {
    let fields = Fields::decode("FLOW.CREATE_MANY", body, CREATE_MANY_KEYS)?;
    let ok_on_success = match fields.get(b"return") {
        None => false,
        Some(value) if value.as_bytes() == Some(b"ok_on_success".as_slice()) => true,
        Some(_) => {
            return Err(RequestError::new(
                "ERR FLOW.CREATE_MANY return mode is not supported by this server",
            ));
        }
    };
    let Some(Value::Arr(raw_items)) = fields.get(b"items") else {
        return Err(RequestError::new("ERR FLOW.CREATE_MANY requires items"));
    };
    let items = raw_items
        .iter()
        .map(create_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CreateMany {
        flow_type: fields.required_bytes(b"type")?,
        state: fields.required_bytes(b"state")?,
        now_ms: fields.integer(b"now_ms")?,
        run_at_ms: fields.integer(b"run_at_ms")?,
        priority: fields.integer(b"priority")?.unwrap_or(0),
        independent: fields.flag(b"independent")?.unwrap_or(false),
        ok_on_success,
        partition_key: fields.bytes(b"partition_key")?,
        items,
    })
}

fn create_item(item: &Value) -> Result<CreateItem, RequestError> {
    let invalid = |what: &str| RequestError::new(format!("ERR FLOW.CREATE_MANY item {what}"));
    let id = |value: &Value| {
        value
            .as_bytes()
            .map(<[u8]>::to_vec)
            .ok_or_else(|| invalid("id must be binary"))
    };
    let payload = |value: &Value| match value {
        Value::Nil => Ok(None),
        Value::Bin(bytes) => Ok(Some(bytes.clone())),
        _ => Err(invalid("payload must be binary")),
    };
    let Value::Arr(parts) = item else {
        return Err(invalid("must be a list"));
    };
    match parts.as_slice() {
        [flow_id] => Ok(CreateItem {
            id: id(flow_id)?,
            partition_key: None,
            payload: None,
        }),
        [flow_id, data] => Ok(CreateItem {
            id: id(flow_id)?,
            partition_key: None,
            payload: payload(data)?,
        }),
        [flow_id, partition, data] => Ok(CreateItem {
            id: id(flow_id)?,
            partition_key: Some(
                partition
                    .as_bytes()
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| invalid("partition must be binary"))?,
            ),
            payload: payload(data)?,
        }),
        _ => Err(invalid("must have one to three fields")),
    }
}

/// Parses a `FLOW.CREATE` body.
///
/// # Errors
/// Refuses a malformed body, an unsupported option, a missing id, type or
/// state, and a field of the wrong type.
pub fn create(body: &[u8]) -> Result<Create, RequestError> {
    let fields = Fields::decode("FLOW.CREATE", body, CREATE_KEYS)?;
    Ok(Create {
        id: fields.required_bytes(b"id")?,
        flow_type: fields.required_bytes(b"type")?,
        state: fields.required_bytes(b"state")?,
        now_ms: fields.integer(b"now_ms")?,
        partition_key: fields.bytes(b"partition_key")?,
        payload: fields.bytes(b"payload")?,
        run_at_ms: fields.integer(b"run_at_ms")?,
        priority: fields.integer(b"priority")?.unwrap_or(0),
    })
}

/// Parses a `FLOW.COMPLETE` body.
///
/// # Errors
/// Refuses a malformed body, an unsupported option, a missing id, lease token
/// or fencing token, and a field of the wrong type.
pub fn complete(body: &[u8]) -> Result<Complete, RequestError> {
    let fields = Fields::decode("FLOW.COMPLETE", body, COMPLETE_KEYS)?;
    Ok(Complete {
        id: fields.required_bytes(b"id")?,
        partition_key: fields.bytes(b"partition_key")?,
        lease_token: fields.required_bytes(b"lease_token")?,
        fencing_token: fields
            .integer(b"fencing_token")?
            .ok_or_else(|| RequestError::new("ERR FLOW.COMPLETE requires fencing_token"))?,
        now_ms: fields.integer(b"now_ms")?,
    })
}
