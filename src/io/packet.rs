use core::error;
use std::{
    io::{BufRead, Write},
    marker::PhantomData,
};

use serde::{Deserialize, Serialize};

use crate::{MainErr, MainResult};

pub struct TcpPacket<'p, T> {
    buffer: Vec<u8>,
    marker: PhantomData<&'p T>,
}

pub type MessagePacket = TcpPacket<'static, crate::socket::Message>;
type HeaderSize = u32;
const fn header_size() -> usize {
    std::mem::size_of::<HeaderSize>() / std::mem::size_of::<u8>()
}

impl<'p, T> From<&'p T> for TcpPacket<'p, T>
where
    T: Serialize,
{
    fn from(r: &'p T) -> Self {
        let vec = serde_json::to_vec(r).expect("T will not work");

        assert!(
            vec.len() <= HeaderSize::MAX as usize,
            "consider making the header size larger"
        );

        let size: u32 = vec.len() as u32;

        tracing::warn!("serialized payload of size: {size}");

        let mut buffer = Vec::with_capacity(header_size() + vec.len());
        buffer.extend_from_slice(&size.to_le_bytes());
        buffer.extend_from_slice(&vec);
        Self {
            marker: PhantomData,
            buffer,
        }
    }
}

impl<'p, T> serde::Serialize for TcpPacket<'p, T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.buffer, serializer)
    }
}

impl<'p, T> TcpPacket<'p, T>
where
    T: serde::Serialize,
{
    pub fn read(inp: &mut dyn BufRead) -> std::io::Result<Option<T>> {
        let buffer = [0u8; 1024];
        let mut vecs = vec![Vec::with_capacity(header_size()), buffer];
        let mut size = None;
        // let mut buf = String::new();
        loop {
            // vecs.iter_mut().for_each(|v| v.clear());
            let n = inp.read_vectored(vecs)?;
            if n == 0 {
                return Ok(None);
            } else if n < header_size() {
                return Err(std::io::Error::other(format!(
                    "read an unexpected amount, expected at least: {}, got: {n}",
                    header_size()
                )));
            }

            let payload_size = u32::from_le_bytes(vecs[0]) as usize;
            tracing::warn!("expecting payload of size: {payload_size}");
            size = Some(payload_size)
        }
        let size: usize = size.ok_or_else(|| Err(std::io::Error::other("no content length")))?;
        let mut buf = vecs[1];
        buf.resize(size, 0);
        inp.read_exact(&mut buf)?;
        let typ = serde_json::from_slice::<T>(&buf).unwrap_or_else(|| {
            std::io::Error::other(format!(
                "malformed payload: {}",
                String::from_utf8_lossy(&buf),
            ))
        })?;
        // let buf = String::from_utf8(buf).map_err(invalid_data)?;
        // log::debug!("< {}", buf);
        Ok(Some(typ))
    }

    pub fn write(out: &mut dyn Write, typ: &T) -> std::io::Result<()> {
        let packet = Self::from(typ);
        serde_json::to_value(&packet)?
    }
}

mod tests {
    use super::TcpPacket;

    #[test]
    fn packet() {
        let json = serde_json::json!({});
        let packet = TcpPacket::from(&json);
        let vec = serde_json::to_value(&packet).unwrap();
    }
}
