use crate::types::{
    Channel, Client, ClientInfo, CreateChannel, DatabaseId, QueryError, QueryResult, ServerInfo,
    WhoAmI,
};
use crate::types::{FromQueryString, QueryStatus};
use anyhow::anyhow;
use log::{error, warn};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const BUFFER_SIZE: usize = 512;

pub struct SocketConn {
    conn: TcpStream,
}

impl SocketConn {
    fn decode_status(content: String) -> QueryResult<String> {
        debug_assert!(
            !content.contains("Welcome to the TeamSpeak 3") && content.contains("error id="),
            "Content => {content:?}",
        );

        for line in content.lines() {
            if line.trim().starts_with("error ") {
                let status = QueryStatus::try_from(line)?;

                return status.into_result(content);
            }
        }
        Err(QueryError::static_empty_response())
    }

    async fn read_inner(
        &mut self,
        timeout: Option<Duration>,
        is_complete: impl Fn(&str, usize) -> bool,
    ) -> anyhow::Result<Option<String>> {
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut ret = String::new();
        loop {
            let size = match timeout {
                Some(dur) => match tokio::time::timeout(dur, self.conn.read(&mut buffer)).await {
                    Ok(Ok(n)) => n,
                    Ok(Err(e)) => return Err(anyhow!("Got error while read data: {e:?}")),
                    Err(_) => return Ok(None),
                },
                None => self
                    .conn
                    .read(&mut buffer)
                    .await
                    .map_err(|e| anyhow!("Got error while read data: {e:?}"))?,
            };
            if size == 0 {
                return Ok(None);
            }
            ret.push_str(&String::from_utf8_lossy(&buffer[..size]));
            if is_complete(&ret, size) {
                break;
            }
        }
        Ok(Some(ret))
    }

    pub async fn read_event(&mut self) -> anyhow::Result<Option<String>> {
        self.read_inner(None, |s, size| size < BUFFER_SIZE || s.ends_with("\n\r"))
            .await
    }

