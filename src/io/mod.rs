pub(super) mod packet;
/// this module has been ripped directly from `lsp_server`
/// https://docs.rs/lsp-server/latest/src/lsp_server/stdio.rs.html
/// ^^ Many thanks to these guys ^^
use std::{
    io::{self, stdin, stdout, BufReader},
    net::TcpStream,
    thread,
};

use crossbeam_channel::{bounded, Receiver, Sender};
use packet::MessagePacket;

use crate::{connection::InitializeConnectionMessage, msg::Message};

pub(crate) fn socket_transport<I: InitializeConnectionMessage>(
    stream: TcpStream,
) -> (Sender<Message>, Receiver<Message>, IoThreads) {
    let (reader_receiver, reader) = make_reader::<I>(stream.try_clone().unwrap());
    let (writer_sender, writer) = make_write(stream);
    let io_threads = make_io_threads(reader, writer);
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
        .name("LspServerReader".to_owned())
        .spawn(move || {
            let stdin = stdin();
            let mut stdin = stdin.lock();
            while let Some(msg) = MessagePacket::read(&mut stdin)? {
                tracing::warn!("sending message {:#?}", msg);
                let is_exit = if let Message::Res(r) = &msg {
                    I::matches(r)
                } else {
                    false
                };

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

// Creates an IoThreads
pub(crate) fn make_io_threads(
    reader: thread::JoinHandle<io::Result<()>>,
    writer: thread::JoinHandle<io::Result<()>>,
) -> IoThreads {
    IoThreads { reader, writer }
}

pub struct IoThreads {
    reader: thread::JoinHandle<io::Result<()>>,
    writer: thread::JoinHandle<io::Result<()>>,
}

impl IoThreads {
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
    let reader = thread::spawn(move || {
        let mut buf_read = BufReader::new(stream);
        while let Some(msg) = MessagePacket::read(&mut buf_read).unwrap() {
            let is_exit = if let Message::Res(r) = &msg {
                I::matches(r)
            } else {
                false
            };
            reader_sender.send(msg).unwrap();
            if is_exit {
                break;
            }
        }
        Ok(())
    });
    (reader_receiver, reader)
}

fn make_write(mut stream: TcpStream) -> (Sender<Message>, thread::JoinHandle<io::Result<()>>) {
    let (writer_sender, writer_receiver) = bounded::<Message>(0);
    let writer = thread::spawn(move || {
        writer_receiver
            .into_iter()
            .try_for_each(|it| MessagePacket::write(&mut stream, &it))
            .unwrap();
        Ok(())
    });
    (writer_sender, writer)
}
