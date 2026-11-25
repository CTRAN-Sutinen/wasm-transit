//! A document's canonical bytes under RFC 8785, the JSON Canonicalization
//! Scheme. Any implementation of the standard writes the same bytes
//! for the same value, so a document's identity can be checked in any
//! language without this repo.
//!
//! The writer never holds the whole document, because wasm memory never
//! shrinks back from a peak and a network is megabytes:
//!
//! - An object whose keys arrive in order (a `BTreeMap`, most maps) streams.
//! - An object with few keys out of order (a struct) is serialized once for
//!   each key, in canonical order. Each pass writes only that key's entry.
//! - An object with many keys out of order (rare) has its entries buffered
//!   and written sorted.
//!
//! The writer learns which way an object takes by writing it. Below the root
//! and its fields, a value is staged in a small buffer. There an object is
//! written whole and taken back if a key comes out of order. The root and
//! its fields hold the whole document, so a pass that does not descend into
//! the values reads their keys first.

use std::fmt;

use serde::ser::{self, Impossible, Serialize};

pub trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    #[inline]
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// Boxed, so a result passed up the writer's calls is one word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(Option<Box<String>>);

impl Error {
    fn new(message: impl Into<String>) -> Error {
        Error(Some(Box::new(message.into())))
    }

    /// The writer's own signal that an object written whole found a key out
    /// of order. It is never an answer, and it costs no allocation.
    fn unsorted() -> Error {
        Error(None)
    }

    fn is_unsorted(&self) -> bool {
        self.0.is_none()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_deref().map_or("a key out of order", String::as_str))
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error::new(msg.to_string())
    }
}

const PASSES_UP_TO: usize = 64;

const STAGE_FROM: u32 = 2;

thread_local! {
    static STAGED: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn write<T: Serialize + ?Sized, S: Sink + ?Sized>(value: &T, sink: &mut S) -> Result<(), Error> {
    emit(value, &mut DynSink(sink), 0)
}

pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    emit(value, &mut out, 0)?;
    Ok(out)
}

pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    Ok(String::from_utf8(to_vec(value)?).expect("canonical JSON is UTF-8"))
}

/// Orders keys by UTF-16 code units. Two ASCII keys compare as their bytes
/// do, which is the same order without the walk.
#[inline]
pub fn key_order(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a.as_bytes().cmp(b.as_bytes());
    }
    a.encode_utf16().cmp(b.encode_utf16())
}

type Key = std::borrow::Cow<'static, str>;

struct DynSink<'a, S: Sink + ?Sized>(&'a mut S);

/// A buffer can take back what it was given since a mark. A digest cannot.
trait Out {
    fn put(&mut self, bytes: &[u8]);
    #[inline]
    fn mark(&self) -> Option<usize> {
        None
    }
    fn rewind(&mut self, _to: usize) {}
}

impl<S: Sink + ?Sized> Out for DynSink<'_, S> {
    #[inline]
    fn put(&mut self, bytes: &[u8]) {
        self.0.put(bytes);
    }
}

impl Out for Vec<u8> {
    #[inline]
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
    #[inline]
    fn mark(&self) -> Option<usize> {
        Some(self.len())
    }
    #[inline]
    fn rewind(&mut self, to: usize) {
        self.truncate(to);
    }
}

fn emit<T: Serialize + ?Sized, O: Out + ?Sized>(value: &T, out: &mut O, depth: u32) -> Result<(), Error> {
    if let Some(mark) = out.mark() {
        return match value.serialize(Writer { out: &mut *out, only: None, depth }) {
            Err(e) if e.is_unsorted() => {
                out.rewind(mark);
                by_key(value, out, depth)
            }
            other => other,
        };
    }
    if depth >= STAGE_FROM {
        // Staging never nests, because a buffer takes back what it was
        // given. So one staging buffer is reused.
        let mut staged = STAGED.with(|s| std::mem::take(&mut *s.borrow_mut()));
        staged.clear();
        let written = emit(value, &mut staged, depth);
        out.put(&staged);
        STAGED.with(|s| *s.borrow_mut() = staged);
        return written;
    }
    match Probe::shape(value)? {
        Shape::Unsorted => by_key(value, out, depth),
        _ => value.serialize(Writer { out, only: None, depth }),
    }
}