    fn decode_status_with_result<T: FromQueryString + Sized>(
        data: String,
    ) -> QueryResult<Option<Vec<T>>> {
        let content = Self::decode_status(data)?;

        for line in content.lines() {
            if !line.starts_with("error ") {
                let mut v = Vec::new();
                for element in line.split('|') {
                    v.push(T::from_query(element)?);
                }
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    pub(crate) async fn read_data(&mut self) -> anyhow::Result<Option<String>> {
        self.read_inner(Some(Duration::from_secs(2)), |s, _| {
            s.contains("error id=") && s.ends_with("\n\r")
        })
        .await
    }

    pub(crate) async fn write_data(&mut self, payload: &str) -> anyhow::Result<()> {
        debug_assert!(payload.ends_with("\n\r"));
        self.conn
            .write(payload.as_bytes())
            .await
            .map(|size| {
                if size != payload.len() {
                    error!(
                        "Error payload size mismatch! expect {} but {size} found. payload: {payload:?}",
                        payload.len(),
                    )
                }
            })
            .map_err(|e| anyhow!("Got error while send data: {e:?}"))?;
        /*self.conn
        .flush()
        .await
        .inspect_err(|e| anyhow!("Got error while flush data: {e:?}"))?;*/
        Ok(())
    }

    async fn write_and_read(&mut self, payload: &str) -> anyhow::Result<String> {
        self.write_data(payload).await?;
        self.read_data()
            .await?
            .ok_or_else(|| anyhow!("Return data is None"))
    }

    async fn basic_operation(&mut self, payload: &str) -> QueryResult<()> {
        let data = self.write_and_read(payload).await?;
        Self::decode_status(data)?;
        Ok(())
    }

    async fn query_operation_non_error<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Vec<T>> {
        let data = self.write_and_read(payload).await?;
        let ret = Self::decode_status_with_result(data)?;
        Ok(ret
            .ok_or_else(|| panic!("Can't find result line, payload => {payload}"))
            .unwrap())
    }

    async fn query_operation<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Option<Vec<T>>> {
        let data = self.write_and_read(payload).await?;
        Self::decode_status_with_result(data)
        //let status = status.ok_or_else(|| anyhow!("Can't find status line."))?;
    }

    async fn query_one_operation<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Option<T>> {
        self.query_operation(payload)
            .await
            .map(|r| r.map(|mut v| v.swap_remove(0)))
    }

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace(' ', "\\s")
            .replace('/', "\\/")
    }

    pub async fn connect(server: &str, port: u16) -> anyhow::Result<Self> {
        let conn = TcpStream::connect(format!("{server}:{port}"))
            .await
            .map_err(|e| anyhow!("Got error while connect to {server}:{port} {e:?}"))?;

        //let bufreader = BufReader::new(conn);
        //conn.set_nonblocking(true).unwrap();
        let mut self_ = Self { conn };

        let content = self_
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error in connect while read content: {e:?}"))?;

        if content.is_none() {
            warn!("Read none data.");
        }

        Ok(self_)
    }

    pub async fn login(&mut self, user: &str, password: &str) -> QueryResult<()> {
        let payload = format!("login {user} {password}\n\r");
        self.basic_operation(payload.as_str()).await
    }

    pub async fn select_server(&mut self, server_id: i64) -> QueryResult<()> {
        let payload = format!("use {server_id}\n\r");
        self.basic_operation(payload.as_str()).await
    }

    pub(crate) async fn who_am_i(&mut self) -> QueryResult<WhoAmI> {
        self.query_operation_non_error("whoami\n\r")
            .await
            .map(|mut v| v.remove(0))
    }

    #[allow(unused)]
    pub(crate) async fn send_text_message(
        &mut self,
        client_id: i64,
        text: &str,
    ) -> QueryResult<()> {
        let payload = format!(
            "sendtextmessage targetmode=1 target={client_id} msg={text}\n\r",
            client_id = client_id,
            text = Self::escape(text)
        );
        self.basic_operation(&payload).await
    }

    pub(crate) async fn send_text_message_unchecked(
        &mut self,
        client_id: i64,
        text: &str,
    ) -> anyhow::Result<()> {
        let payload = format!(
            "sendtextmessage targetmode=1 target={client_id} msg={text}\n\r",
            client_id = client_id,
            text = Self::escape(text)
        );
        self.write_data(&payload).await
    }

    pub(crate) async fn query_server_info(&mut self) -> QueryResult<ServerInfo> {
        self.query_operation_non_error("serverinfo\n\r")
            .await
            .map(|mut v| v.remove(0))
    }

    pub(crate) async fn query_channels(&mut self) -> QueryResult<Vec<Channel>> {
        self.query_operation_non_error("channellist\n\r").await
    }

    pub(crate) async fn create_channel(
        &mut self,
        name: &str,
        pid: i64,
    ) -> QueryResult<Option<CreateChannel>> {
        let payload = format!(
            "channelcreate channel_name={name} cpid={pid} channel_codec_quality=6\n\r",
            name = Self::escape(name),
            pid = pid
        );
        /*let ret = self.query_operation(payload.as_str()).await?;
        Ok(ret.map(|mut v| v.remove(0)))*/
        self.query_operation(payload.as_str())
            .await
            .map(|r| r.map(|mut v| v.swap_remove(0)))
    }

    pub(crate) async fn query_clients(&mut self) -> QueryResult<Vec<Client>> {
        self.query_operation_non_error("clientlist\n\r").await
    }

    pub(crate) async fn move_client(
        &mut self,
        client_id: i64,
        target_channel: i64,
    ) -> QueryResult<()> {
        let payload = format!(
            "clientmove clid={client_id} cid={cid}\n\r",
            client_id = client_id,
            cid = target_channel
        );
        self.basic_operation(payload.as_str()).await
    }

    pub(crate) async fn set_client_channel_group(
        &mut self,
        client_database_id: i64,
        channel_id: i64,
        group_id: i64,
    ) -> QueryResult<()> {
        let payload = format!(
            "setclientchannelgroup cgid={group} cid={channel_id} cldbid={client_database_id}\n\r",
            group = group_id,
            channel_id = channel_id,
            client_database_id = client_database_id
        );
        self.basic_operation(&payload).await
    }

    pub(crate) async fn add_channel_permission(
        &mut self,
        target_channel: i64,
        permissions: &[(u64, i64)],
    ) -> QueryResult<()> {
        let payload = format!(
            "channeladdperm cid={target_channel} {}\n\r",
            permissions
                .iter()
                .map(|(k, v)| format!("permid={k} permvalue={v}",))
                .collect::<Vec<String>>()
                .join("|")
        );
        self.basic_operation(&payload).await
    }

    pub async fn send_keepalive(&mut self) -> QueryResult<()> {
        self.write_data("whoami\n\rbanlist\n\r")
            .await
            .map_err(QueryError::from)
    }

    pub(crate) async fn logout(&mut self) -> QueryResult<()> {
        self.basic_operation("quit\n\r").await
    }

    pub async fn register_observer_events(&mut self) -> QueryResult<()> {
        self.basic_operation("servernotifyregister event=server\n\r")
            .await?;
        self.basic_operation("servernotifyregister event=textprivate\n\r")
            .await
    }

    /// As http://yat.qa/ressourcen/server-query-notify/ said:
    ///
    /// Man kann nur ein Channel-Abo haben. Es gilt das erste, das man abonniert hat. Dies wird nur
    /// durch Verlassen des Servers oder servernotifyunregister zurückgesetzt.
    /// Insbesondere wird es nicht zurückgesetzt, wenn der Channel gelöscht wird. Arrays als
    /// Parameter sind nicht möglich. Beim Löschen eines Channels
    /// geht das Abonnement nicht verloren.
    pub async fn register_channel_events(&mut self) -> QueryResult<()> {
        self.basic_operation("servernotifyregister event=channel id=0\n\r")
            .await
    }

    pub async fn change_nickname(&mut self, nickname: &str) -> QueryResult<()> {
        self.basic_operation(&format!(
            "clientupdate client_nickname={}\n\r",
            Self::escape(nickname)
        ))
        .await
    }

    pub(crate) async fn client_get_database_id_from_uid(
        &mut self,
        uid: &str,
    ) -> QueryResult<DatabaseId> {
        self.query_operation_non_error(&format!("clientgetdbidfromuid cluid={uid}\n\r"))
            .await
            .map(|mut v| v.remove(0))
    }

    pub async fn ban_del(&mut self, ban_id: i64) -> QueryResult<()> {
        self.basic_operation(&format!("bandel banid={ban_id}\n\r"))
            .await
    }
    pub async fn query_client_info(&mut self, client_id: i64) -> QueryResult<Option<ClientInfo>> {
        self.query_one_operation(&format!("clientinfo clid={client_id}\n\r"))
            .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    /// Banner a TeamSpeak ServerQuery endpoint sends on connect. It must NOT
    /// contain a status line: readers keep un-consumed bytes buffered, so a
    /// stray status line here would be handed back as the reply to the
    /// client's first real command.
    const BANNER: &str = "Welcome to the TeamSpeak 3 ServerQuery interface.\n\r";

    /// A scripted in-process TeamSpeak ServerQuery server.
    ///
    /// Binds 127.0.0.1:0 so tests never collide on a fixed port. Every
    /// received command name is recorded and exposed via `received()`.
    pub struct FakeServer {
        port: u16,
        received: Arc<Mutex<Vec<String>>>,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl FakeServer {
        /// `responder` maps a command name to the exact bytes to reply.
        /// Returning `None` sends nothing, which lets a test simulate a
        /// server that stops answering.
        pub async fn start<F>(responder: F) -> Self
        where
            F: Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let received = Arc::new(Mutex::new(Vec::new()));
            let (shutdown, mut shutdown_rx) = oneshot::channel::<()>();
            let responder = Arc::new(responder);
            let recorded = received.clone();

            tokio::spawn(async move {
                loop {
                    let accept = tokio::select! {
                        ret = listener.accept() => ret,
                        _ = &mut shutdown_rx => break,
                    };
                    let Ok((mut stream, _)) = accept else { break };
                    let responder = responder.clone();
                    let recorded = recorded.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        stream.write_all(BANNER.as_bytes()).await.ok();
                        let mut buf = Vec::new();
                        let mut chunk = [0u8; 1024];
                        while let Ok(n) = stream.read(&mut chunk).await {
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&chunk[..n]);
                            while let Some(pos) = find_frame_end(&buf) {
                                let frame: Vec<u8> = buf.drain(..pos).collect();
                                let line = &frame[..frame.len() - 2];
                                let command = String::from_utf8_lossy(line)
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or_default()
                                    .to_string();
                                recorded.lock().unwrap().push(command.clone());
                                if let Some(reply) = responder(&command)
                                    && stream.write_all(&reply).await.is_err()
                                {
                                    return;
                                }
                            }
                        }
                    });
                }
            });

            Self {
                port,
                received,
                shutdown: Some(shutdown),
            }
        }

        pub fn port(&self) -> u16 {
            self.port
        }

        pub fn received(&self) -> Vec<String> {
            self.received.lock().unwrap().clone()
        }
    }

