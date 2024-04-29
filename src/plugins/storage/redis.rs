use super::{KVMap, MapType};
use anyhow::anyhow;
use redis::AsyncCommands;
pub struct Redis {
    conn: redis::aio::MultiplexedConnection,
}

impl Redis {
    pub async fn load(url: &str) -> anyhow::Result<Self> {
        let redis = redis::Client::open(url)
            .map_err(|e| anyhow!("Connect redis server error! {:?}", e))?
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| anyhow!("Get redis connection error: {:?}", e))?;
        Ok(Self { conn: redis })
    }
}

#[async_trait::async_trait]
impl KVMap for Redis {
    async fn set(&mut self, key: String, value: String) -> anyhow::Result<Option<()>> {
        Ok(self.conn.set(key.to_string(), value.to_string()).await?)
    }

    async fn delete(&mut self, key: String) -> anyhow::Result<()> {
        self.conn.del(key.to_string()).await?;
        Ok(())
    }

    async fn get(&mut self, key: String) -> anyhow::Result<Option<String>> {
        Ok(__self
            .conn
            .get::<_, Option<String>>(key.to_string())
            .await?)
    }

    async fn close(self) -> anyhow::Result<()> {
        Ok(())
    }
}