fn by_key<T: Serialize + ?Sized, O: Out + ?Sized>(value: &T, out: &mut O, depth: u32) -> Result<(), Error> {
    let keys = Probe::keys(value)?;
    if keys.len() <= PASSES_UP_TO {
        let mut order: Vec<&Key> = keys.iter().collect();
        order.sort_by(|a, b| key_order(a, b));
        order.dedup();
        out.put(b"{");
        for (i, key) in order.into_iter().enumerate() {
            if i > 0 {
                out.put(b",");
            }
            value.serialize(Writer { out: &mut *out, only: Some(key), depth })?;
        }
        out.put(b"}");
        return Ok(());
    }
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(keys.len());
    value.serialize(Buffer { entries: &mut entries, depth })?;
    entries.sort_by(|a, b| key_order(&a.0, &b.0));
    out.put(b"{");
    for (i, (key, bytes)) in entries.iter().enumerate() {
        if i > 0 {
            out.put(b",");
        }
        string(key, out);
        out.put(b":");
        out.put(bytes);
    }
    out.put(b"}");
    Ok(())
}

#[inline]
fn string<O: Out + ?Sized>(s: &str, out: &mut O) {
    out.put(b"\"");
    let bytes = s.as_bytes();
    if !bytes.iter().any(|&b| b < 0x20 || b == b'"' || b == b'\\') {
        out.put(bytes);
        out.put(b"\"");
        return;
    }
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let escape: Option<&[u8]> = match b {
            b'"' => Some(b"\\\""),
            b'\\' => Some(b"\\\\"),
            0x08 => Some(b"\\b"),
            0x09 => Some(b"\\t"),
            0x0a => Some(b"\\n"),
            0x0c => Some(b"\\f"),
            0x0d => Some(b"\\r"),
            0x00..=0x1f => None,
            _ => continue,
        };
        out.put(&bytes[start..i]);
        match escape {
            Some(e) => out.put(e),
            None => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.put(&[b'\\', b'u', b'0', b'0', HEX[(b >> 4) as usize], HEX[(b & 0xf) as usize]]);
            }
        }
        start = i + 1;
    }
    out.put(&bytes[start..]);
    out.put(b"\"");
}

#[inline]
fn number<O: Out + ?Sized>(v: f64, out: &mut O) -> Result<(), Error> {
    if !v.is_finite() {
        return Err(Error::new(format!("the number {v} has no JSON form")));
    }
    if v == 0.0 {
        out.put(b"0");
        return Ok(());
    }
    let mut buffer = ryu_js::Buffer::new();
    out.put(buffer.format_finite(v).as_bytes());
    Ok(())
}

#[inline]
fn integer<I: itoa::Integer, O: Out + ?Sized>(v: I, out: &mut O) {
    let mut buffer = itoa::Buffer::new();
    out.put(buffer.format(v).as_bytes());
}

enum Shape {
    Other,
    Sorted,
    Unsorted,
    Keys(Vec<Key>),
}

/// Reads a value's shape without descending into it. With `collect` it keeps
/// the keys. Otherwise it only checks their order.
struct Probe {
    collect: bool,
}

struct Keys {
    collect: bool,
    keys: Vec<Key>,
    last: String,
    pending: String,
    seen: bool,
    sorted: bool,
}

impl Keys {
    fn new(collect: bool) -> Keys {
        Keys { collect, keys: Vec::new(), last: String::new(), pending: String::new(), seen: false, sorted: true }
    }

