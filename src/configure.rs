pub mod config {
    use anyhow::anyhow;
    use log::info;
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::fmt::Debug;
    use tap::TapFallible;
    use tokio::io::AsyncReadExt;

    use crate::plugins::{Backend, ForkConnection};

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
        leveldb: Option<String>,
        ignore_user: Option<Vec<String>>,
        whitelist_ip: Option<Vec<String>>,
        #[cfg(feature = "tracker")]
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
        move_to_channel: Option<String>,
    }

    impl Message {
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
    }

    impl Misc {
        pub fn interval(&self) -> u64 {
            self.interval.unwrap_or(5)
        }
    }

    #[derive(Clone, Debug, Default, Deserialize)]
    pub struct MutePorter {
        enable: bool,
        #[serde(rename = "monitor")]
        monitor_channel: i64,
        #[serde(rename = "target")]
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
        #[serde(default)]
        mute_porter: MutePorter,
        custom_message: Option<Message>,
        permissions: Option<Vec<Permission>>,
        telegram: Telegram,
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

            info!("Load {:?} as {:?}", &path, id);
            let mut ret = vec![(id, p_config.clone())];

            for path in p_config.additional() {
                let config = Self::load(path)
                    .await
                    .tap_err(|e| log::error!("Load additional configure {path:?} error: {e:?}"))?;
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
            toml::from_str(&buf).map_err(|e| anyhow!("Deserialize failure: {e:?}"))
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
