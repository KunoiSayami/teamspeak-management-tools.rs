use crate::datastructures::{
    Channel, Client, CreateChannel, DatabaseId, QueryError, QueryResult, ServerInfo, WhoAmI,
};
use crate::datastructures::{FromQueryString, QueryStatus};
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
            "Content => {:?}",
            content
        );

        for line in content.lines() {
            if line.trim().starts_with("error ") {
                let status = QueryStatus::try_from(line)?;

                return status.into_result(content);
            }
        }
        Err(QueryError::static_empty_response())
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
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut ret = String::new();
        loop {
            let size = if let Ok(data) =
                tokio::time::timeout(Duration::from_secs(2), self.conn.read(&mut buffer)).await
            {
                match data {
                    Ok(size) => size,
                    Err(e) => return Err(anyhow!("Got error while read data: {:?}", e)),
                }
            } else {
                return Ok(None);
            };

            ret.push_str(&String::from_utf8_lossy(&buffer[..size]));
            if size < BUFFER_SIZE || (ret.contains("error id=") && ret.ends_with("\n\r")) {
                break;
            }
        }
        Ok(Some(ret))
    }

    pub(crate) async fn write_data(&mut self, payload: &str) -> anyhow::Result<()> {
        debug_assert!(payload.ends_with("\n\r"));
        self.conn
            .write(payload.as_bytes())
            .await
            .map(|size| {
                if size != payload.as_bytes().len() {
                    error!(
                        "Error payload size mismatch! expect {} but {} found. payload: {:?}",
                        payload.as_bytes().len(),
                        size,
                        payload
                    )
                }
            })
            .map_err(|e| anyhow!("Got error while send data: {:?}", e))?;
        /*self.conn
        .flush()
        .await
        .map_err(|e| anyhow!("Got error while flush data: {:?}", e))?;*/
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
        Self::decode_status(data).map(|_| ())
    }

    async fn query_operation_non_error<T: FromQueryString + Sized>(
        &mut self,
        payload: &str,
    ) -> QueryResult<Vec<T>> {
        let data = self.write_and_read(payload).await?;
        let ret = Self::decode_status_with_result(data)?;
        Ok(ret
            .ok_or_else(|| panic!("Can't find result line, payload => {}", payload))
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

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace(' ', "\\s")
            .replace('/', "\\/")
    }

    pub async fn connect(server: &str, port: u16) -> anyhow::Result<Self> {
        let conn = TcpStream::connect(format!("{}:{}", server, port))
            .await
            .map_err(|e| anyhow!("Got error while connect to {}:{} {:?}", server, port, e))?;

        //let bufreader = BufReader::new(conn);
        //conn.set_nonblocking(true).unwrap();
        let mut self_ = Self { conn };

        let content = self_
            .read_data()
            .await
            .map_err(|e| anyhow!("Got error in connect while read content: {:?}", e))?;

        if content.is_none() {
            warn!("Read none data.");
        }

        Ok(self_)
    }

    pub async fn login(&mut self, user: &str, password: &str) -> QueryResult<()> {
        let payload = format!("login {} {}\n\r", user, password);
        self.basic_operation(payload.as_str()).await
    }

    pub async fn select_server(&mut self, server_id: i64) -> QueryResult<()> {
        let payload = format!("use {}\n\r", server_id);
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

    #[allow(dead_code)]
    async fn query_channels(&mut self) -> QueryResult<Vec<Channel>> {
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

    pub(crate) async fn move_client_to_channel(
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
            "channeladdperm cid={} {}",
            target_channel,
            permissions
                .iter()
                .map(|(k, v)| format!("permid={} permvalue={}\n\r", k, v))
                .collect::<Vec<String>>()
                .join("|")
        );
        self.basic_operation(&payload).await
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
        self.query_operation_non_error(&format!("clientgetdbidfromuid cluid={}\n\r", uid))
            .await
            .map(|mut v| v.remove(0))
    }

    pub async fn ban_del(&mut self, ban_id: i64) -> QueryResult<()> {
        self.basic_operation(&format!("bandel banid={}\n\r", ban_id))
            .await
    }
}
