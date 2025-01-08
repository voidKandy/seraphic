pub(super) mod packet;
/// this module has been ripped directly from `lsp_server`
/// https://docs.rs/lsp-server/latest/src/lsp_server/stdio.rs.html
/// ^^ Many thanks to these guys ^^
use std::{
    io::{self, stdin, stdout, BufReader},
    net::TcpStream,
    thread,
    time::Duration,
};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use packet::MessagePacket;

use crate::{connection::InitializeConnectionMessage, msg::Message};

pub(crate) fn socket_transport<I: InitializeConnectionMessage>(
    stream: TcpStream,
) -> (Sender<Message>, Receiver<Message>, IoThreads) {
    let (reader_receiver, reader) = make_reader::<I>(stream.try_clone().unwrap());
    let (writer_sender, writer) = make_write(stream);
    let io_threads = IoThreads { reader, writer };
    (writer_sender, reader_receiver, io_threads)
}

/// Creates an RPC connection via stdio.
pub(crate) fn stdio_transport<I: InitializeConnectionMessage>(
) -> (Sender<Message>, Receiver<Message>, IoThreads) {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let writer = thread::Builder::new()
        .name("RPCServerWriter".to_owned())
        .spawn(move || {
            let stdout = stdout();
            let mut stdout = stdout.lock();
            writer_receiver
                .into_iter()
                .try_for_each(|it| MessagePacket::write(&mut stdout, &it))
        })
        .unwrap();
    let (reader_sender, reader_receiver) = bounded::<Message>(0);
    let reader = thread::Builder::new()
        .name("RPCServerReader".to_owned())
        .spawn(move || {
            let stdin = stdin();
            let mut stdin = stdin.lock();
            while let Some(msg) = MessagePacket::read(&mut stdin)? {
                let is_exit = matches!(msg, Message::Exit);

                if let Err(e) = reader_sender.send(msg) {
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }

                if is_exit {
                    break;
                }
            }
            Ok(())
        })
        .unwrap();
    let threads = IoThreads { reader, writer };
    (writer_sender, reader_receiver, threads)
}

pub struct IoThreads {
    reader: thread::JoinHandle<io::Result<()>>,
    writer: thread::JoinHandle<io::Result<()>>,
}

impl IoThreads {
    pub(crate) fn new(
        reader: thread::JoinHandle<io::Result<()>>,
        writer: thread::JoinHandle<io::Result<()>>,
    ) -> Self {
        Self { reader, writer }
    }

    pub fn join(self) -> io::Result<()> {
        match self.reader.join() {
            Ok(r) => r?,
            Err(err) => std::panic::panic_any(err),
        }
        match self.writer.join() {
            Ok(r) => r,
            Err(err) => {
                std::panic::panic_any(err);
            }
        }
    }
}

fn make_reader<I: InitializeConnectionMessage>(
    stream: TcpStream,
) -> (Receiver<Message>, thread::JoinHandle<io::Result<()>>) {
    let (reader_sender, reader_receiver) = bounded::<Message>(0);
    let mut should_exit = false;
    let reader = thread::spawn(move || {
        let mut buf_read = BufReader::new(stream);
        while !should_exit {
            if let Some(msg) = MessagePacket::read(&mut buf_read).unwrap() {
                should_exit = matches!(msg, Message::Exit);

                reader_sender
                    .send_timeout(msg, Duration::from_secs(1))
                    .unwrap();
            }
        }
        tracing::debug!("reader out");
        Ok(())
    });
    (reader_receiver, reader)
}

fn make_write(mut stream: TcpStream) -> (Sender<Message>, thread::JoinHandle<io::Result<()>>) {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let mut should_exit = false;
    let writer = thread::spawn(move || {
        while !should_exit {
            match writer_receiver.try_recv() {
                Ok(msg) => {
                    should_exit = matches!(msg, Message::Exit);
                    MessagePacket::write(&mut stream, &msg)?;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }
        }
        tracing::warn!("writer out");
        Ok(())
    });
    (writer_sender, writer)
}
