use crate::packet::{header_size, PacketRead, TcpPacket};
use std::io::ErrorKind;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

impl<T> TcpPacket<T>
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + std::fmt::Debug,
{
    pub async fn async_read<R>(inp: &mut R) -> std::io::Result<PacketRead<T>>
    where
        R: AsyncRead + std::marker::Unpin,
    {
        let mut header = [0u8; header_size()];
        let mut buffer = [0u8; 1024].to_vec();
        let mut size = None;
        while size.is_none() {
            match inp.read_exact(&mut header).await {
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
                    return Ok(PacketRead::Disconnected);
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    return Ok(PacketRead::Empty);
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
        match inp.read_exact(&mut buffer).await {
            Ok(_) => {
                let typ = serde_json::from_slice::<T>(&buffer).map_err(|err| {
                    std::io::Error::other(format!(
                        "malformed payload: {}\nErr: {err:#?}",
                        String::from_utf8_lossy(&buffer),
                    ))
                })?;
                Ok(PacketRead::Message(typ))
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                return Ok(PacketRead::Empty);
            }
            Err(err) => {
                return Err(std::io::Error::other(format!(
                    "unexepect error when reading payload: {err:#?}\nbuffer: {}",
                    String::from_utf8_lossy(&buffer)
                )));
            }
        }
    }

    pub async fn async_write<W>(out: &mut W, typ: &T) -> std::io::Result<()>
    where
        W: AsyncWrite + std::marker::Unpin,
    {
        let packet = Self::from(typ);
        let _ = out.write_all(&packet.buffer).await?;
        out.flush().await?;
        Ok(())
    }
}
