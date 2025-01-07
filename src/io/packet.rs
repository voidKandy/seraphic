use crate::msg::Message;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    marker::PhantomData,
};

pub struct TcpPacket<T> {
    buffer: Vec<u8>,
    marker: PhantomData<T>,
}

pub type MessagePacket = TcpPacket<Message>;
type HeaderSize = u32;
const fn header_size() -> usize {
    std::mem::size_of::<HeaderSize>() / std::mem::size_of::<u8>()
}

impl<T> From<&T> for TcpPacket<T>
where
    T: Serialize,
{
    fn from(r: &T) -> Self {
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

impl<'de, T> serde::Deserialize<'de> for TcpPacket<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let buffer = <Vec<u8> as Deserialize>::deserialize(deserializer)?;
        Ok(Self {
            buffer,
            marker: PhantomData,
        })
    }
}

impl<T> TcpPacket<T>
where
    T: serde::Serialize + for<'de> Deserialize<'de>,
{
    pub fn read(inp: &mut dyn BufRead) -> std::io::Result<Option<T>> {
        let mut header = [0u8; header_size()];
        let mut buffer = [0u8; 1024].to_vec();
        let mut size = None;
        tracing::warn!("reading");
        loop {
            match inp.read_exact(&mut header) {
                Ok(_) => {
                    if header.is_empty() {
                        break;
                    }
                    let payload_size = u32::from_le_bytes(header) as usize;
                    tracing::warn!("expecting payload of size: {payload_size}");
                    size = Some(payload_size)
                }
                Err(err) => {
                    return Err(std::io::Error::other(format!(
                        "unexepect error when reading: {err:#?}",
                    )));
                }
            }
        }
        let size: usize = size.ok_or(std::io::Error::other("no content length"))?;
        buffer.resize(size, 0);
        inp.read_exact(&mut buffer)?;
        let typ = serde_json::from_slice::<T>(&buffer).map_err(|err| {
            std::io::Error::other(format!(
                "malformed payload: {}\nErr: {err:#?}",
                String::from_utf8_lossy(&buffer),
            ))
        })?;
        // let buf = String::from_utf8(buf).map_err(invalid_data)?;
        // log::debug!("< {}", buf);
        Ok(Some(typ))
    }

    pub fn write(out: &mut dyn Write, typ: &T) -> std::io::Result<()> {
        let packet = Self::from(typ);
        out.write_all(&packet.buffer)?;
        out.flush()?;
        Ok(())
    }
}