    fn push(&mut self, key: &str, field: Option<&'static str>) {
        if self.collect {
            self.keys.push(match field {
                Some(f) => Key::Borrowed(f),
                None => Key::Owned(key.to_string()),
            });
            return;
        }
        if self.sorted {
            if self.seen && !key_order(&self.last, key).is_lt() {
                self.sorted = false;
            }
            self.seen = true;
            self.last.clear();
            self.last.push_str(key);
        }
    }

    fn shape(self) -> Shape {
        match (self.collect, self.sorted) {
            (true, _) => Shape::Keys(self.keys),
            (false, true) => Shape::Sorted,
            (false, false) => Shape::Unsorted,
        }
    }
}

impl ser::Serializer for Probe {
    type Ok = Shape;
    type Error = ProbeError;
    type SerializeSeq = Impossible<Shape, ProbeError>;
    type SerializeTuple = Impossible<Shape, ProbeError>;
    type SerializeTupleStruct = Impossible<Shape, ProbeError>;
    type SerializeTupleVariant = Impossible<Shape, ProbeError>;
    type SerializeMap = Keys;
    type SerializeStruct = Keys;
    type SerializeStructVariant = Impossible<Shape, ProbeError>;

    fn serialize_bool(self, _: bool) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_i8(self, _: i8) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_i16(self, _: i16) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_i32(self, _: i32) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_i64(self, _: i64) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_u8(self, _: u8) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_u16(self, _: u16) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_u32(self, _: u32) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_u64(self, _: u64) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_f32(self, _: f32) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_f64(self, _: f64) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_char(self, _: char) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_str(self, _: &str) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_none(self) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Shape, ProbeError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<Shape, ProbeError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<Shape, ProbeError> {
        Ok(Shape::Other)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, ProbeError> {
        Err(ProbeError::Stop)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, ProbeError> {
        Err(ProbeError::Stop)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, ProbeError> {
        Err(ProbeError::Stop)
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, ProbeError> {
        Err(ProbeError::Stop)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Keys, ProbeError> {
        Ok(Keys::new(self.collect))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Keys, ProbeError> {
        Ok(Keys::new(self.collect))
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, ProbeError> {
        Err(ProbeError::Stop)
    }
}

/// `Stop` is at a sequence or a struct variant. Neither is an error.
#[derive(Debug)]
enum ProbeError {
    Stop,
    Custom(String),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Stop => f.write_str("not an object"),
            ProbeError::Custom(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for ProbeError {}

impl ser::Error for ProbeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        ProbeError::Custom(msg.to_string())
    }
}

impl ser::SerializeMap for Keys {
    type Ok = Shape;
    type Error = ProbeError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), ProbeError> {
        let mut pending = std::mem::take(&mut self.pending);
        key_into(key, &mut pending).map_err(|e| ProbeError::Custom(e.to_string()))?;
        self.push(&pending, None);
        self.pending = pending;
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), ProbeError> {
        Ok(())
    }
    fn end(self) -> Result<Shape, ProbeError> {
        Ok(self.shape())
    }
}

impl ser::SerializeStruct for Keys {
    type Ok = Shape;
    type Error = ProbeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, _: &T) -> Result<(), ProbeError> {
        self.push(key, Some(key));
        Ok(())
    }
    fn skip_field(&mut self, _: &'static str) -> Result<(), ProbeError> {
        Ok(())
    }
    fn end(self) -> Result<Shape, ProbeError> {
        Ok(self.shape())
    }
}

impl Probe {
    fn shape<T: Serialize + ?Sized>(value: &T) -> Result<Shape, Error> {
        match value.serialize(Probe { collect: false }) {
            Ok(shape) => Ok(shape),
            Err(ProbeError::Stop) => Ok(Shape::Other),
            Err(ProbeError::Custom(e)) => Err(Error::new(e)),
        }
    }

    fn keys<T: Serialize + ?Sized>(value: &T) -> Result<Vec<Key>, Error> {
        match value.serialize(Probe { collect: true }) {
            Ok(Shape::Keys(keys)) => Ok(keys),
            Ok(_) | Err(ProbeError::Stop) => Err(not_object()),
            Err(ProbeError::Custom(e)) => Err(Error::new(e)),
        }
    }
}

struct KeyInto<'s>(&'s mut String);

fn key_into<T: Serialize + ?Sized>(key: &T, into: &mut String) -> Result<(), Error> {
    into.clear();
    key.serialize(KeyInto(into))
}

fn not_a_string_key() -> Error {
    Error::new("a map key must be a string")
}

impl ser::Serializer for KeyInto<'_> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Impossible<(), Error>;
    type SerializeStructVariant = Impossible<(), Error>;

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.0.push_str(v);
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.0.push_str(if v { "true" } else { "false" });
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.0.push_str(itoa::Buffer::new().format(v));
        Ok(())
    }
    fn serialize_f32(self, _: f32) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_f64(self, _: f64) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_none(self) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<(), Error> {
        self.0.push_str(variant);
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<(), Error> {
        Err(not_a_string_key())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(not_a_string_key())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        Err(not_a_string_key())
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, Error> {
        Err(not_a_string_key())
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, Error> {
        Err(not_a_string_key())
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Err(not_a_string_key())
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        Err(not_a_string_key())
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, Error> {
        Err(not_a_string_key())
    }
}

struct KeyText;

impl ser::Serializer for KeyText {
    type Ok = String;
    type Error = Error;
    type SerializeSeq = Impossible<String, Error>;
    type SerializeTuple = Impossible<String, Error>;
    type SerializeTupleStruct = Impossible<String, Error>;
    type SerializeTupleVariant = Impossible<String, Error>;
    type SerializeMap = Impossible<String, Error>;
    type SerializeStruct = Impossible<String, Error>;
    type SerializeStructVariant = Impossible<String, Error>;

    fn serialize_str(self, v: &str) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_char(self, v: char) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_bool(self, v: bool) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_i8(self, v: i8) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_i16(self, v: i16) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_i32(self, v: i32) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_i64(self, v: i64) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_u8(self, v: u8) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_u16(self, v: u16) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_u32(self, v: u32) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_u64(self, v: u64) -> Result<String, Error> {
        Ok(v.to_string())
    }
    fn serialize_f32(self, _: f32) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_f64(self, _: f64) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_none(self) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String, Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<String, Error> {
        Ok(variant.to_string())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<String, Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<String, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        Err(Error::new("a map key must be a string"))
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, Error> {
        Err(Error::new("a map key must be a string"))
    }
}

// With `only`, the writer writes that one entry and no braces, and the
// caller writes the braces and the entries in canonical order.

struct Writer<'a, 'k, O: Out + ?Sized> {
    out: &'a mut O,
    only: Option<&'k str>,
    depth: u32,
}

struct Entries<'a, 'k, O: Out + ?Sized> {
    out: &'a mut O,
    only: Option<&'k str>,
    depth: u32,
    first: bool,
    last: String,
    pending: String,
}

impl<O: Out + ?Sized> Entries<'_, '_, O> {
    fn entry<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        match self.only {
            Some(only) if only != key => return Ok(()),
            Some(_) => {}
            None => {
                if !self.first && !key_order(&self.last, key).is_lt() {
                    return Err(Error::unsorted());
                }
                self.last.clear();
                self.last.push_str(key);
            }
        }
        if !self.first {
            self.out.put(b",");
        }
        self.first = false;
        string(key, self.out);
        self.out.put(b":");
        emit(value, self.out, self.depth + 1)
    }

    fn close(self) -> Result<(), Error> {
        if self.only.is_none() {
            self.out.put(b"}");
        }
        Ok(())
    }
}

struct Items<'a, O: Out + ?Sized> {
    out: &'a mut O,
    depth: u32,
    first: bool,
    close: &'static [u8],
}

impl<O: Out + ?Sized> Items<'_, O> {
    fn item<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        if !self.first {
            self.out.put(b",");
        }
        self.first = false;
        emit(value, self.out, self.depth + 1)
    }
}

struct VariantFields<'a, O: Out + ?Sized> {
    out: &'a mut O,
    depth: u32,
    variant: &'static str,
    fields: Vec<(&'static str, Vec<u8>)>,
}

impl<'a, 'k, O: Out + ?Sized> ser::Serializer for Writer<'a, 'k, O> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Items<'a, O>;
    type SerializeTuple = Items<'a, O>;
    type SerializeTupleStruct = Items<'a, O>;
    type SerializeTupleVariant = Items<'a, O>;
    type SerializeMap = Entries<'a, 'k, O>;
    type SerializeStruct = Entries<'a, 'k, O>;
    type SerializeStructVariant = VariantFields<'a, O>;

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.out.put(if v { b"true" } else { b"false" });
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        integer(v, self.out);
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        number(v as f64, self.out)
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        number(v, self.out)
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        string(v.encode_utf8(&mut [0; 4]), self.out);
        Ok(())
    }
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        string(v, self.out);
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.out.put(b"[");
        for (i, b) in v.iter().enumerate() {
            if i > 0 {
                self.out.put(b",");
            }
            integer(*b, self.out);
        }
        self.out.put(b"]");
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.out.put(b"null");
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.out.put(b"null");
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        self.out.put(b"null");
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<(), Error> {
        string(variant, self.out);
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, variant: &'static str, value: &T) -> Result<(), Error> {
        self.out.put(b"{");
        string(variant, self.out);
        self.out.put(b":");
        emit(value, self.out, self.depth + 1)?;
        self.out.put(b"}");
        Ok(())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Items<'a, O>, Error> {
        self.out.put(b"[");
        Ok(Items { out: self.out, depth: self.depth, first: true, close: b"]" })
    }
    fn serialize_tuple(self, len: usize) -> Result<Items<'a, O>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Items<'a, O>, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, variant: &'static str, _: usize) -> Result<Items<'a, O>, Error> {
        self.out.put(b"{");
        string(variant, self.out);
        self.out.put(b":[");
        Ok(Items { out: self.out, depth: self.depth, first: true, close: b"]}" })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Entries<'a, 'k, O>, Error> {
        if self.only.is_none() {
            self.out.put(b"{");
        }
        Ok(Entries { out: self.out, only: self.only, depth: self.depth, first: true, last: String::new(), pending: String::new() })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Entries<'a, 'k, O>, Error> {
        if self.only.is_none() {
            self.out.put(b"{");
        }
        Ok(Entries { out: self.out, only: self.only, depth: self.depth, first: true, last: String::new(), pending: String::new() })
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, variant: &'static str, _: usize) -> Result<VariantFields<'a, O>, Error> {
        Ok(VariantFields { out: self.out, depth: self.depth, variant, fields: Vec::new() })
    }
}

impl<O: Out + ?Sized> ser::SerializeSeq for Items<'_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }
    fn end(self) -> Result<(), Error> {
        self.out.put(self.close);
        Ok(())
    }
}

impl<O: Out + ?Sized> ser::SerializeTuple for Items<'_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }
    fn end(self) -> Result<(), Error> {
        self.out.put(self.close);
        Ok(())
    }
}

impl<O: Out + ?Sized> ser::SerializeTupleStruct for Items<'_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }
    fn end(self) -> Result<(), Error> {
        self.out.put(self.close);
        Ok(())
    }
}

impl<O: Out + ?Sized> ser::SerializeTupleVariant for Items<'_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.item(value)
    }
    fn end(self) -> Result<(), Error> {
        self.out.put(self.close);
        Ok(())
    }
}

