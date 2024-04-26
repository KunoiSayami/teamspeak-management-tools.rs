mod types {
    use crate::types::{NotifyClientEnterView, NotifyClientLeftView};
    use teloxide::adaptors::DefaultParseMode;
    use teloxide::payloads::SendMessage;
    use teloxide::prelude::{ChatId, Requester, RequesterExt};
    use teloxide::requests::JsonRequest;
    use teloxide::types::ParseMode;
    use teloxide::Bot;
    use tokio::sync::mpsc;
    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub(super) enum TelegramData {
        Enter(String, i64, String, String, String),
        Left(String, NotifyClientLeftView, String),
    }

    impl TelegramData {
        fn from_left(time: String, view: &NotifyClientLeftView, nickname: String) -> Self {
            Self::Left(time, view.clone(), nickname)
        }
        fn from_enter(time: String, view: &NotifyClientEnterView) -> Self {
            Self::Enter(
                time,
                view.client_id(),
                view.client_unique_identifier().to_string(),
                view.client_nickname().to_string(),
                view.client_country().to_string(),
            )
        }
    }

    impl std::fmt::Display for TelegramData {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TelegramData::Enter(time, client_id, client_identifier, nickname, country) => {
                    write!(
                        f,
                        "[{}] <b>{}</b>(<code>{}</code>:{})[{}] joined",
                        time,
                        nickname,
                        client_identifier,
                        client_id,
                        country_emoji::flag(country).unwrap_or_else(|| country.to_string())
                    )
                }
                TelegramData::Left(time, view, nickname) => match view.reason_id() {
                    8 => {
                        if view.reason().is_empty() {
                            write!(
                                f,
                                "[{}] <b>{}</b>({}) left",
                                time,
                                nickname,
                                view.client_id()
                            )
                        } else {
                            write!(
                                f,
                                "[{}] <b>{}</b>({}) left ({})",
                                time,
                                nickname,
                                view.client_id(),
                                view.reason()
                            )
                        }
                    }
                    3 => write!(
                        f,
                        "[{}] <b>{}</b>({}) connection lost #timeout",
                        time,
                        nickname,
                        view.client_id()
                    ),
                    5 | 6 => {
                        write!(f,
                               "[{time}] <b>{nickname}</b>({client_id}) was #{operation} by <b>{invoker}</b>(<code>{invoker_uid}</code>){reason}",
                               time = time,
                               nickname = nickname,
                               operation = if view.reason_id() == 5 { "kicked" } else { "banned" },
                               client_id = view.client_id(),
                               invoker = view.invoker_name(),
                               invoker_uid = view.invoker_uid(),
                               reason = if view.reason().is_empty() {
                                   " with no reason".to_string()
                               } else {
                                   format!(": {}", view.reason())
                               }
                        )
                    }
                    _ => unreachable!("Got unexpected left message: {:?}", view),
                },
            }
        }
    }

    #[derive(Clone, Debug)]
    pub(super) enum CombineData {
        Send(String, TelegramData),
        //Terminate,
    }

    impl CombineData {
        pub fn new(config_id: String, data: TelegramData) -> Self {
            Self::Send(config_id, data)
        }

        /*pub fn terminate() -> Self {
            Self::Terminate
        }*/
    }

    #[derive(Clone, Debug)]
    pub struct TelegramHelper {
        sender: mpsc::Sender<CombineData>,
    }

    impl TelegramHelper {
        pub async fn send_left(
            &self,
            id: String,
            time: String,
            view: &NotifyClientLeftView,
            nickname: String,
        ) -> Option<()> {
            self.sender
                .send(CombineData::new(
                    id,
                    TelegramData::from_left(time, view, nickname),
                ))
                .await
                .map(|_| ())
                .ok()
        }

        pub async fn send_enter(
            &self,
            id: String,
            time: String,
            view: &NotifyClientEnterView,
        ) -> Option<()> {
            self.sender
                .send(CombineData::new(id, TelegramData::from_enter(time, view)))
                .await
                .map(|_| ())
                .ok()
        }

        /*pub async fn send_terminate(&self) -> Option<()> {
            self.sender.send(CombineData::terminate())
        }*/

        pub(super) fn new() -> (Self, mpsc::Receiver<CombineData>) {
            let (sender, r) = mpsc::channel(4096);
            (Self { sender }, r)
        }

        pub fn into_bind(self, config_id: String) -> BindTelegramHelper {
            BindTelegramHelper::new(config_id, self)
        }
    }

    #[derive(Clone, Debug)]
    pub struct BindTelegramHelper {
        inner: TelegramHelper,
        config_id: String,
    }

    impl BindTelegramHelper {
        pub async fn send_left(
            &self,
            time: String,
            view: &NotifyClientLeftView,
            nickname: String,
        ) -> Option<()> {
            self.inner
                .send_left(self.config_id.clone(), time, view, nickname)
                .await
        }
        pub async fn send_enter(&self, time: String, view: &NotifyClientEnterView) -> Option<()> {
            self.inner
                .send_enter(self.config_id.clone(), time, view)
                .await
        }

        fn new(config_id: String, helper: TelegramHelper) -> Self {
            Self {
                config_id,
                inner: helper,
            }
        }
    }

    #[derive(Clone, Debug)]
    pub(super) struct TelegramBot {
        bot: DefaultParseMode<Bot>,
        channel_id: i64,
    }

    impl TelegramBot {
        pub fn new(token: &str, api_key: String, channel_id: i64) -> anyhow::Result<Self> {
            Ok(Self {
                bot: Bot::new(token)
                    .set_api_url(api_key.parse()?)
                    .parse_mode(ParseMode::Html),
                channel_id,
            })
        }

        pub fn send(&self, message: String) -> JsonRequest<SendMessage> {
            self.bot.send_message(ChatId(self.channel_id), message)
        }
    }
}

