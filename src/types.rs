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
/*
pub mod.rs channel {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    //#[allow(dead_code)]
    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Channel {
        cid: i64,
        pid: i64,
        channel_order: i64,
        channel_name: String,
        total_clients: i64,
        channel_needed_subscribe_power: i64,
    }

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
*/

// TODO: Rename this
mod client {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Client {
        #[serde(rename = "cid")]
        channel_id: i64,
        #[serde(rename = "clid")]
        client_id: i64,
        client_database_id: i64,
        client_type: i64,
        client_nickname: String,
    }

    impl Client {
        pub fn client_id(&self) -> i64 {
            self.client_id
        }
        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
        pub fn client_database_id(&self) -> i64 {
            self.client_database_id
        }
        pub fn client_type(&self) -> i64 {
            self.client_type
        }
        pub fn client_nickname(&self) -> &str {
            &self.client_nickname
        }
        pub fn client_is_user(&self) -> bool {
            self.client_type == 0
        }
    }

    impl FromQueryString for Client {}

    #[cfg(test)]
    mod test {
        use crate::types::client::Client;
        use crate::types::FromQueryString;

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
            //assert_eq!(result.client_database_id(), "1".to_string());
        }
    }
}

pub mod notifies {
    use crate::types::FromQueryString;
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

    #[derive(Clone, Debug, Deserialize)]
    pub struct NotifyClientMovedView {
        #[serde(rename = "ctid")]
        channel_id: i64,
        /*#[serde(rename = "reasonid", default)]
        reason_id: i64,
        #[serde(rename = "invokerid", default)]
        invoker_id: i64,*/
        #[cfg(feature = "tracker")]
        #[serde(rename = "invokeruid", default)]
        invoker_uid: String,
        #[cfg(feature = "tracker")]
        #[serde(rename = "invokername", default)]
        invoker_name: String,
        #[serde(rename = "clid", default)]
        client_id: i64,
    }

    impl NotifyClientMovedView {
        pub fn channel_id(&self) -> i64 {
            self.channel_id
        }
        /*pub fn reason_id(&self) -> i64 {
            self.reason_id
        }
        pub fn invoker_id(&self) -> i64 {
            self.invoker_id
        }*/
        #[cfg(feature = "tracker")]
        pub fn invoker_uid(&self) -> &str {
            &self.invoker_uid
        }
        #[cfg(feature = "tracker")]
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
    use crate::types::{QueryError, QueryResult};
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
        use super::BanEntry;
        use super::FromQueryString;

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

mod status_result {
    use crate::types::QueryStatus;
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

mod client_info {
    use super::FromQueryString;
    use serde_derive::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct ClientInfo {
        /*#[serde(rename = "clid")]
        channel_id: i64,
        #[serde(rename = "cid")]
        client_id: i64,*/
        client_input_muted: bool,
        client_output_muted: bool,
        /*#[serde(rename = "client_outputonly_muted")]
        client_output_only_muted: bool,*/
        client_input_hardware: bool,
        client_output_hardware: bool,
        //client_unique_identifier: String,
        client_away: bool,
        client_idle_time: i64,
    }

    impl ClientInfo {
        pub fn is_client_muted(&self) -> bool {
            self.client_away
                || self.client_input_muted
                || self.client_output_muted
                || !self.client_output_hardware
                || !self.client_input_hardware
                || self.client_idle_time / 1000 > 300
        }
    }

    impl FromQueryString for ClientInfo {}
}

mod pseudo_event_helper {
    use async_trait::async_trait;

    #[async_trait]
    pub trait EventHelperTrait {
        async fn insert(
            &self,
            client_id: i32,
            user_id: Option<String>,
            nickname: Option<String>,
            channel: Option<i32>,
        ) -> Option<()>;
        async fn terminate(&self) -> Option<()>;
    }

    #[cfg(not(feature = "tracker"))]
    #[derive(Clone, Debug)]
    pub struct PseudoEventHelper {}

    #[cfg(not(feature = "tracker"))]
    impl PseudoEventHelper {
        pub fn new() -> (Self, Self) {
            (Self {}, Self {})
        }

        pub async fn wait(self) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
            tokio::spawn(async { Ok(()) }).await
        }
    }

    #[cfg(not(feature = "tracker"))]
    #[async_trait]
    impl EventHelperTrait for PseudoEventHelper {
        async fn insert(
            &self,
            _client_id: i32,
            _user_id: Option<String>,
            _nickname: Option<String>,
            _channel: Option<i32>,
        ) -> Option<()> {
            Some(())
        }

        async fn terminate(&self) -> Option<()> {
            Some(())
        }
    }
}

mod queue {

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct MessageQueue<T>(Vec<T>);

    impl<T> MessageQueue<T> {
        /*pub fn len(&self) -> usize {
            self.inner.len()
        }*/
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        pub fn push(&mut self, element: T) {
            self.0.push(element)
        }

        pub fn new() -> Self {
            Self(Vec::new())
        }

        pub fn get_vec(&mut self) -> Vec<T> {
            std::mem::take(&mut self.0)
        }
    }
}

pub use ban_entry::BanEntry;
//pub use channel::Channel;
pub use client::Client;
pub use client_info::ClientInfo;
pub use client_query_result::DatabaseId;
pub use create_channel::CreateChannel;
pub use notifies::{
    NotifyClientEnterView, NotifyClientLeftView, NotifyClientMovedView, NotifyTextMessage,
};
pub use pseudo_event_helper::EventHelperTrait;
pub use queue::MessageQueue;

#[cfg(not(feature = "tracker"))]
pub use pseudo_event_helper::PseudoEventHelper;
pub use query_status::QueryStatus;
use serde::Deserialize;
pub use server_info::ServerInfo;
pub use status_result::{QueryError, QueryResult};
pub use whoami::WhoAmI;