impl<O: Out + ?Sized> ser::SerializeMap for Entries<'_, '_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        key_into(key, &mut self.pending)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = std::mem::take(&mut self.pending);
        let written = self.entry(&key, value);
        self.pending = key;
        written
    }
    fn end(self) -> Result<(), Error> {
        self.close()
    }
}

impl<O: Out + ?Sized> ser::SerializeStruct for Entries<'_, '_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        self.entry(key, value)
    }
    fn skip_field(&mut self, _: &'static str) -> Result<(), Error> {
        Ok(())
    }
    fn end(self) -> Result<(), Error> {
        self.close()
    }
}

impl<O: Out + ?Sized> ser::SerializeStructVariant for VariantFields<'_, O> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        let mut bytes = Vec::new();
        emit(value, &mut bytes, self.depth + 1)?;
        self.fields.push((key, bytes));
        Ok(())
    }
    fn skip_field(&mut self, _: &'static str) -> Result<(), Error> {
        Ok(())
    }
    fn end(mut self) -> Result<(), Error> {
        self.fields.sort_by(|a, b| key_order(a.0, b.0));
        self.out.put(b"{");
        string(self.variant, self.out);
        self.out.put(b":{");
        for (i, (key, bytes)) in self.fields.iter().enumerate() {
            if i > 0 {
                self.out.put(b",");
            }
            string(key, self.out);
            self.out.put(b":");
            self.out.put(bytes);
        }
        self.out.put(b"}}");
        Ok(())
    }
}

