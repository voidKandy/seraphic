use std::{
    marker::PhantomData,
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc},
};

use crossbeam_channel::{Receiver, Sender};

use crate::Message;

use super::{io::IoThreads, Connection, InitializeConnectionMessage};

pub(crate) trait TcpEndpoint {
    type InitializeReq: InitializeConnectionMessage;
    const CHANNEL_BUFFER_SIZE: usize;

    fn socket_transport_connection(
        stream: TcpStream,
        shutdown: Arc<AtomicBool>,
    ) -> Connection<Self::InitializeReq> {
        stream.set_nonblocking(true).unwrap();
        let (reader_receiver, reader) =
            Self::make_reader(stream.try_clone().unwrap(), Arc::clone(&shutdown));
        let (writer_sender, writer) = Self::make_writer(stream, Arc::clone(&shutdown));
        let io_threads = IoThreads {
            reader,
            writer,
            shutdown_signal: shutdown,
        };

        Connection {
            sender: writer_sender,
            receiver: reader_receiver,
            io_threads,
            init_request_marker: PhantomData,
        }
    }

    /// returns a receiver for the endpoint as well as the thread containing the reader writer loop
    fn make_reader(
        stream: TcpStream,
        shutdown_signal: Arc<AtomicBool>,
    ) -> (
        Receiver<Message>,
        std::thread::JoinHandle<std::io::Result<()>>,
    ) {
        let (reader_sender, reader_receiver) =
            crossbeam_channel::bounded::<Message>(Self::CHANNEL_BUFFER_SIZE);
        let reader = std::thread::spawn(move || {
            let _ = Self::reader_thread_logic(stream, reader_sender, shutdown_signal)?;
            Ok(())
        });
        (reader_receiver, reader)
    }

    fn make_writer(
        stream: TcpStream,
        shutdown_signal: Arc<AtomicBool>,
    ) -> (
        Sender<Message>,
        std::thread::JoinHandle<std::io::Result<()>>,
    ) {
        let (writer_sender, writer_receiver) =
            crossbeam_channel::bounded::<Message>(Self::CHANNEL_BUFFER_SIZE);
        let writer = std::thread::spawn(move || {
            let _ = Self::writer_thread_logic(stream, writer_receiver, shutdown_signal)?;
            Ok(())
        });

        (writer_sender, writer)
    }

    fn reader_thread_logic(
        stream: TcpStream,
        reader_sender: Sender<Message>,
        shutdown_signal: Arc<AtomicBool>,
    ) -> std::io::Result<()>;

    fn writer_thread_logic(
        stream: TcpStream,
        writer_receiver: Receiver<Message>,
        shutdown_signal: Arc<AtomicBool>,
    ) -> std::io::Result<()>;
}