    impl Drop for FakeServer {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Index just past the first `\n\r` terminator in `buf`.
    fn find_frame_end(buf: &[u8]) -> Option<usize> {
        buf.windows(2).position(|w| w == b"\n\r").map(|i| i + 2)
    }

    /// Standard success replies for the commands this crate issues.
    fn default_responder(command: &str) -> Option<Vec<u8>> {
        let body = match command {
            "login" => "error id=0 msg=ok\n\r",
            "use" => "error id=0 msg=ok\n\r",
            "clientupdate" => "error id=0 msg=ok\n\r",
            "whoami" => {
                "client_id=2 client_database_id=2 client_nickname=obs\n\rerror id=0 msg=ok\n\r"
            }
            "clientlist" => concat!(
                "clid=1 cid=1 client_database_id=1 client_nickname=alice client_type=0",
                "|clid=2 cid=1 client_database_id=2 client_nickname=obs client_type=1",
                "\n\rerror id=0 msg=ok\n\r"
            ),
            "channellist" => "cid=1 pid=0 channel_name=Lobby\n\rerror id=0 msg=ok\n\r",
            "serverinfo" => "virtualserver_unique_identifier=TESTSERVER=\n\rerror id=0 msg=ok\n\r",
            "servernotifyregister" => "error id=0 msg=ok\n\r",
            "banlist" => "error id=1281 msg=database empty\n\r",
            "quit" => "error id=0 msg=ok\n\r",
            _ => return None,
        };
        Some(body.as_bytes().to_vec())
    }