struct Buffer<'e> {
    entries: &'e mut Vec<(String, Vec<u8>)>,
    depth: u32,
}

struct BufferEntries<'e> {
    entries: &'e mut Vec<(String, Vec<u8>)>,
    depth: u32,
    pending: Option<String>,
}

impl<'e> ser::Serializer for Buffer<'e> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = BufferEntries<'e>;
    type SerializeStruct = BufferEntries<'e>;
    type SerializeStructVariant = Impossible<(), Error>;

    fn serialize_map(self, _: Option<usize>) -> Result<BufferEntries<'e>, Error> {
        Ok(BufferEntries { entries: self.entries, depth: self.depth, pending: None })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<BufferEntries<'e>, Error> {
        Ok(BufferEntries { entries: self.entries, depth: self.depth, pending: None })
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    // The probe has said that this value is an object.
    fn serialize_bool(self, _: bool) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_i8(self, _: i8) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_i16(self, _: i16) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_i32(self, _: i32) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_i64(self, _: i64) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_u8(self, _: u8) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_u16(self, _: u16) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_u32(self, _: u32) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_u64(self, _: u64) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_f32(self, _: f32) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_f64(self, _: f64) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_char(self, _: char) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_str(self, _: &str) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_none(self) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_unit(self) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, _: &'static str, _: &T) -> Result<(), Error> {
        Err(not_object())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(not_object())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        Err(not_object())
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeTupleStruct, Error> {
        Err(not_object())
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeTupleVariant, Error> {
        Err(not_object())
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, _: usize) -> Result<Self::SerializeStructVariant, Error> {
        Err(not_object())
    }
}

fn not_object() -> Error {
    Error::new("the value changed shape between two passes")
}

impl ser::SerializeMap for BufferEntries<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.pending = Some(key.serialize(KeyText)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self.pending.take().ok_or_else(|| Error::new("a map value came before its key"))?;
        let mut bytes = Vec::new();
        emit(value, &mut bytes, self.depth + 1)?;
        self.entries.push((key, bytes));
        Ok(())
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}

impl ser::SerializeStruct for BufferEntries<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        let mut bytes = Vec::new();
        emit(value, &mut bytes, self.depth + 1)?;
        self.entries.push((key.to_string(), bytes));
        Ok(())
    }
    fn skip_field(&mut self, _: &'static str) -> Result<(), Error> {
        Ok(())
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}
