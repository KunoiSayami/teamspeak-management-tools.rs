use log::warn;

use crate::DEFAULT_LEVELDB_LOCATION;

use self::leveldb::LevelDB;

mod leveldb;
pub mod redis;

pub trait MapType: std::fmt::Display + Send + Sync {}

#[async_trait::async_trait]
pub trait KVMap {
    async fn set(&mut self, key: String, value: String) -> anyhow::Result<Option<()>>;

    async fn delete(&mut self, key: String) -> anyhow::Result<()>;

    async fn get(&mut self, key: String) -> anyhow::Result<Option<String>>;

    async fn close(self) -> anyhow::Result<()>;
}

pub enum Backend {
    LevelDB(leveldb::LevelDB),
    Redis,
}

impl Backend {
    pub async fn new(
        redis_addr: Option<&String>,
        leveldb: Option<&String>,
    ) -> anyhow::Result<(Self, Box<dyn KVMap>)> {
        if let Some(redis_addr) = redis_addr {
            let m = redis::Redis::load(redis_addr).await?;

            Ok((Self::Redis, Box::new(m)))
        } else {
            let (conn, db) = LevelDB::new(leveldb.map(|x| x.as_str()).unwrap_or_else(|| {
                warn!("Should specify least one database backend, consider use leveldb=<file> in configure file");
                DEFAULT_LEVELDB_LOCATION
            }));

            Ok((Self::LevelDB(db), Box::new(conn)))
        }
    }
}
