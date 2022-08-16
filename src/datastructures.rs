pub trait FromQueryString: for<'de> Deserialize<'de> {
    fn from_query(data: &str) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        serde_teamspeak_querystring::from_str(data)
            .map_err(|e| anyhow::anyhow!("Got parser error: {:?}", e))
    }
}

pub mod whoami {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct WhoAmI {
        client_id: i64,
        client_database_id: i64,
    }

    impl WhoAmI {
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
    }

    impl FromQueryString for WhoAmI {}
}

pub mod create_channel {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct CreateChannel {
        cid: i64,
    }

    impl CreateChannel {
        pub fn cid(&self) -> i64 {
            self.cid
        }
    }

    impl FromQueryString for CreateChannel {}
}

pub mod channel {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[allow(dead_code)]
    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Channel {
        cid: i64,
        pid: i64,
        channel_order: i64,
        channel_name: String,
        total_clients: i64,
        channel_needed_subscribe_power: i64,
    }

    #[allow(dead_code)]
    impl Channel {
        pub fn cid(&self) -> i64 {
            self.cid
        }
        pub fn pid(&self) -> i64 {
            self.pid
        }
        pub fn channel_order(&self) -> i64 {
            self.channel_order
        }
        pub fn channel_name(&self) -> &str {
            &self.channel_name
        }
        pub fn total_clients(&self) -> i64 {
            self.total_clients
        }
        pub fn channel_needed_subscribe_power(&self) -> i64 {
            self.channel_needed_subscribe_power
        }
    }

    impl FromQueryString for Channel {}
}

pub mod client {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[allow(dead_code)]
    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Client {
        clid: i64,
        cid: i64,
        client_database_id: i64,
        client_type: i64,
        //client_unique_identifier: String,
        client_nickname: String,
    }

    #[allow(dead_code)]
    impl Client {
        pub fn client_id(&self) -> i64 {
            self.clid
        }
        pub fn channel_id(&self) -> i64 {
            self.cid
        }
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
        pub fn client_type(&self) -> i64 {
            self.client_type
        }
        pub fn client_unique_identifier(&self) -> String {
            format!("{}", self.client_database_id)
        }
        pub fn client_nickname(&self) -> &str {
            &self.client_nickname
        }
    }

    impl FromQueryString for Client {}

    #[cfg(test)]
    mod test {
        use crate::datastructures::client::Client;
        use crate::datastructures::FromQueryString;

        const TEST_STRING: &str = "clid=8 cid=1 client_database_id=1 client_nickname=serveradmin client_type=1 client_unique_identifier=serveradmin";

        #[test]
        fn test() {
            let result = Client::from_query(TEST_STRING).unwrap();
            assert_eq!(result.client_id(), 8);
            assert_eq!(result.channel_id(), 1);
            assert_eq!(result.client_database_id(), 1);
            assert_eq!(result.client_nickname(), "serveradmin".to_string());
            assert_eq!(result.client_type(), 1);
            //assert_eq!(result.client_unique_identifier(), "serveradmin".to_string());
            assert_eq!(result.client_unique_identifier(), "1".to_string());
        }
    }
}

