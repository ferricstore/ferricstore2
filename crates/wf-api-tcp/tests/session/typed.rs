//! A minimal FSNP typed-value encoder and decoder for the tests.

use std::collections::BTreeMap;

use super::TestResult;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Typed {
    Nil,
    Bool(bool),
    Int(i64),
    Bin(Vec<u8>),
    List(Vec<Typed>),
    Map(BTreeMap<String, Typed>),
}

pub(crate) fn bin(text: &str) -> Typed {
    Typed::Bin(text.as_bytes().to_vec())
}

pub(crate) fn map(entries: &[(&str, Typed)]) -> Typed {
    Typed::Map(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

fn length(out: &mut Vec<u8>, length: usize) -> TestResult {
    out.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
    Ok(())
}

pub(crate) fn encode(value: &Typed) -> TestResult<Vec<u8>> {
    let mut out = Vec::new();
    write(value, &mut out)?;
    Ok(out)
}

fn write(value: &Typed, out: &mut Vec<u8>) -> TestResult {
    match value {
        Typed::Nil => out.push(0),
        Typed::Bool(true) => out.push(1),
        Typed::Bool(false) => out.push(2),
        Typed::Int(number) => {
            out.push(3);
            out.extend_from_slice(&number.to_be_bytes());
        }
        Typed::Bin(bytes) => {
            out.push(4);
            length(out, bytes.len())?;
            out.extend_from_slice(bytes);
        }
        Typed::List(items) => {
            out.push(5);
            length(out, items.len())?;
            for item in items {
                write(item, out)?;
            }
        }
        Typed::Map(entries) => {
            out.push(6);
            length(out, entries.len())?;
            for (key, value) in entries {
                length(out, key.len())?;
                out.extend_from_slice(key.as_bytes());
                write(value, out)?;
            }
        }
    }
    Ok(())
}

/// Decodes a reply body: the status and its typed value.
pub(crate) fn reply(body: &[u8]) -> TestResult<(u16, Typed)> {
    let (status, rest) = body.split_at_checked(2).ok_or("short reply")?;
    let mut input = rest;
    let value = read(&mut input)?;
    assert!(input.is_empty(), "trailing reply bytes");
    Ok((u16::from_be_bytes(status.try_into()?), value))
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> TestResult<&'a [u8]> {
    let (head, rest) = input.split_at_checked(count).ok_or("truncated value")?;
    *input = rest;
    Ok(head)
}

fn read_length(input: &mut &[u8]) -> TestResult<usize> {
    Ok(usize::try_from(u32::from_be_bytes(
        take(input, 4)?.try_into()?,
    ))?)
}

fn read(input: &mut &[u8]) -> TestResult<Typed> {
    let tag = *take(input, 1)?.first().ok_or("empty value")?;
    Ok(match tag {
        0 => Typed::Nil,
        1 => Typed::Bool(true),
        2 => Typed::Bool(false),
        3 => Typed::Int(i64::from_be_bytes(take(input, 8)?.try_into()?)),
        4 => {
            let count = read_length(input)?;
            Typed::Bin(take(input, count)?.to_vec())
        }
        5 => {
            let count = read_length(input)?;
            Typed::List((0..count).map(|_| read(input)).collect::<TestResult<_>>()?)
        }
        6 => {
            let count = read_length(input)?;
            let mut entries = BTreeMap::new();
            for _ in 0..count {
                let key_length = read_length(input)?;
                let key = String::from_utf8(take(input, key_length)?.to_vec())?;
                // Clients refuse a map that repeats a key, as the SDKs do.
                if entries.insert(key, read(input)?).is_some() {
                    return Err("duplicate map key".into());
                }
            }
            Typed::Map(entries)
        }
        other => return Err(format!("unexpected tag {other}").into()),
    })
}

/// Follows a path of map keys.
pub(crate) fn at<'a>(value: &'a Typed, path: &[&str]) -> TestResult<&'a Typed> {
    path.iter().try_fold(value, |current, key| match current {
        Typed::Map(entries) => entries
            .get(*key)
            .ok_or_else(|| format!("missing {key}").into()),
        _ => Err(format!("{key} is not inside a map").into()),
    })
}

/// The text items of a list.
pub(crate) fn texts(value: &Typed) -> TestResult<Vec<String>> {
    match value {
        Typed::List(items) => items
            .iter()
            .map(|item| match item {
                Typed::Bin(bytes) => Ok(String::from_utf8(bytes.clone())?),
                _ => Err("non-text list item".into()),
            })
            .collect(),
        _ => Err("not a list".into()),
    }
}
