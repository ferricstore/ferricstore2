//! FSNP typed values and the bounded reader shared by the request parsers and
//! reply encoders (ADR-011). Everything here reads untrusted bytes, so no
//! read can index past its input and every declared length is checked against
//! the bytes that remain before anything is allocated.

mod reader;
mod value;

pub(crate) use reader::Reader;
pub(crate) use value::{TooLong, Value, put_bin, put_len};

#[cfg(test)]
mod tests;
