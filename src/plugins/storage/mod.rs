use log::warn;

use crate::DEFAULT_LEVEL_DB_LOCATION;

use self::leveldb::LevelDB;

mod leveldb;
pub mod redis;

//pub trait MapType: std::fmt::Display + Send + Sync {}

#[async_trait::async_trait]
pub trait KVMap: Send + Sync {
    async fn set(&mut self, key: String, value: String) -> anyhow::Result<Option<()>>;

    async fn delete(&mut self, key: String) -> anyhow::Result<()>;

    async fn get(&mut self, key: String) -> anyhow::Result<Option<String>>;
}

#[async_trait::async_trait]
pub trait ForkConnection {
    async fn fork(&self) -> anyhow::Result<Box<dyn KVMap>>;
}

pub enum Backend {
    LevelDB(leveldb::LevelDB),
    Redis,
}

impl Backend {
    pub async fn connect(
        redis_addr: Option<&String>,
        leveldb: Option<&String>,
    ) -> anyhow::Result<(Self, Box<dyn ForkConnection>)> {
        if let Some(redis_addr) = redis_addr {
            let m = redis::RedisConn::connect(redis_addr).await?;

            Ok((Self::Redis, Box::new(m)))
        } else {
            let (conn, db) = LevelDB::new(leveldb.map(|x| x.as_str()).unwrap_or_else(|| {
                warn!("Should specify least one database backend, consider use leveldb=<file> in configure file");
                DEFAULT_LEVEL_DB_LOCATION
            }).to_string());

            Ok((Self::LevelDB(db), Box::new(conn)))
        }
    }

    pub async fn disconnect(self) -> anyhow::Result<()> {
        if let Self::LevelDB(db) = self {
            if db.exit().await.is_none() {
                return Ok(());
            }
            for _ in 0..30 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if db.is_finished() {
                    return Ok(());
                }
            }
            Err(anyhow::anyhow!("Not exit after 3 seconds"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
impl From<leveldb::LevelDB> for Backend {
    fn from(value: leveldb::LevelDB) -> Self {
        Self::LevelDB(value)
    }
}
