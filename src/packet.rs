use serde::{Deserialize, Deserializer, Serialize};
use std::{
    io::{BufRead, ErrorKind, Write},
    marker::PhantomData,
};

use crate::MainResult;

#[derive(Clone, Debug)]
pub struct TcpPacket<T> {
    pub(crate) buffer: Vec<u8>,
    marker: PhantomData<T>,
}

impl<T> PartialEq for TcpPacket<T> {
    fn eq(&self, other: &Self) -> bool {
        self.buffer.eq(&other.buffer)
    }
}

type HeaderSize = u32;
pub(crate) const fn header_size() -> usize {
    std::mem::size_of::<HeaderSize>() / std::mem::size_of::<u8>()
}

impl<T> TcpPacket<T> {
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

impl<T> TcpPacket<T>
where
    T: Serialize + std::fmt::Debug + for<'de> Deserialize<'de>,
{
    pub fn try_into_inner(self) -> MainResult<T> {
        let buf = &self.buffer[header_size()..];
        let str = String::from_utf8_lossy(buf);
        serde_json::from_slice::<T>(buf).map_err(|err| {
            std::io::Error::other(format!(
                "error getting tcp packet inner from slice: {err:#?}\nbuffer: {str}"
            ))
            .into()
        })
    }
}

impl<T> From<&T> for TcpPacket<T>
where
    T: Serialize + std::fmt::Debug + for<'de> Deserialize<'de>,
{
    fn from(r: &T) -> Self {
        let vec = serde_json::to_vec(r).expect("T will not work");

        assert!(
            vec.len() <= HeaderSize::MAX as usize,
            "consider making the header size larger"
        );

        let size: u32 = vec.len() as u32;

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

impl<T> Serialize for TcpPacket<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.buffer.serialize(serializer)
    }
}

impl<T> TcpPacket<T>
where
    T: Serialize + std::fmt::Debug + for<'de> Deserialize<'de>,
{
    pub fn read(inp: &mut dyn BufRead) -> std::io::Result<Option<T>> {
        let mut header = [0u8; header_size()];
        let mut buffer = [0u8; 1024].to_vec();
        let mut size = None;
        while size.is_none() {
            match inp.read_exact(&mut header) {
                Ok(_) => {
                    if header.is_empty() {
                        break;
                    }
                    let payload_size = u32::from_le_bytes(header) as usize;
                    size = Some(payload_size);
                }
                Err(err)
                    if err.kind() == ErrorKind::UnexpectedEof && header == [0u8; header_size()] =>
                {
                    return Ok(None);
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    return Ok(None);
                }
                Err(err) => {
                    return Err(std::io::Error::other(format!(
                        "unexepect error when reading header: {err:#?}\nbuffer: {}",
                        String::from_utf8_lossy(&buffer)
                    )));
                }
            }
        }
        let size: usize = size.ok_or(std::io::Error::other("no content length"))?;
        tracing::debug!("got payload size from header: {size}");
        buffer.resize(size, 0);
        match inp.read_exact(&mut buffer) {
            Ok(_) => {
                let typ = serde_json::from_slice::<T>(&buffer).map_err(|err| {
                    std::io::Error::other(format!(
                        "malformed payload: {}\nErr: {err:#?}",
                        String::from_utf8_lossy(&buffer),
                    ))
                })?;
                Ok(Some(typ))
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                return Ok(None);
            }
            Err(err) => {
                return Err(std::io::Error::other(format!(
                    "unexepect error when reading payload: {err:#?}\nbuffer: {}",
                    String::from_utf8_lossy(&buffer)
                )));
            }
        }
    }

    pub fn write(out: &mut dyn Write, typ: &T) -> std::io::Result<()> {
        let packet = Self::from(typ);
        out.write_all(&packet.buffer)?;
        out.flush()?;
        Ok(())
    }
}
