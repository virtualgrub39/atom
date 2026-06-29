/// simple u8 buffer reader / writer utilities
/// used for bytecode blob parsing and assembly
use crate::atom::{AtomError, AtomResult, Opcode};

pub trait Readable: Sized {
    const SIZE: usize;
    fn from_bytes(bytes: &[u8]) -> Self;
}

impl Readable for u8 {
    const SIZE: usize = 1;
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0]
    }
}

impl Readable for u16 {
    const SIZE: usize = 2;
    fn from_bytes(bytes: &[u8]) -> Self {
        u16::from_le_bytes([bytes[0], bytes[1]])
    }
}

impl Readable for i16 {
    const SIZE: usize = 2;
    fn from_bytes(bytes: &[u8]) -> Self {
        i16::from_le_bytes([bytes[0], bytes[1]])
    }
}

impl Readable for u32 {
    const SIZE: usize = 4;
    fn from_bytes(bytes: &[u8]) -> Self {
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

impl Readable for f64 {
    const SIZE: usize = 8;
    fn from_bytes(bytes: &[u8]) -> Self {
        f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        return Self { data };
    }

    pub fn fetch<T: Readable>(&mut self) -> AtomResult<T> {
        if self.data.len() < T::SIZE {
            return Err(AtomError::MalformedBytecode);
        }

        let (bytes, rest) = self.data.split_at(T::SIZE);
        self.data = rest;
        Ok(T::from_bytes(bytes))
    }

    pub fn fetch_vec(&mut self, n: usize) -> AtomResult<Vec<u8>> {
        if self.data.len() < n {
            return Err(AtomError::MalformedBytecode);
        }

        let (bytes, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(bytes.to_vec())
    }

    pub fn fetch_str(&mut self) -> AtomResult<String> {
        if self.data.len() == 0 {
            return Err(AtomError::MalformedBytecode);
        }

        let len = self.fetch::<u16>()?;
        let bytes = self.fetch_vec(len as usize)?;
        String::from_utf8(bytes).map_err(|_| AtomError::MalformedBytecode)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

pub trait Writable {
    fn write_to(&self, buf: &mut Vec<u8>);

    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write_to(&mut buf);
        buf
    }
}

impl Writable for u8 {
    fn write_to(&self, buf: &mut Vec<u8>) {
        buf.push(*self);
    }
}

impl Writable for u16 {
    fn write_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.to_le_bytes());
    }
}

impl Writable for Vec<u8> {
    fn write_to(&self, buf: &mut Vec<u8>) {
        buf.extend(self);
    }
}

impl Writable for Opcode {
    fn write_to(&self, buf: &mut Vec<u8>) {
        buf.push(*self as u8);
    }
}

impl Writable for &str {
    fn write_to(&self, buf: &mut Vec<u8>) {
        let len = self.len() as u16;
        len.write_to(buf);
        buf.extend_from_slice(self.as_bytes());
    }
}

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn write<T: Writable>(&mut self, value: T) -> &mut Self {
        value.write_to(&mut self.buf);
        self
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}