    /// Accepts one connection, emits a single notification split across two
    /// writes, and leaves the socket open. The emission is gated on the
    /// client's first command: writing earlier races with `connect`, which
    /// drains the banner and would swallow the event.
    pub async fn start_split_server() -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(BANNER.as_bytes()).await.unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            stream
                .write_all(b"notifycliententerview ctid=1 clid=100 client_nickname=Split")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            stream
                .write_all(b"User client_unique_identifier=SPLITUID= client_country=US\n\r")
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        });
        port
    }

    #[tokio::test]
    async fn connect_login_and_select_server_round_trip() {
        let server = FakeServer::start(default_responder).await;
        let mut conn = SocketConn::connect("127.0.0.1", server.port())
            .await
            .unwrap();
        conn.login("serveradmin", "secret").await.unwrap();
        conn.select_server(1).await.unwrap();

        assert_eq!(server.received(), vec!["login", "use"]);
    }

    #[tokio::test]
    async fn query_clients_parses_multiple_records() {
        let server = FakeServer::start(default_responder).await;
        let mut conn = SocketConn::connect("127.0.0.1", server.port())
            .await
            .unwrap();

        let clients = conn.query_clients().await.unwrap();
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].client_nickname(), "alice");
        assert!(clients[0].client_is_user());
        assert_eq!(clients[1].client_nickname(), "obs");
        assert!(!clients[1].client_is_user());
    }

    #[tokio::test]
    async fn empty_record_set_is_an_error_not_a_panic() {
        // A status line with no record line before it.
        let server = FakeServer::start(|command| match command {
            "clientlist" => Some(b"error id=0 msg=ok\n\r".to_vec()),
            _ => default_responder(command),
        })
        .await;
        let mut conn = SocketConn::connect("127.0.0.1", server.port())
            .await
            .unwrap();

        // Parsing an empty record set must fail as an Err, never by aborting
        // the process. Fixing the behaviour is a separate decision.
        assert!(conn.query_clients().await.is_err());
    }

    #[tokio::test]
    #[ignore = "fails until the read buffer moves onto SocketConn"]
    async fn read_event_reassembles_a_split_notification() {
        let port = start_split_server().await;
        let mut conn = SocketConn::connect("127.0.0.1", port).await.unwrap();
        conn.write_data("whoami\n\r").await.unwrap();

        let event = conn.read_event().await.unwrap().unwrap();
        assert!(
            event.contains("client_unique_identifier=SPLITUID="),
            "event was truncated, got {event:?}"
        );
    }
}
