pub(super) mod packet;
/// this module has been ripped directly from `lsp_server`
/// https://docs.rs/lsp-server/latest/src/lsp_server/stdio.rs.html
/// ^^ Many thanks to these guys ^^
use std::{
    io::{self, stdin, stdout, BufReader},
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

pub(crate) fn socket_transport<I: InitializeConnectionMessage>(
    stream: TcpStream,
) -> (Sender<Message>, Receiver<Message>, IoThreads) {
    let shutdown = Arc::new(AtomicBool::new(false));
    let (reader_receiver, reader) =
        make_reader::<I>(stream.try_clone().unwrap(), Arc::clone(&shutdown));
    let (writer_sender, writer) = make_writer(stream, Arc::clone(&shutdown));
    let io_threads = IoThreads {
        reader,
        writer,
        shutdown_signal: shutdown,
    };
    (writer_sender, reader_receiver, io_threads)
}

pub struct IoThreads {
    reader: thread::JoinHandle<io::Result<()>>,
    writer: thread::JoinHandle<io::Result<()>>,
    shutdown_signal: Arc<AtomicBool>,
}

impl IoThreads {
    pub(crate) fn new(
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
        tracing::warn!("joining threads");

        match self.reader.join() {
            Ok(r) => r?,
            Err(err) => std::panic::panic_any(err),
        }
        tracing::warn!("reader joined");
        match self.writer.join() {
            Ok(r) => r?,
            Err(err) => {
                std::panic::panic_any(err);
            }
        }

        tracing::warn!("writer joined");

        Ok(())
    }

    pub fn force_shutdown(self) -> io::Result<()> {
        self.shutdown_signal.store(true, Ordering::Relaxed);
        self.join()?;
        Ok(())
    }
}

fn make_reader<I: InitializeConnectionMessage>(
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

fn make_writer(
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