pub mod notifies {
    use crate::datastructures::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Copy, Clone, Debug)]
    pub struct ClientBasicInfo {
        channel_id: i64,
        client_id: i64,
    }

    impl ClientBasicInfo {
        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
    }

    impl From<NotifyClientMovedView> for ClientBasicInfo {
        fn from(view: NotifyClientMovedView) -> Self {
            Self {
                channel_id: view.channel_id(),
                client_id: view.client_id(),
            }
        }
    }

    impl From<NotifyClientEnterView> for ClientBasicInfo {
        fn from(view: NotifyClientEnterView) -> Self {
            Self {
                channel_id: view.channel_id(),
                client_id: view.client_id(),
            }
        }
    }

    #[allow(dead_code)]
    #[derive(Clone, Debug, Deserialize)]
    pub struct NotifyClientMovedView {
        #[serde(rename = "ctid")]
        channel_id: i64,
        #[serde(rename = "reasonid", default)]
        reason_id: i64,
        #[serde(rename = "invokerid", default)]
        invoker_id: i64,
        #[serde(rename = "invokeruid", default)]
        invoker_uid: String,
        #[serde(rename = "invokername", default)]
        invoker_name: String,
        #[serde(rename = "clid", default)]
        client_id: i64,
    }

    #[allow(dead_code)]
    impl NotifyClientMovedView {
        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
        pub fn reason_id(&self) -> i64 {
            self.reason_id
        }
        pub fn invoker_id(&self) -> i64 {
            self.invoker_id
        }
        pub fn invoker_uid(&self) -> &str {
            &self.invoker_uid
        }
        pub fn invoker_name(&self) -> &str {
            &self.invoker_name
        }
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct NotifyClientEnterView {
        #[serde(rename = "clid")]
        client_id: i64,
        #[serde(rename = "ctid")]
        channel_id: i64,
        client_nickname: String,
        client_unique_identifier: String,
        client_country: String,
    }

    impl NotifyClientEnterView {
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
        pub fn client_nickname(&self) -> &str {
            &self.client_nickname
        }
        pub fn client_country(&self) -> &str {
            &self.client_country
        }
        pub fn client_unique_identifier(&self) -> &str {
            &self.client_unique_identifier
        }

        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
    }

    fn default_left_reason_id() -> i64 {
        8
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct NotifyClientLeftView {
        #[serde(rename = "clid")]
        client_id: i64,
        #[serde(rename = "reasonmsg", default)]
        reason: String,
        #[serde(rename = "reasonid", default = "default_left_reason_id")]
        reason_id: i64,
        /*#[serde(rename = "invokerid", default)]
        invoker_id: i64,*/
        #[serde(rename = "invokeruid", default)]
        invoker_uid: String,
        #[serde(rename = "invokername", default)]
        invoker_name: String,
    }

    impl NotifyClientLeftView {
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
        pub fn reason(&self) -> &str {
            &self.reason
        }
        pub fn reason_id(&self) -> i64 {
            self.reason_id
        }
        pub fn invoker_uid(&self) -> &str {
            &self.invoker_uid
        }
        pub fn invoker_name(&self) -> &str {
            &self.invoker_name
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct NotifyTextMessage {
        /*#[serde(rename = "targetmode", default)]
        target_mode: i8,*/
        msg: String,
        //target: i64,
        #[serde(rename = "invokerid", default)]
        invoker_id: i64,
        #[serde(rename = "invokername", default)]
        invoker_name: String,
        #[serde(rename = "invokeruid", default)]
        invoker_uid: String,
    }

    impl NotifyTextMessage {
        /*pub fn target_mode(&self) -> i8 {
            self.target_mode
        }*/
        pub fn msg(&self) -> &str {
            &self.msg
        }
        /*pub fn target(&self) -> i64 {
            self.target
        }*/
        pub fn invoker_name(&self) -> &str {
            &self.invoker_name
        }
        pub fn invoker_uid(&self) -> &str {
            &self.invoker_uid
        }
        pub fn invoker_id(&self) -> i64 {
            self.invoker_id
        }
    }

    impl FromQueryString for NotifyClientMovedView {}
    impl FromQueryString for NotifyClientEnterView {}
    impl FromQueryString for NotifyClientLeftView {}
    impl FromQueryString for NotifyTextMessage {}
}

pub mod query_status {
    use crate::datastructures::{QueryError, QueryResult};
    use anyhow::anyhow;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct WebQueryStatus {
        code: i32,
        message: String,
    }

    impl WebQueryStatus {
        pub fn into_status(self) -> QueryStatus {
            QueryStatus {
                id: self.code,
                msg: self.message,
            }
        }
    }

    impl From<WebQueryStatus> for QueryStatus {
        fn from(status: WebQueryStatus) -> Self {
            status.into_status()
        }
    }

    #[allow(dead_code)]
    #[derive(Clone, Debug, Deserialize)]
    pub struct QueryStatus {
        id: i32,
        msg: String,
    }

    impl Default for QueryStatus {
        fn default() -> Self {
            Self {
                id: 0,
                msg: "ok".to_string(),
            }
        }
    }

    impl QueryStatus {
        pub fn id(&self) -> i32 {
            self.id
        }
        pub fn msg(&self) -> &String {
            &self.msg
        }

        pub fn into_err(self) -> QueryError {
            QueryError::from(self)
        }

        pub fn into_result<T>(self, ret: T) -> QueryResult<T> {
            if self.id == 0 {
                return Ok(ret);
            }
            Err(self.into_err())
        }
    }

    impl TryFrom<&str> for QueryStatus {
        type Error = anyhow::Error;

        fn try_from(value: &str) -> Result<Self, Self::Error> {
            let (_, line) = value
                .split_once("error ")
                .ok_or_else(|| anyhow!("Split error: {}", value))?;
            serde_teamspeak_querystring::from_str(line)
                .map_err(|e| anyhow!("Got error while parse string: {:?} {:?}", line, e))
        }
    }
}

pub mod server_info {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct ServerInfo {
        #[serde(rename = "virtualserver_unique_identifier")]
        virtual_server_unique_identifier: String,
    }

    impl ServerInfo {
        pub fn virtual_server_unique_identifier(&self) -> &str {
            &self.virtual_server_unique_identifier
        }
    }

    impl FromQueryString for ServerInfo {}
}

pub mod client_query_result {

    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Deserialize)]
    pub struct DatabaseId {
        /*#[serde(rename = "cluid")]
        client_unique_id: String,*/
        #[serde(rename = "cldbid")]
        client_database_id: i64,
    }

    impl DatabaseId {
        /*pub fn client_unique_id(&self) -> &str {
            &self.client_unique_id
        }*/
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
    }

    impl FromQueryString for DatabaseId {}
}

pub mod ban_entry {
    use super::FromQueryString;
    use serde_derive::Deserialize;
    use std::fmt::{Display, Formatter};

    #[derive(Clone, Debug, Deserialize)]
    pub struct BanEntry {
        #[serde(rename = "banid")]
        ban_id: i64,
        #[serde(default)]
        ip: String,
        #[serde(default)]
        reason: String,
        /*#[serde(rename = "invokercldbid", default)]
        invoker_client_database_id: i64,*/
        #[serde(rename = "invokername", default)]
        invoker_name: String,
        #[serde(rename = "invokeruid", default)]
        invoker_uid: String,
        /*#[serde(rename = "lastnickname", default)]
        last_nickname: String,*/
    }

    impl BanEntry {
        pub fn ban_id(&self) -> i64 {
            self.ban_id
        }
        pub fn ip(&self) -> &str {
            &self.ip
        }
        pub fn reason(&self) -> &str {
            &self.reason
        }
        /*pub fn invoker_client_database_id(&self) -> i64 {
            self.invoker_client_database_id
        }*/
        pub fn invoker_name(&self) -> &str {
            &self.invoker_name
        }
        pub fn invoker_uid(&self) -> &str {
            &self.invoker_uid
        }
    }

    impl Display for BanEntry {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "id: {}, reason: {}, invoker: {}, operator: {}",
                self.ban_id(),
                self.reason(),
                self.invoker_uid(),
                self.invoker_name()
            )
        }
    }

    #[cfg(test)]
    mod test {
        use crate::datastructures::ban_entry::BanEntry;
        use crate::datastructures::client::Client;
        use crate::datastructures::FromQueryString;

        const TEST_STRING: &str = r#"banid=5 ip name uid=953jm1Ez3CvbAx7FKzb19zAQm48= mytsid 
        lastnickname=باب created=1541834015 duration=0 invokername=AdminUser invokercldbid=2 
        invokeruid=QuietTeamspeak= reason enforcements=0|banid=6 ip=1.1.1.1 
        name uid mytsid lastnickname=باب created=1541834015 duration=0 invokername=AdminUser 
        invokercldbid=2 invokeruid=QuietTeamspeak= reason enforcements=0|banid=12 
        ip=114.5.1.4 name uid=+1145141919810 mytsid 
        lastnickname=!\s\s\s\s\s\s\s\s\s\s\s\s\s\sValidname created=1549729305 duration=0 
        invokername=AdminUser invokercldbid=2 invokeruid=QuietTeamspeak= 
        reason=Spam enforcements=0"#;

        #[test]
        fn test() {
            TEST_STRING
                .split('|')
                .map(|s| BanEntry::from_query(s))
                .for_each(|entry| drop(entry));
        }
    }

    impl FromQueryString for BanEntry {}
}

