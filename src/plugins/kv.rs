pub mod v1 {
    use anyhow::anyhow;
    use redis::AsyncCommands;
    use std::fmt::Display;

    enum KVTypes {
        LevelDB(rusty_leveldb::AsyncDB),
        Redis(redis::aio::Connection),
    }

    pub struct KVMap {
        inner: KVTypes,
    }

    impl KVMap {
        pub async fn new_redis(redis_server: &str) -> anyhow::Result<Self> {
            let redis = redis::Client::open(redis_server)
                .map_err(|e| anyhow!("Connect redis server error! {:?}", e))?;
            let redis_conn = redis
                .get_async_connection()
                .await
                .map_err(|e| anyhow!("Get redis connection error: {:?}", e))?;
            Ok(Self {
                inner: KVTypes::Redis(redis_conn),
            })
        }

        pub async fn new_leveldb(level_db: &str) -> anyhow::Result<Self> {
            Self::new_leveldb_with_option(level_db, rusty_leveldb::Options::default()).await
        }

        pub(super) async fn new_leveldb_with_option(
            level_db: &str,
            options: rusty_leveldb::Options,
        ) -> anyhow::Result<Self> {
            let level_db = rusty_leveldb::AsyncDB::new(level_db, options)
                .map_err(|e| anyhow!("Connect to leveldb error: {:?}", e))?;
            Ok(Self {
                inner: KVTypes::LevelDB(level_db),
            })
        }

        pub async fn set<D: Display + Send + Sync>(&mut self, k: D, v: D) -> anyhow::Result<()> {
            match self.inner {
                KVTypes::LevelDB(ref mut db) => {
                    db.put(
                        k.to_string().as_bytes().to_vec(),
                        v.to_string().as_bytes().to_vec(),
                    )
                    .await?;
                }
                KVTypes::Redis(ref mut redis) => {
                    redis.set(k.to_string(), v.to_string()).await?;
                }
            }
            Ok(())
        }

        pub async fn delete<D: Display + Send + Sync>(&mut self, k: D) -> anyhow::Result<()> {
            match self.inner {
                KVTypes::LevelDB(ref mut db) => {
                    db.delete(k.to_string().as_bytes().to_vec()).await?;
                }
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
                KVTypes::LevelDB(ref mut db) => db
                    .get(k.to_string().as_bytes().to_vec())
                    .await?
                    .map(String::from_utf8)
                    .transpose()?,
                KVTypes::Redis(ref mut redis) => {
                    redis.get::<_, Option<String>>(k.to_string()).await?
                }
            })
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        async fn async_test() -> anyhow::Result<()> {
            let mut db =
                KVMap::new_leveldb_with_option("db.tmp", rusty_leveldb::in_memory()).await?;
            db.set("A", "114514").await?;
            assert_eq!(db.get("A").await?, Some("114514".to_string()));
            db.delete("A").await?;
            assert_eq!(db.get("A").await?, None);
            Ok(())
        }

        #[test]
        fn test() {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async_test())
                .unwrap()
        }
    }
}

pub use v1 as current;
