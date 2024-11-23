use super::{ForkConnection, KVMap};
use anyhow::anyhow;
use redis::AsyncCommands;

pub struct RedisConn {
    conn: redis::Client,
}

impl RedisConn {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let redis =
            redis::Client::open(url).map_err(|e| anyhow!("Connect redis server error! {e:?}"))?;
        Ok(Self { conn: redis })
    }
}

pub struct RedisAgent {
    conn: redis::aio::MultiplexedConnection,
}

#[async_trait::async_trait]
impl ForkConnection for RedisConn {
    async fn fork(&self) -> anyhow::Result<Box<dyn KVMap>> {
        let conn = self
            .conn
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| anyhow!("Get redis connection error: {e:?}"))?;
        Ok(Box::new(RedisAgent { conn }))
    }
}

#[async_trait::async_trait]
impl KVMap for RedisAgent {
    async fn set(&mut self, key: String, value: String) -> anyhow::Result<Option<()>> {
        Ok(self.conn.set(key.to_string(), value.to_string()).await?)
    }

    async fn delete(&mut self, key: String) -> anyhow::Result<()> {
        // https://github.com/redis-rs/redis-rs/issues/1228
        let _: () = self.conn.del(key.to_string()).await?;
        Ok(())
    }

    async fn get(&mut self, key: String) -> anyhow::Result<Option<String>> {
        Ok(__self
            .conn
            .get::<_, Option<String>>(key.to_string())
            .await?)
    }
}
