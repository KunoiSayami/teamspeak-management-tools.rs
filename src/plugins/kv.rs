pub mod v1 {
    use anyhow::anyhow;
    use redis::AsyncCommands;
    use std::fmt::Display;

    enum KVTypes {
        Redis(redis::aio::Connection),
    }

    pub struct KVMap {
        inner: KVTypes,
    }

    impl KVMap {
        pub async fn new_redis(redis_server: &str) -> anyhow::Result<Self> {
            let redis = redis::Client::open(redis_server)
                .map_err(|e| anyhow!("Connect redis server error! {:?}", e))?
                .get_async_connection()
                .await
                .map_err(|e| anyhow!("Get redis connection error: {:?}", e))?;
            Ok(Self {
                inner: KVTypes::Redis(redis),
            })
        }

        pub async fn set<D: Display + Send + Sync, V: Display + Send + Sync>(
            &mut self,
            k: D,
            v: V,
        ) -> anyhow::Result<()> {
            match self.inner {
                KVTypes::Redis(ref mut redis) => {
                    redis.set(k.to_string(), v.to_string()).await?;
                }
            }
            Ok(())
        }

        pub async fn delete<D: Display + Send + Sync>(&mut self, k: D) -> anyhow::Result<()> {
            match self.inner {
                KVTypes::Redis(ref mut redis) => {
                    redis.del(k.to_string()).await?;
                }
            }
            Ok(())
        }

        pub async fn get<D: Display + Send + Sync>(
            &mut self,
            k: D,
        ) -> anyhow::Result<Option<String>> {
            Ok(match self.inner {
                KVTypes::Redis(ref mut redis) => {
                    redis.get::<_, Option<String>>(k.to_string()).await?
                }
            })
        }
    }
}

pub use v1 as current;
