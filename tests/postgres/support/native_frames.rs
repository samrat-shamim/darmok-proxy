//! A single native command-phase consumer for the no-Sync regression.
//! The existing connector establishes the connection. Its driver is retained
//! unpolled, and no requests are submitted through its Client. Only this
//! consumer reads/writes after startup Ready; it sends Terminate normally.
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_postgres::{Client, Connection, NoTls, config::Host, tls::NoTlsStream};

pub struct NativeFrames {
    stream: TcpStream,
    _client: Client,
    _connection: Connection<TcpStream, NoTlsStream>,
    pub trace: Vec<&'static str>,
}

impl NativeFrames {
    pub async fn connect() -> Self {
        let config: tokio_postgres::Config = std::env::var("DARMOK_TEST_DATABASE_URL")
            .expect("required native fixture URL")
            .parse()
            .unwrap();
        let [Host::Tcp(host)] = config.get_hosts() else {
            panic!("native frame fixture requires one TCP host")
        };
        let [port] = config.get_ports() else {
            panic!("native frame fixture requires one explicit port")
        };
        let socket = tokio::time::timeout(
            Duration::from_secs(20),
            TcpStream::connect((host.as_str(), *port)),
        )
        .await
        .unwrap()
        .unwrap()
        .into_std()
        .unwrap();
        let stream = TcpStream::from_std(socket.try_clone().unwrap()).unwrap();
        let startup = TcpStream::from_std(socket).unwrap();
        let (client, connection) =
            tokio::time::timeout(Duration::from_secs(20), config.connect_raw(startup, NoTls))
                .await
                .unwrap()
                .unwrap();
        Self {
            stream,
            _client: client,
            _connection: connection,
            trace: Vec::new(),
        }
    }

    async fn send(&mut self, tag: u8, payload: &[u8]) {
        let mut frame = Vec::with_capacity(payload.len() + 5);
        frame.push(tag);
        frame.extend_from_slice(&u32::try_from(payload.len() + 4).unwrap().to_be_bytes());
        frame.extend_from_slice(payload);
        tokio::time::timeout(Duration::from_secs(20), self.stream.write_all(&frame))
            .await
            .unwrap()
            .unwrap();
    }

    pub async fn message(&mut self) -> (u8, Vec<u8>) {
        tokio::time::timeout(Duration::from_secs(20), async {
            let mut header = [0; 5];
            self.stream.read_exact(&mut header).await.unwrap();
            let length = u32::from_be_bytes(header[1..].try_into().unwrap());
            assert!((4..=1_048_576).contains(&length));
            let mut payload = vec![0; usize::try_from(length - 4).unwrap()];
            self.stream.read_exact(&mut payload).await.unwrap();
            assert_ne!(header[0], b'E', "native fixture returned ErrorResponse");
            (header[0], payload)
        })
        .await
        .expect("native frame fixture did not complete")
    }

    pub async fn simple(&mut self, sql: &str) -> (Vec<String>, Vec<String>) {
        self.send(b'Q', &cstring(sql)).await;
        let mut commands = Vec::new();
        let mut rows = Vec::new();
        loop {
            let (tag, payload) = self.message().await;
            match tag {
                b'T' => assert_eq!(i16::from_be_bytes(payload[..2].try_into().unwrap()), 1),
                b'D' => {
                    assert_eq!(i16::from_be_bytes(payload[..2].try_into().unwrap()), 1);
                    let bytes =
                        usize::try_from(i32::from_be_bytes(payload[2..6].try_into().unwrap()))
                            .unwrap();
                    assert_eq!(payload.len(), bytes + 6);
                    rows.push(String::from_utf8(payload[6..].to_vec()).unwrap());
                }
                b'C' => commands.push(command(&payload)),
                b'Z' => {
                    assert_eq!(payload, b"I");
                    return (commands, rows);
                }
                other => panic!("unexpected native simple-query message: {other}"),
            }
        }
    }

    pub async fn parse_bind(&mut self, statement: &str, portal: &str, sql: &str) {
        let mut parse = cstring(statement);
        parse.extend_from_slice(&cstring(sql));
        parse.extend_from_slice(&0_u16.to_be_bytes());
        self.send(b'P', &parse).await;
        self.trace.push("Parse");
        let mut bind = cstring(portal);
        bind.extend_from_slice(&cstring(statement));
        bind.extend_from_slice(&[0; 6]); // no parameter/result formats or values
        self.send(b'B', &bind).await;
        self.trace.push("Bind");
    }

    pub async fn flush_parse_bind(&mut self) {
        self.send(b'H', &[]).await;
        self.trace.push("Flush");
        assert_eq!(self.message().await, (b'1', Vec::new()));
        assert_eq!(self.message().await, (b'2', Vec::new()));
    }

    pub async fn execute_sync(&mut self, portal: &str) {
        let mut execute = cstring(portal);
        execute.extend_from_slice(&0_u32.to_be_bytes());
        self.send(b'E', &execute).await;
        self.trace.push("Execute");
        self.send(b'S', &[]).await;
        self.trace.push("Sync");
    }

    pub async fn completion(&mut self, expected: &str) {
        assert_eq!(self.message().await, (b'1', Vec::new()));
        assert_eq!(self.message().await, (b'2', Vec::new()));
        let (tag, payload) = self.message().await;
        assert_eq!(tag, b'C');
        assert_eq!(command(&payload), expected);
        assert_eq!(self.message().await, (b'Z', b"I".to_vec()));
    }

    pub async fn close(mut self) {
        self.send(b'X', &[]).await;
        self.stream.shutdown().await.unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(20), self.stream.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}

fn cstring(text: &str) -> Vec<u8> {
    assert!(!text.as_bytes().contains(&0));
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn command(payload: &[u8]) -> String {
    assert_eq!(payload.last(), Some(&0));
    String::from_utf8(payload[..payload.len() - 1].to_vec()).unwrap()
}
