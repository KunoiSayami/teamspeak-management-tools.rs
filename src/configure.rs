pub mod config {
    use anyhow::anyhow;
    use log::info;
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::fmt::Debug;
    use tokio::io::AsyncReadExt;

    use crate::plugins::{Backend, ForkConnection};

    const DEFAULT_TELEGRAM_SERVER: &str = "https://api.telegram.org/";

    #[derive(Clone, Debug, Deserialize)]
    #[serde(untagged)]
    pub enum Numbers {
        Single(i64),
        Multiple(Vec<i64>),
    }

    impl Numbers {
        fn get_vec(&self) -> Vec<i64> {
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
        #[serde(alias = "channel-id")]
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
        #[serde(alias = "server-id")]
        server_id: Option<i64>,
        #[serde(alias = "channel-id")]
        channel_id: Numbers,
        #[serde(alias = "privilege-group-id")]
        privilege_group_id: i64,
        #[serde(alias = "redis-server")]
        redis_server: Option<String>,
        leveldb: Option<String>,
        #[serde(alias = "ignore-user")]
        ignore_user: Option<Vec<String>>,
        #[serde(alias = "whitelist-ip")]
        whitelist_ip: Option<Vec<String>>,
        #[cfg(feature = "tracker")]
        #[serde(alias = "track-channel-member")]
        track_channel_member: Option<String>,
    }

    impl Server {
        pub fn server_id(&self) -> i64 {
            self.server_id.unwrap_or(1)
        }

        pub fn channels(&self) -> Vec<i64> {
            self.channel_id.get_vec()
        }

        pub fn privilege_group_id(&self) -> i64 {
            self.privilege_group_id
        }

        /* pub fn redis_server(&self) -> String {
            if let Some(server) = &self.redis_server {
                server.clone()
            } else {
                String::from("redis://127.0.0.1")
            }
        }*/

        pub fn ignore_user_name(&self) -> Vec<String> {
            self.ignore_user.clone().unwrap_or_default()
        }

        pub fn whitelist_ip(&self) -> Vec<String> {
            self.whitelist_ip.clone().unwrap_or_default()
        }

        #[cfg(feature = "tracker")]
        pub fn track_channel_member(&self) -> &Option<String> {
            &self.track_channel_member
        }
    }

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct Message {
        #[serde(alias = "move-to-channel")]
        move_to_channel: Option<String>,
    }

    impl Message {
        pub fn move_to_channel(&self) -> String {
            self.move_to_channel
                .clone()
                .unwrap_or_else(|| "You have been moved into your channel.".into())
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Telegram {
        #[serde(alias = "api-key", alias = "api")]
        api_key: String,
        #[serde(alias = "api-server")]
        api_server: Option<String>,
        target: i64,
        #[serde(default)]
        responsible: bool,
        #[serde(default, alias = "allowed-chat")]
        allowed_chat: Vec<i64>,
    }

    impl Telegram {
        pub fn api_key(&self) -> &str {
            &self.api_key
        }

        pub fn api_server(&self) -> String {
            if let Some(server) = &self.api_server {
                return server.clone();
            }
            DEFAULT_TELEGRAM_SERVER.into()
        }

        pub fn target(&self) -> i64 {
            self.target
        }

        pub fn responsible(&self) -> bool {
            self.responsible
        }

        pub fn allowed_chat(&self) -> &[i64] {
            &self.allowed_chat
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Misc {
        interval: Option<u64>,
    }

    impl Misc {
        pub fn interval(&self) -> u64 {
            self.interval.unwrap_or(5)
        }
    }

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct MutePorter {
        enable: bool,
        #[serde(alias = "monitor", alias = "monitor-channel")]
        monitor_channel: i64,
        #[serde(alias = "target", alias = "target-channel")]
        target_channel: i64,
        #[serde(default)]
        whitelist: Vec<i64>,
    }

    impl MutePorter {
        pub fn enable(&self) -> bool {
            self.enable
        }

        pub fn monitor_channel(&self) -> i64 {
            self.monitor_channel
        }

        pub fn target_channel(&self) -> i64 {
            self.target_channel
        }

        pub fn check_whitelist(&self, client_id: i64) -> bool {
            self.whitelist.contains(&client_id)
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    pub struct Config {
        server: Server,
        misc: Misc,
        #[serde(default, alias = "mute-porter")]
        mute_porter: MutePorter,
        #[serde(alias = "custom-message")]
        custom_message: Option<Message>,
        permissions: Option<Vec<Permission>>,
        telegram: Telegram,
        #[serde(alias = "raw-query")]
        raw_query: RawQuery,
        #[serde(default)]
        additional: Vec<String>,
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

        pub fn get_id(&self) -> String {
            format!(
                "{}:{}({})",
                Self::parse_server(&self.raw_query.server()),
                self.raw_query.port(),
                self.server.server_id.unwrap_or(1)
            )
        }

        fn parse_server(server: &str) -> &str {
            if server.eq("localhost") || server.eq("::1") || server.eq("127.0.0.1") {
                return "";
            }
            server
        }

        pub fn mute_porter(&self) -> &MutePorter {
            &self.mute_porter
        }

        pub fn additional(&self) -> &[String] {
            &self.additional
        }

        pub async fn load_config(path: String) -> anyhow::Result<Vec<(String, Self)>> {
            let p_config = Self::load(&path).await?;
            let id = Self::config_xxhash(p_config.get_id().as_bytes());

            info!("Load {path:?} as {id:?}");
            let mut ret = vec![(id, p_config.clone())];

            for path in p_config.additional() {
                let config = Self::load(path).await.inspect_err(|e| {
                    log::error!("Load additional configure {path:?} error: {e:?}")
                })?;
                let id = Self::config_xxhash(config.get_id().as_bytes());
                info!("Load {path:?} as {id:?}");
                ret.push((id, config));
            }

            Ok(ret)
        }

        pub fn config_xxhash(input: &[u8]) -> String {
            format!("{:08x}", xxhash_rust::xxh3::xxh3_64(input))
        }

        pub async fn load(path: &str) -> anyhow::Result<Self> {
            let mut file = tokio::fs::File::open(path).await?;
            let mut buf = String::new();

            file.read_to_string(&mut buf).await?;
            toml::from_str(&buf).map_err(|e| {
                // Never format the toml error with `{e:?}`: its Debug output
                // embeds the whole input, which includes raw-query.password.
                // `message()` is the reason alone, BUT for serde type errors
                // the message embeds the offending literal, which may itself
                // be a credential (e.g. an unquoted numeric password). Strip
                // the value and keep only the expected type.
                let message = e.message().to_string();
                let message = if message.starts_with("invalid type: ") {
                    match message.find(", expected ") {
                        Some(pos) => {
                            format!("invalid type [value omitted]{}", &message[pos..])
                        }
                        None => "type mismatch [value omitted]".to_string(),
                    }
                } else {
                    message
                };
                match e.span().filter(|span| span.start < span.end) {
                    Some(span) => {
                        // Byte offset to 1-based line and column within it.
                        let before = &buf[..span.start.min(buf.len())];
                        let line = before.matches('\n').count() + 1;
                        let column = before
                            .rfind('\n')
                            .map(|i| before.len() - i)
                            .unwrap_or(before.len() + 1);
                        anyhow!("Unable to parse config: {message}")
                            .context(format!("{path}:{line}:{column}"))
                    }
                    None => anyhow!("Unable to parse config {path}: {message}"),
                }
            })
        }

        pub async fn load_kv_map(&self) -> anyhow::Result<(Backend, Box<dyn ForkConnection>)> {
            Backend::connect(
                self.server.redis_server.as_ref(),
                self.server.leveldb.as_ref(),
            )
            .await
        }
    }

    /*impl TryFrom<dyn AsRef<Path>> for Config {
        type Error = anyhow::Error;

        fn try_from(value: Box<dyn AsRef<Path>>) -> Result<Self, Self::Error> {
            Config::try_from(value.as_ref())
        }
    }*/
}

pub use config::Config;

#[cfg(test)]
mod test {
    use super::Config;

    /// A config with one deliberately invalid value and a secret in
    /// `raw-query.password`. Only the invalid value may appear in the error.
    const BAD_CONFIG: &str = r#"
[server]
channel-id = [1]
privilege-group-id = 5
leveldb = "./level.db"

[telegram]
api-key = ""
target = 0

[misc]
interval = 5

[raw-query]
server = "127.0.0.1"
port = "not-a-number"
user = "serveradmin"
password = "SUPERSECRETVALUE"
"#;

    fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("tsmt-configure-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// `main` returns anyhow::Result, and Rust's Termination impl prints the
    /// error with Debug - so assert on Debug, matching production output.
    #[tokio::test]
    async fn parse_error_does_not_include_the_password() {
        let path = write_temp("bad-password.toml", BAD_CONFIG);
        let err = Config::load(path.to_str().unwrap()).await.unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            !rendered.contains("SUPERSECRETVALUE"),
            "config error leaked the password: {rendered}"
        );
        assert!(
            rendered.contains("invalid type"),
            "config error is uninformative: {rendered}"
        );
    }

    #[tokio::test]
    async fn parse_error_reports_a_location_for_type_errors() {
        // A wrong type is the case where toml supplies a usable span. Derive
        // the expected line from the fixture so editing it cannot break the
        // assertion for the wrong reason.
        let expected_line = BAD_CONFIG
            .lines()
            .position(|line| line.starts_with("port ="))
            .expect("fixture must contain the invalid port")
            + 1;

        let path = write_temp("bad-location.toml", BAD_CONFIG);
        let err = Config::load(path.to_str().unwrap()).await.unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains(&format!(":{expected_line}:")),
            "type error should name line {expected_line}, got: {rendered}"
        );
    }

    #[tokio::test]
    async fn secret_field_itself_is_not_echoed_when_it_is_the_invalid_value() {
        // If password is the mistyped field, serde's invalid_type message
        // embeds the literal. The redaction must strip it.
        let contents = "[server]\nchannel-id = [1]\nprivilege-group-id = 5\n\
                        leveldb = \"./level.db\"\n[telegram]\napi-key = \"\"\ntarget = 0\n\
                        [misc]\ninterval = 5\n[raw-query]\nserver = \"127.0.0.1\"\n\
                        port = 10011\nuser = \"serveradmin\"\npassword = 987654321\n";
        let path = write_temp("secret-as-value.toml", contents);
        let err = Config::load(path.to_str().unwrap()).await.unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            !rendered.contains("987654321"),
            "config error leaked the password value: {rendered}"
        );
        assert!(
            rendered.contains("value omitted"),
            "redacted error should say the value was omitted: {rendered}"
        );
    }

    #[tokio::test]
    async fn missing_field_error_does_not_invent_a_location() {
        // Missing fields yield a degenerate span (0..0); reporting path:1:1
        // would be misleading. The message alone must name the key.
        let contents = "[server]\nchannel-id = [1]\nprivilege-group-id = 5\n\
                        leveldb = \"./level.db\"\n[telegram]\napi-key = \"\"\ntarget = 0\n\
                        [misc]\ninterval = 5\n[raw-query]\nport = 10011\n\
                        password = \"SUPERSECRETVALUE\"\n";
        let path = write_temp("missing-field.toml", contents);
        let err = Config::load(path.to_str().unwrap()).await.unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            !rendered.contains("SUPERSECRETVALUE"),
            "config error leaked the password: {rendered}"
        );
        assert!(
            rendered.contains("missing field"),
            "error should name the missing field, got: {rendered}"
        );
        assert!(
            !rendered.contains(":1:1"),
            "should not report a bogus position for a degenerate span: {rendered}"
        );
    }
}
