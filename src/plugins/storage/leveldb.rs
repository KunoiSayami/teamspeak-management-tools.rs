use helper_generator::Helper;
use tokio::sync::{mpsc::Receiver, oneshot};

pub type OnceSender<T> = tokio::sync::oneshot::Sender<T>;
pub use rusty_leveldb::Result;

use super::KVMap;

#[derive(Clone, Debug)]
pub struct ConnProxier(DatabaseHelper);

impl From<DatabaseHelper> for ConnProxier {
    fn from(value: DatabaseHelper) -> Self {
        Self(value)
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
        let mut opt = rusty_leveldb::Options::default();
        opt.create_if_missing = true;
        opt
    }

    pub fn new(file: &str) -> (ConnProxier, Self) {
        let (sender, receiver) = DatabaseHelper::new(2048);

        (
            sender.clone().into(),
            Self {
                conn: sender,
                handle: std::thread::Builder::new()
                    .name(String::from("LevelDB thread"))
                    .spawn(move || Self::run(file, receiver))
                    .expect("Fail to spawn thread"),
            },
        )
    }

    pub fn run(file: &str, mut recv: Receiver<DatabaseEvent>) -> Result<()> {
        let mut db = rusty_leveldb::DB::open(file, Self::opt())?;
        while let Some(event) = recv.blocking_recv() {
            match event {
                DatabaseEvent::Set(k, v, sender) => {
                    let ret = db.put(k.as_bytes(), v.as_bytes());
                    sender.send(ret);
                    db.flush()?;
                }
                DatabaseEvent::Get(k, sender) => {
                    sender.send(
                        db.get(k.as_bytes())
                            .map_or(Ok(None), |bytes| String::from_utf8(bytes).map(|s| Some(s))),
                    );
                }
                DatabaseEvent::Delete(k, sender) => {
                    sender.send(db.delete(k.as_bytes()));
                    db.flush()?;
                }
                DatabaseEvent::Exit => break,
            }
        }
        Ok(())
    }

    pub fn get_connection(&self) -> ConnProxier {
        ConnProxier(self.conn.clone())
    }
}

#[async_trait::async_trait]
impl KVMap for ConnProxier {
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

    async fn close(self) -> anyhow::Result<()> {
        self.0.exit().await;
        /* for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if self.handle.is_finished() {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("Not exit after 3 seconds")) */
        Ok(())
    }
}
