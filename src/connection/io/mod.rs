pub(super) mod packet;
use std::{
    io::{self, BufReader},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, park_timeout},
    time::Duration,
};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use packet::MessagePacket;

use crate::{connection::InitializeConnectionMessage, msg::Message};

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

    pub fn force_shutdown(self) -> io::Result<()> {
        self.shutdown_signal.store(true, Ordering::Relaxed);
        self.join()?;
        Ok(())
    }
}

pub fn make_reader<I: InitializeConnectionMessage>(
    stream: TcpStream,
    shutdown_signal: Arc<AtomicBool>,
) -> (Receiver<Message>, thread::JoinHandle<io::Result<()>>) {
    let (reader_sender, reader_receiver) = bounded::<Message>(5);
    let reader = thread::spawn(move || {
        let mut buf_read = BufReader::new(stream);
        while !shutdown_signal.load(Ordering::Relaxed) {
            if let Some(msg) = MessagePacket::read(&mut buf_read).unwrap() {
                tracing::debug!("got message: {msg:#?}");
                if matches!(msg, Message::Exit) {
                    shutdown_signal.store(true, Ordering::Relaxed);
                }

                reader_sender.send(msg).unwrap();
            }
        }
        tracing::debug!("reader should_join");
        Ok(())
    });
    (reader_receiver, reader)
}

pub fn make_writer(
    mut stream: TcpStream,
    shutdown_signal: Arc<AtomicBool>,
) -> (Sender<Message>, thread::JoinHandle<io::Result<()>>) {
    let (writer_sender, writer_receiver) = bounded::<Message>(5);
    let writer = thread::spawn(move || {
        while !shutdown_signal.load(Ordering::Relaxed) {
            match writer_receiver.try_recv() {
                Ok(msg) => {
                    tracing::debug!("going to send msg: {msg:#?}");
                    if matches!(msg, Message::Exit) {
                        shutdown_signal.store(true, Ordering::Relaxed);
                    }
                    MessagePacket::write(&mut stream, &msg)?;
                }
                Err(TryRecvError::Empty) => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(TryRecvError::Disconnected) => {
                    shutdown_signal.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        tracing::debug!("writer should join");
        Ok(())
    });
    (writer_sender, writer)
}