pub mod config {
    use anyhow::anyhow;
    use serde_derive::Deserialize;
    use std::collections::HashMap;
    use std::fs::read_to_string;
    use std::path::Path;

    #[derive(Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum Numbers {
        Single(i64),
        Multiple(Vec<i64>),
    }

    impl Numbers {
        fn to_vec(&self) -> Vec<i64> {
            match self {
                Numbers::Single(id) => {
                    vec![*id]
                }
                Numbers::Multiple(ids) => ids.clone(),
            }
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Permission {
        channel_id: Numbers,
        map: Vec<(u64, i64)>,
    }

    impl Permission {
        pub fn channel_id(&self) -> &Numbers {
            &self.channel_id
        }
        pub fn map(&self) -> &Vec<(u64, i64)> {
            &self.map
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct RawQuery {
        server: Option<String>,
        port: Option<u16>,
        user: String,
        password: String,
    }

    impl RawQuery {
        pub fn server(&self) -> String {
            if let Some(server) = &self.server {
                server.clone()
            } else {
                String::from("127.0.0.1")
            }
        }
        pub fn port(&self) -> u16 {
            self.port.unwrap_or(10011)
        }
        pub fn user(&self) -> &str {
            &self.user
        }
        pub fn password(&self) -> &str {
            &self.password
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Server {
        server_id: Option<i64>,
        channel_id: Numbers,
        privilege_group_id: i64,
        redis_server: Option<String>,
        ignore_user: Option<Vec<String>>,
        whitelist_ip: Option<Vec<String>>,
    }

    impl Server {
        pub fn server_id(&self) -> i64 {
            self.server_id.unwrap_or(1)
        }
        pub fn channels(&self) -> Vec<i64> {
            self.channel_id.to_vec()
        }
        pub fn privilege_group_id(&self) -> i64 {
            self.privilege_group_id
        }
        pub fn redis_server(&self) -> String {
            if let Some(server) = &self.redis_server {
                server.clone()
            } else {
                String::from("redis://127.0.0.1")
            }
        }
        pub fn ignore_user_name(&self) -> Vec<String> {
            self.ignore_user.clone().unwrap_or_default()
        }
        pub fn whitelist_ip(&self) -> Vec<String> {
            self.whitelist_ip.clone().unwrap_or_default()
        }
    }

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Message {
        //channel_not_found: Option<String>,
        //create_channel: Option<String>,
        move_to_channel: Option<String>,
    }

    impl Message {
        /*pub fn channel_not_found(&self) -> String {
            self.channel_not_found
                .clone()
                .unwrap_or_else(|| "I can't find you channel.".to_string())
        }
        pub fn create_channel(&self) -> String {
            self.create_channel
                .clone()
                .unwrap_or_else(|| "Your Channel has been created!".to_string())
        }*/
        pub fn move_to_channel(&self) -> String {
            self.move_to_channel
                .clone()
                .unwrap_or_else(|| "You have been moved into your channel.".to_string())
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Telegram {
        api_key: String,
        api_server: Option<String>,
        target: i64,
    }

    impl Telegram {
        pub fn api_key(&self) -> &str {
            &self.api_key
        }
        pub fn api_server(&self) -> String {
            if let Some(server) = &self.api_server {
                return server.clone();
            }
            String::from("https://api.telegram.org/")
        }
        pub fn target(&self) -> i64 {
            self.target
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Misc {
        interval: Option<u64>,
        systemd: Option<bool>,
    }

    impl Misc {
        pub fn interval(&self) -> u64 {
            self.interval.unwrap_or(5)
        }

        pub fn systemd(&self) -> bool {
            self.systemd.unwrap_or(false)
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Config {
        server: Server,
        misc: Misc,
        custom_message: Option<Message>,
        permissions: Option<Vec<Permission>>,
        telegram: Telegram,
        raw_query: RawQuery,
    }

    impl Config {
        pub fn server(&self) -> &Server {
            &self.server
        }
        pub fn misc(&self) -> &Misc {
            &self.misc
        }
        pub fn raw_query(&self) -> &RawQuery {
            &self.raw_query
        }
        pub fn message(&self) -> Message {
            self.custom_message.clone().unwrap_or_default()
        }
        pub fn telegram(&self) -> &Telegram {
            &self.telegram
        }
        pub fn channel_permissions(&self) -> HashMap<i64, Vec<(u64, i64)>> {
            let mut m = Default::default();
            match &self.permissions {
                None => m,
                Some(permissions) => {
                    for permission in permissions {
                        match permission.channel_id() {
                            Numbers::Single(channel_id) => {
                                m.insert(*channel_id, permission.map().clone());
                            }
                            Numbers::Multiple(channel_ids) => {
                                for channel_id in channel_ids {
                                    m.insert(*channel_id, permission.map().clone());
                                }
                            }
                        }
                    }
                    m
                }
            }
        }
    }

    impl TryFrom<&Path> for Config {
        type Error = anyhow::Error;

        fn try_from(path: &Path) -> Result<Self, Self::Error> {
            let content = read_to_string(path).map_err(|e| anyhow!("Read error: {:?}", e))?;

            toml::from_str(&content).map_err(|e| anyhow!("Deserialize toml error: {:?}", e))
        }
    }
}

mod status_result {
    use crate::datastructures::QueryStatus;
    use anyhow::Error;
    use std::fmt::{Display, Formatter};

    pub type QueryResult<T> = Result<T, QueryError>;

    #[derive(Clone, Default, Debug)]
    pub struct QueryError {
        code: i32,
        message: String,
    }

    impl QueryError {
        pub fn static_empty_response() -> Self {
            Self {
                code: -1,
                message: "Expect result but none found.".to_string(),
            }
        }
        pub fn code(&self) -> i32 {
            self.code
        }
    }

    impl Display for QueryError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}({})", self.message, self.code)
        }
    }

    impl std::error::Error for QueryError {}

    impl From<QueryStatus> for QueryError {
        fn from(status: QueryStatus) -> Self {
            Self {
                code: status.id(),
                message: status.msg().clone(),
            }
        }
    }

    impl From<Error> for QueryError {
        fn from(s: Error) -> Self {
            Self {
                code: -2,
                message: s.to_string(),
            }
        }
    }
}

pub use ban_entry::BanEntry;
pub use channel::Channel;
pub use client::Client;
pub use client_query_result::DatabaseId;
pub use config::Config;
pub use create_channel::CreateChannel;
pub use notifies::{
    NotifyClientEnterView, NotifyClientLeftView, NotifyClientMovedView, NotifyTextMessage,
};
pub use query_status::{QueryStatus, WebQueryStatus};
use serde::Deserialize;
pub use server_info::ServerInfo;
pub use status_result::{QueryError, QueryResult};
pub use whoami::WhoAmI;
