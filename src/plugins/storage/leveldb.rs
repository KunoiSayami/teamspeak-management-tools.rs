use helper_generator::Helper;
use tokio::sync::{mpsc::Receiver, oneshot};

pub type OnceSender<T> = tokio::sync::oneshot::Sender<T>;
pub use rusty_leveldb::Result;

use super::{ForkConnection, KVMap};

#[derive(Clone, Debug)]
pub struct ConnAgent(DatabaseHelper);

impl From<DatabaseHelper> for ConnAgent {
    fn from(value: DatabaseHelper) -> Self {
        Self(value)
    }
}

#[async_trait::async_trait]
impl ForkConnection for ConnAgent {
    async fn fork(&self) -> anyhow::Result<Box<dyn KVMap>> {
        Ok(Box::new(self.clone()))
    }
}

pub struct LevelDB {
    conn: DatabaseHelper,
    handle: std::thread::JoinHandle<Result<()>>,
}

#[derive(Helper)]
pub enum DatabaseEvent {
    Set(String, String, OnceSender<Result<()>>),
    Get(
        String,
        OnceSender<std::result::Result<Option<String>, std::string::FromUtf8Error>>,
    ),
    Delete(String, OnceSender<Result<()>>),
    Exit,
}

impl LevelDB {
    pub fn opt() -> rusty_leveldb::Options {
        rusty_leveldb::Options {
            create_if_missing: true,
            ..Default::default()
        }
    }

    pub fn new(file: String) -> (ConnAgent, Self) {
        log::warn!("LevelDB is experimental feature, may need some additional check");
        Self::new_with_opt(file, Self::opt)
    }

    fn new_with_opt(file: String, opt_fn: fn() -> rusty_leveldb::Options) -> (ConnAgent, Self) {
        let (sender, receiver) = DatabaseHelper::new(2048);

        (
            sender.clone().into(),
            Self {
                conn: sender,
                handle: std::thread::Builder::new()
                    .name(String::from("LevelDB thread"))
                    .spawn(move || Self::run(&file, opt_fn, receiver))
                    .expect("Fail to spawn thread"),
            },
        )
    }

    pub fn run(
        file: &str,
        opt_fn: fn() -> rusty_leveldb::Options,
        mut recv: Receiver<DatabaseEvent>,
    ) -> Result<()> {
        let mut db = rusty_leveldb::DB::open(file, opt_fn())?;
        while let Some(event) = recv.blocking_recv() {
            match event {
                DatabaseEvent::Set(k, v, sender) => {
                    let ret = db.put(k.as_bytes(), v.as_bytes());
                    sender.send(ret).ok();
                    db.flush()?;
                }
                DatabaseEvent::Get(k, sender) => {
                    sender
                        .send(
                            db.get(k.as_bytes())
                                .map_or(Ok(None), |bytes| String::from_utf8(bytes).map(Some)),
                        )
                        .ok();
                }
                DatabaseEvent::Delete(k, sender) => {
                    sender.send(db.delete(k.as_bytes())).ok();
                    db.flush()?;
                }
                DatabaseEvent::Exit => break,
            }
        }
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    pub async fn exit(&self) -> Option<()> {
        self.conn.exit().await
    }
}

#[async_trait::async_trait]
impl KVMap for ConnAgent {
    async fn set(&mut self, key: String, value: String) -> anyhow::Result<Option<()>> {
        let (sender, receiver) = oneshot::channel();
        self.0.set(key.to_string(), value.to_string(), sender).await;
        receiver.await??;
        Ok(Some(()))
    }

    async fn delete(&mut self, key: String) -> anyhow::Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.0.delete(key.to_string(), sender).await;
        receiver.await??;
        Ok(())
    }

    async fn get(&mut self, key: String) -> anyhow::Result<Option<String>> {
        let (sender, receiver) = oneshot::channel();
        self.0.get(key.to_string(), sender).await;

        Ok(receiver.await??)
    }
}

#[cfg(test)]
mod test {
    use crate::plugins::{Backend, ForkConnection};

    use super::{ConnAgent, LevelDB};

    async fn async_test_leveldb(agent: ConnAgent) -> anyhow::Result<()> {
        let mut conn = agent.fork().await?;
        conn.set("key".to_string(), "value".to_string()).await?;
        assert_eq!(
            conn.get("key".to_string()).await?,
            Some("value".to_string())
        );
        conn.set("key".to_string(), "value".to_string()).await?;
        conn.delete("key".to_string()).await?;
        assert_eq!(conn.get("key".to_string()).await?, None);

        Ok(())
    }

    #[test]
    fn test_leveldb() {
        let (agent, db) = LevelDB::new_with_opt("db".to_string(), rusty_leveldb::in_memory);
        let backend = Backend::from(db);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async_test_leveldb(agent))
            .unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(backend.disconnect())
            .unwrap();
    }
}
