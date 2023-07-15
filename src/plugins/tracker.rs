pub type DatabaseResult<T> = Result<T, sqlx::Error>;

pub mod v1 {
    use super::DatabaseResult;
    use sqlx::SqliteConnection;

    pub const VERSION: &str = "1";

    pub(super) const CREATE_TABLE: &str = r#"
        CREATE TABLE "users" (
            "timestamp"	INTEGER NOT NULL,
            "client_id" INTEGER NOT NULL,
            "id"	TEXT,
            "nickname" TEXT,
            "channel"	INTEGER
        );
        
        CREATE TABLE "meta" (
            "key" TEXT NOT NULL,
            "value" TEXT
        );
        "#;

    pub(super) async fn insert(
        conn: &mut SqliteConnection,
        client_id: i32,
        user_id: Option<String>,
        nickname: Option<String>,
        channel: Option<i32>,
    ) -> DatabaseResult<()> {
        sqlx::query(r#"INSERT INTO "users" VALUES (?, ?, ?, ?, ?)"#)
            .bind(kstool::time::get_current_second() as i64)
            .bind(client_id)
            .bind(user_id)
            .bind(nickname)
            .bind(channel)
            .execute(conn)
            .await
            .map(|_| ())
    }
}

#[allow(unused)]
async fn update_database_version(conn: &mut SqliteConnection) -> DatabaseResult<()> {
    sqlx::query(r#"UPDATE "meta" SET "value" = ? WHERE "key" = 'version'"#)
        .bind(VERSION)
        .execute(conn)
        .await
        .map(|_| ())
}

async fn create_new_database(conn: &mut SqliteConnection) -> DatabaseResult<()> {
    sqlx::query(current::CREATE_TABLE)
        .execute(conn)
        .await
        .map(|_| ())
}

async fn insert_database_version(conn: &mut SqliteConnection) -> DatabaseResult<()> {
    sqlx::query(r#"INSERT INTO "meta" VALUES ("version", ?)"#)
        .bind(VERSION)
        .execute(conn)
        .await
        .map(|_| ())
}

async fn check_database(conn: &mut SqliteConnection) -> DatabaseResult<bool> {
    sqlx::query_as::<_, (i32,)>(
        r#"SELECT COUNT(*) FROM "sqlite_master" WHERE "type" = 'table' AND "name" = 'meta';"#,
    )
    .fetch_one(conn)
    .await
    .map(|(count,)| count > 0)
}

pub mod types {
    use crate::datastructures::EventHelperTrait;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    #[derive(Clone, Debug)]
    pub enum Event {
        Insert(i32, Option<String>, Option<String>, Option<i32>),
        Terminate,
    }

    #[derive(Clone)]
    pub struct EventHelper {
        sender: Option<mpsc::Sender<Event>>,
    }

    impl EventHelper {
        async fn send(&self, event: Event) -> Option<()> {
            if let Some(ref sender) = self.sender {
                sender.send(event).await.ok()?;
            }
            Some(())
        }
    }

    #[async_trait]
    impl EventHelperTrait for EventHelper {
        async fn insert(
            &self,
            client_id: i32,
            user_id: Option<String>,
            nickname: Option<String>,
            channel: Option<i32>,
        ) -> Option<()> {
            self.send(Event::Insert(client_id, user_id, nickname, channel))
                .await
        }

        async fn terminate(&self) -> Option<()> {
            self.send(Event::Terminate).await
        }
    }

    impl From<mpsc::Sender<Event>> for EventHelper {
        fn from(value: mpsc::Sender<Event>) -> Self {
            Self {
                sender: Some(value),
            }
        }
    }

    impl From<Option<mpsc::Sender<Event>>> for EventHelper {
        fn from(value: Option<mpsc::Sender<Event>>) -> Self {
            Self { sender: value }
        }
    }
}

pub mod handler {
    use super::types::EventHelper;
    use super::Event;
    use super::{check_database, create_new_database, insert_database_version, DatabaseResult};
    use anyhow::anyhow;
    use log::error;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::{ConnectOptions, SqliteConnection};
    use tap::TapFallible;
    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;

    #[derive(Debug)]
    pub struct DatabaseHelper {
        handler: JoinHandle<anyhow::Result<()>>,
    }

    impl DatabaseHelper {
        pub async fn new(filename: Option<String>) -> DatabaseResult<(Self, EventHelper)> {
            match filename {
                None => Self::create_empty().await,
                Some(filename) => Self::create(filename).await,
            }
        }

        async fn create(filename: String) -> DatabaseResult<(Self, EventHelper)> {
            let mut conn = SqliteConnectOptions::new()
                .filename(filename)
                .create_if_missing(true)
                .connect()
                .await?;
            if !check_database(&mut conn).await? {
                create_new_database(&mut conn).await?;
                insert_database_version(&mut conn).await?;
            }

            let (sender, receiver) = mpsc::channel(2048);

            Ok((
                tokio::spawn(Self::server(conn, receiver)).into(),
                sender.into(),
            ))
        }

        async fn server(
            mut conn: SqliteConnection,
            mut receiver: mpsc::Receiver<Event>,
        ) -> anyhow::Result<()> {
            while let Some(event) = receiver.recv().await {
                match event {
                    Event::Insert(client_id, user_id, nickname, channel) => {
                        super::current::insert(&mut conn, client_id, user_id, nickname, channel)
                            .await
                            .tap_err(|e| error!("Unable insert to database: {:?}", e))
                            .ok();
                    }
                    Event::Terminate => {
                        break;
                    }
                }
            }
            Ok(())
        }

        async fn create_empty() -> DatabaseResult<(Self, EventHelper)> {
            Ok((tokio::spawn(async { Ok(()) }).into(), None.into()))
        }

        pub async fn safe_new(
            filename: Option<String>,
            error_handler: fn(&sqlx::Error) -> (),
        ) -> (Self, EventHelper) {
            match Self::new(filename).await.tap_err(error_handler) {
                Ok(ret) => ret,
                Err(_) => Self::create_empty().await.unwrap(),
            }
        }

        pub async fn wait(self) -> anyhow::Result<anyhow::Result<()>> {
            self.handler
                .await
                .map_err(|e| anyhow!("Unable wait handler: {:?}", e))
        }
    }

    impl From<JoinHandle<anyhow::Result<()>>> for DatabaseHelper {
        fn from(value: JoinHandle<anyhow::Result<()>>) -> Self {
            Self { handler: value }
        }
    }
}

use sqlx::SqliteConnection;

pub use handler::DatabaseHelper;
pub use types::Event;
pub use types::EventHelper as DatabaseEventHelper;
pub use v1 as current;
pub use v1::VERSION;