mod thread {
    use super::types::{CombineData, TelegramBot, TelegramHelper};
    use crate::configure::Config;
    use crate::types::MessageQueue;
    use anyhow::anyhow;
    use log::{debug, error, info, warn};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use teloxide::prelude::Request;
    use tokio::sync::{mpsc, Notify};

    const QUERY_BOT_ERROR: &str = "Query bot error";
    const QUERY_CONFIG_ERROR: &str = "Query config id error";

    pub fn telegram_bootstrap(
        configs: &Vec<(String, Config)>,
        notifier: Arc<Notify>,
    ) -> anyhow::Result<(tokio::task::JoinHandle<anyhow::Result<()>>, TelegramHelper)> {
        // A hashmap container configure and bot relationship
        let mut config_map = HashMap::new();
        // A hashmap container bot instance
        let mut bot_map = HashMap::new();
        // A hashmap container bot id with messages relationship (Queue is configure id with unsent message)
        let mut pool_map: HashMap<String, HashMap<String, MessageQueue<String>>> = HashMap::new();
        for (_, config) in configs {
            let config_id = config.get_id();

            // Check is config available in Telegram
            if config.telegram().api_key().is_empty() {
                info!("Configure: [{}] token is empty, skipped all send message request. Send to telegram disabled.", &config_id);
                continue;
            }

            // Get bot self ID
            let bot_id = match config.telegram().api_key().split_once(':') {
                None => {
                    warn!(
                        "Configure: [{}] token in invalid format, ignore.",
                        &config_id
                    );
                    continue;
                }
                Some((id, _)) => id.to_string(),
            };

            // If bot id is correct, insert into configure map
            config_map.insert(config_id.clone(), bot_id.clone());

            // Check is bot has been created (maybe used by another configure)
            if bot_map.get(&bot_id).is_none() {
                bot_map.insert(
                    bot_id.clone(),
                    TelegramBot::new(
                        config.telegram().api_key(),
                        config.telegram().api_server(),
                        config.telegram().target(),
                    )
                    .map_err(|e| anyhow!("Parse error: {:?}", e))?,
                );
            }

            // Initialize message pool
            match pool_map.get_mut(&bot_id) {
                Some(element) => {
                    element.insert(config_id.clone(), MessageQueue::new());
                }
                None => {
                    let mut m = HashMap::new();
                    m.insert(config_id, MessageQueue::<String>::new());
                    pool_map.insert(bot_id.clone(), HashMap::new());
                }
            }
        }

        let (sender, receiver) = TelegramHelper::new();
        let handler = tokio::spawn(telegram_thread(
            receiver, bot_map, config_map, pool_map, notifier,
        ));
        Ok((handler, sender))
    }

    async fn telegram_thread(
        mut receiver: mpsc::Receiver<CombineData>,
        bot_map: HashMap<String, TelegramBot>,
        config_map: HashMap<String, String>,
        mut pool_map: HashMap<String, HashMap<String, MessageQueue<String>>>,
        notifier: Arc<Notify>,
    ) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                cmd = receiver.recv() => {
                    if let Some(cmd) = cmd {
                        match cmd {
                            CombineData::Send(config_id, data) => {
                                if let Some(bot_id) = config_map.get(&config_id) {
                                    let map = pool_map
                                        .get_mut(bot_id)
                                        .expect(QUERY_BOT_ERROR)
                                        .get_mut(&config_id)
                                        .expect(QUERY_CONFIG_ERROR);
                                    map.push(data.to_string());
                                }
                            }
                        }
                    } else {
                        break;
                    }
                }

                // Tick by timer
                _ = interval.tick() => {}
                _ = notifier.notified() => {}
            }
        }

        for (bot_id, pool) in &mut pool_map {
            if pool.is_empty() {
                continue;
            }
            for (config_id, origin_queue) in pool {
                if origin_queue.is_empty() {
                    continue;
                }
                let bot = bot_map.get(bot_id).unwrap();
                let mut sent = 0;
                for message in origin_queue.chunks(5) {
                    if let Err(e) = bot
                        .send(format!("[{}]\n{}", config_id, message.join("\n")))
                        .send()
                        .await
                    {
                        error!("Got error in send telegram message {:?}", e);
                        break;
                    }
                    sent += message.len();
                }
                if sent >= origin_queue.len() {
                    origin_queue.clear()
                } else {
                    origin_queue.drain(..sent);
                }
            }
        }
        debug!("Send message daemon exiting...");
        Ok(())
    }
}

pub use thread::telegram_bootstrap;
pub use types::{BindTelegramHelper, TelegramHelper};
