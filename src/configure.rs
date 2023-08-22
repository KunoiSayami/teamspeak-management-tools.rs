pub mod config {
    use crate::plugins::kv::current::KVMap;
    use crate::DEFAULT_LEVELDB_LOCATION;
    use anyhow::anyhow;
    use serde_derive::Deserialize;
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::fs::read_to_string;
    use std::path::Path;

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

        #[deprecated]
        #[allow(unused)]
        pub fn redis_server(&self) -> String {
            if let Some(server) = &self.redis_server {
                server.clone()
            } else {
                String::from("redis://127.0.0.1")
            }
        }

        pub async fn get_kv_map(&self) -> anyhow::Result<KVMap> {
            if let Some(redis) = &self.redis_server {
                return KVMap::new_redis(redis).await;
            }

            KVMap::new_leveldb(if let Some(db) = &self.leveldb {
                db
            } else {
                DEFAULT_LEVELDB_LOCATION
            })
            .await
        }

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
        #[allow(unused)]
        #[deprecated(since = "4.0.0")]
        systemd: Option<bool>,
    }

    impl Misc {
        pub fn interval(&self) -> u64 {
            self.interval.unwrap_or(5)
        }

        #[allow(unused, deprecated)]
        #[deprecated(since = "4.0.0")]
        pub fn systemd(&self) -> bool {
            self.systemd.unwrap_or(false)
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
                "{}:{} {}",
                self.raw_query.server(),
                self.raw_query.port(),
                self.server.server_id.unwrap_or(1)
            )
        }

        pub fn mute_porter(&self) -> &MutePorter {
            &self.mute_porter
        }
    }

    impl TryFrom<&Path> for Config {
        type Error = anyhow::Error;

        fn try_from(path: &Path) -> Result<Self, Self::Error> {
            let content = read_to_string(path).map_err(|e| anyhow!("Read error: {:?}", e))?;

            toml::from_str(&content).map_err(|e| anyhow!("Deserialize toml error: {:?}", e))
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
