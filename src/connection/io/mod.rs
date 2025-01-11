pub(crate) mod packet;
use crate::{connection::InitializeConnectionMessage, msg::Message};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use packet::MessagePacket;
use std::{
    io::{self, BufReader},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub struct IoThreads {
    pub reader: thread::JoinHandle<io::Result<()>>,
    pub writer: thread::JoinHandle<io::Result<()>>,
    pub shutdown_signal: Arc<AtomicBool>,
}

impl IoThreads {
    pub fn new(
        reader: thread::JoinHandle<io::Result<()>>,
        writer: thread::JoinHandle<io::Result<()>>,
        shutdown_signal: Arc<AtomicBool>,
    ) -> Self {
        Self {
            reader,
            writer,
            shutdown_signal,
        }
    }

    pub fn join(self) -> io::Result<()> {
        tracing::debug!("joining threads");

        match self.reader.join() {
            Ok(r) => r?,
            Err(err) => std::panic::panic_any(err),
        }
        tracing::debug!("reader joined");

        match self.writer.join() {
            Ok(r) => r?,
            Err(err) => {
                std::panic::panic_any(err);
            }
        }

        tracing::debug!("writer joined");
        Ok(())
    }
}
