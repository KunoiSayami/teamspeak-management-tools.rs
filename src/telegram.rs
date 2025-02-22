mod types {
    use crate::types::{NotifyClientEnterView, NotifyClientLeftView};
    use teloxide::Bot;
    use teloxide::adaptors::DefaultParseMode;
    use teloxide::prelude::{ChatId, Request, Requester, RequesterExt};
    use teloxide::types::ParseMode;
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
                        "[{time}] <b>{nickname}</b>(<code>{client_identifier}</code>:{client_id})[{}] joined",
                        country_emoji::flag(country).unwrap_or_else(|| country.into())
                    )
                }
                TelegramData::Left(time, view, nickname) => match view.reason_id() {
                    8 => {
                        if view.reason().is_empty() {
                            write!(f, "[{time}] <b>{nickname}</b>({}) left", view.client_id())
                        } else {
                            write!(
                                f,
                                "[{time}] <b>{nickname}</b>({}) left ({})",
                                view.client_id(),
                                view.reason()
                            )
                        }
                    }
                    3 => write!(
                        f,
                        "[{time}] <b>{nickname}</b>({}) connection lost #timeout",
                        view.client_id()
                    ),
                    5 | 6 => {
                        write!(
                            f,
                            "[{time}] <b>{nickname}</b>({client_id}) was #{operation} by <b>{invoker}</b>(<code>{invoker_uid}</code>){reason}",
                            time = time,
                            nickname = nickname,
                            operation = if view.reason_id() == 5 {
                                "kicked"
                            } else {
                                "banned"
                            },
                            client_id = view.client_id(),
                            invoker = view.invoker_name(),
                            invoker_uid = view.invoker_uid(),
                            reason = if view.reason().is_empty() {
                                " with no reason".into()
                            } else {
                                format!(": {}", view.reason())
                            }
                        )
                    }
                    _ => unreachable!("Got unexpected left message: {view:?}"),
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

    pub type BotType = DefaultParseMode<Bot>;

    #[derive(Clone, Debug)]
    pub(super) struct TelegramBot {
        bot: BotType,
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

        pub fn send(
            &self,
            message: String,
        ) -> impl std::future::Future<Output = Result<teloxide::prelude::Message, teloxide::RequestError>>
        {
            self.bot
                .send_message(ChatId(self.channel_id), message)
                .send()
        }

        pub fn into_inner(self) -> BotType {
            self.bot
        }

        pub fn valid(&self) -> bool {
            self.channel_id != 0
        }
    }

    pub(super) struct PoolIter<'a> {
        name: &'a str,
        iter: std::slice::Iter<'a, String>,
    }

    impl<'a> From<(&'a str, std::slice::Iter<'a, String>)> for PoolIter<'a> {
        fn from(value: (&'a str, std::slice::Iter<'a, String>)) -> Self {
            Self {
                name: value.0,
                iter: value.1,
            }
        }
    }
    impl<'a> Iterator for PoolIter<'a> {
        type Item = (&'a str, &'a String);

        fn next(&mut self) -> Option<Self::Item> {
            self.iter.next().map(|s| (self.name, s))
        }
    }
}

mod thread {
    use super::types::{CombineData, TelegramBot, TelegramData, TelegramHelper};
    use crate::{
        configure::Config,
        types::{ConfigMappedUserState, SafeUserState},
    };
    use anyhow::anyhow;
    use bot_impl::ResponderPool;
    use log::{debug, error, info, warn};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Notify, broadcast, mpsc};

    const QUERY_BOT_ERROR: &str = "Query bot error";

    pub fn telegram_bootstrap(
        configs: &Vec<(String, Config)>,
        notifier: Arc<Notify>,
    ) -> anyhow::Result<(
        tokio::task::JoinHandle<anyhow::Result<()>>,
        TelegramHelper,
        ConfigMappedUserState,
    )> {
        // A hashmap container configure and bot relationship
        let mut config_map = HashMap::new();
        // A hashmap container bot instance
        let mut bot_map = HashMap::new();

        // A hashmap container channel-client relationship
        let mut user_state_map = ConfigMappedUserState::new();

        let mut bot_responder = HashMap::new();
        // A hashmap container bot id with messages relationship (Queue is configure id with unsent message)
        //let mut pool_map: HashMap<String, HashMap<String, MessageQueue<String>>> = HashMap::new();
        for (_, config) in configs {
            let config_id = config.get_id();

            // Check is config available in Telegram
            if config.telegram().api_key().is_empty() {
                info!("Configure: [{config_id}] token is empty, skipped all send message request.",);
                user_state_map.insert(config_id, SafeUserState::create_none());
                continue;
            }

            // Get bot self ID
            let Some((bot_id, _)) = config.telegram().api_key().split_once(':') else {
                warn!("Configure: [{config_id}] token in invalid format, ignore.",);
                continue;
            };

            if config.telegram().responsible() {
                user_state_map.insert(config_id.clone(), SafeUserState::create());
                // If responsible, insert to bot responder
                bot_responder
                    .entry(bot_id.to_string())
                    .or_insert_with(Vec::new)
                    .push((config_id.clone(), config.telegram().allowed_chat().to_vec()));
            } else {
                user_state_map.insert(config_id.clone(), SafeUserState::create_none());
            }

            // If bot id is correct, insert into configure map
            config_map.insert(config_id.clone(), bot_id.to_string());

            // Check is bot has been created (maybe used by another configure)
            if !bot_map.contains_key(bot_id) {
                bot_map.insert(
                    bot_id.into(),
                    (
                        TelegramBot::new(
                            config.telegram().api_key(),
                            config.telegram().api_server(),
                            config.telegram().target(),
                        )
                        .map_err(|e| anyhow!("Parse error: {e:?}"))?,
                        vec![],
                    ),
                );
            }
        }

        let (sender, receiver) = TelegramHelper::new();

        let handler = if config_map.is_empty() {
            tokio::spawn(void_thread(receiver, notifier))
        } else {
            tokio::spawn(telegram_thread(
                receiver,
                bot_map,
                config_map,
                notifier,
                (user_state_map.clone(), bot_responder),
            ))
        };
        Ok((handler, sender, user_state_map))
    }

    async fn telegram_thread(
        mut receiver: mpsc::Receiver<CombineData>,
        mut bot_map: HashMap<String, (TelegramBot, Vec<(String, TelegramData)>)>,
        config_map: HashMap<String, String>,
        notifier: Arc<Notify>,
        (user_state, bot_responder): (
            ConfigMappedUserState,
            HashMap<String, Vec<(String, Vec<i64>)>>,
        ),
    ) -> anyhow::Result<()> {
        if bot_map.is_empty() {
            info!("No configure found, Send to telegram disabled.");
            return Ok(());
        }
        //let mut queue = Vec::new();
        let mut pending = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        let (exit_sender, exit_signal) = broadcast::channel(5);
        let response_pool =
            ResponderPool::spawn(user_state, bot_responder, bot_map.clone(), exit_signal).await;
        loop {
            tokio::select! {
                cmd = receiver.recv() => {
                    let Some(CombineData::Send(config_id, data)) = cmd else {
                        break;
                    };
                    if let Some(bot_id) = config_map.get(&config_id) {
                        bot_map
                            .get_mut(bot_id)
                            .expect(QUERY_BOT_ERROR)
                            .1
                            .push((config_id, data));
                    }
                }

                // Tick by timer
                _ = interval.tick() => {
                    for (bot_id, (bot, queue)) in &mut bot_map {
                        if queue.is_empty() {
                            continue;
                        }

                        if !bot.valid() {
                            queue.clear();
                            continue;
                        }

                        let mut sent = 0;

                        for chunk in queue.chunks(8) {
                            let mut prev = &String::new();
                            for (config_id, data) in chunk {
                                if !config_id.eq(prev) {
                                    pending.push(config_id.clone());
                                    prev = config_id;
                                }
                                pending.push(data.to_string());
                            }
                            let message = pending.join("\n");
                            pending.clear();

                            if let Err(e) = bot.send(message).await {
                                error!("Got error in {bot_id} send telegram message {e:?}");
                                break;
                            }
                            sent += chunk.len();
                        }
                        if sent >= queue.len() {
                            queue.clear()
                        } else {
                            queue.drain(..sent);
                        }
                    }
                }
                _ = notifier.notified() => {
                    break
                }
            }
        }
        exit_sender.send(true).ok();

        match tokio::time::timeout(Duration::from_secs(3), response_pool.wait()).await {
            Ok(ret) => ret?,
            Err(_) => warn!("Responder exit timeout"),
        };

        debug!("Send message daemon exiting...");
        Ok(())
    }

    async fn void_thread(
        mut receiver: mpsc::Receiver<CombineData>,
        notifier: Arc<Notify>,
    ) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                cmd = receiver.recv() => {
                    if cmd.is_none() {
                        break
                    }
                }
                _ = notifier.notified() => {
                    break
                }
            }
        }
        Ok(())
    }

    /* async fn start_response_handler(
           user_state: ConfigMappedUserState,
           bot_responder: HashMap<String, Vec<(String, Vec<i64>)>>,
           exit_signal: broadcast::Receiver<bool>,
       ) -> anyhow::Result<()> {
           for key in bot_responder.keys() {}

           Ok(())
       }
    */
    mod bot_impl {
        use std::{collections::HashMap, sync::Arc};

        use log::warn;
        use teloxide::{
            dispatching::{HandlerExt as _, UpdateFilterExt as _},
            dptree,
            prelude::{Dispatcher, Requester as _},
            types::{Message, Update},
            utils::command::BotCommands,
        };
        use tokio::{sync::broadcast, task::JoinHandle};

        use crate::{telegram::types::BotType, types::ConfigMappedUserState};

        use super::{TelegramBot, TelegramData};

        #[derive(BotCommands, Clone)]
        #[command(rename_rule = "lowercase")]
        enum Command {
            Ping,
            List,
        }

        pub struct ResponderPool {
            handles: Vec<Responder>,
        }

        impl ResponderPool {
            pub async fn wait(self) -> anyhow::Result<()> {
                for handle in self.handles {
                    handle.join().await?;
                }
                Ok(())
            }

            pub async fn spawn(
                channel_map: ConfigMappedUserState,
                bots: HashMap<String, Vec<(String, Vec<i64>)>>,
                bot_map: HashMap<String, (TelegramBot, Vec<(String, TelegramData)>)>,
                exit_signal: broadcast::Receiver<bool>,
            ) -> Self {
                let mut v = vec![];
                for (bot_id, config_with_chat) in bots {
                    v.push(Responder::spawn(
                        bot_map.get(&bot_id).unwrap().0.clone(),
                        config_with_chat,
                        channel_map.clone(),
                        exit_signal.resubscribe(),
                    ));
                }
                Self { handles: v }
            }
        }

        pub struct Responder {
            handle: JoinHandle<anyhow::Result<()>>,
        }

        impl Responder {
            pub fn spawn(
                bot: TelegramBot,
                config_with_chat: Vec<(String, Vec<i64>)>,
                channel_map: ConfigMappedUserState,
                exit_signal: broadcast::Receiver<bool>,
            ) -> Self {
                let handle =
                    tokio::spawn(Self::run(bot, config_with_chat, channel_map, exit_signal));
                Self { handle }
            }

            fn build_chat_map(
                config_with_chat: Vec<(String, Vec<i64>)>,
            ) -> HashMap<i64, Vec<String>> {
                let mut m = HashMap::new();
                for (config_id, chats) in config_with_chat {
                    for chat in chats {
                        m.entry(chat)
                            .or_insert_with(Vec::new)
                            .push(config_id.clone());
                    }
                }
                m
            }

            async fn run(
                bot: TelegramBot,
                config_with_chat: Vec<(String, Vec<i64>)>,
                channel_map: ConfigMappedUserState,
                mut exit_signal: broadcast::Receiver<bool>,
            ) -> anyhow::Result<()> {
                let bot = bot.into_inner();
                let chat_map = Arc::new(Self::build_chat_map(config_with_chat));
                let handle_message = Update::filter_message().branch(
                    dptree::entry()
                        .filter(|msg: Message| {
                            //log::debug!("{:?}", msg.chat);
                            !msg.chat.is_channel()
                        })
                        .filter_command::<Command>()
                        .endpoint(
                            |msg: Message,
                             bot: BotType,
                             cmd: Command,
                             chat_map: Arc<HashMap<i64, Vec<String>>>,
                             channel_map: ConfigMappedUserState| async move {
                                match cmd {
                                    Command::Ping => handle_ping(bot, msg).await,
                                    Command::List => {
                                        handle_list(bot, msg, chat_map, channel_map).await
                                    }
                                }
                                .inspect_err(|e| log::error!("Handle command error: {e:?}"))
                            },
                        ),
                );

                let dispatcher = Dispatcher::builder(bot, dptree::entry().branch(handle_message))
                    .dependencies(dptree::deps![chat_map, channel_map])
                    /* .default_handler(|update| async move {
                        log::debug!("Unhandled message {:?}", update.from());
                    }) */
                    .default_handler(|_| async {});

                //#[cfg(not(debug_assertions))]
                //dispatcher.enable_ctrlc_handler().build().dispatch().await;

                //#[cfg(debug_assertions)]
                tokio::select! {
                    _ = async move {
                        dispatcher.build().dispatch().await
                    } => {}
                    _ = exit_signal.recv() => {}
                }
                Ok(())
            }
            pub async fn join(self) -> anyhow::Result<()> {
                self.handle.await?
            }
        }

        pub async fn handle_ping(bot: BotType, msg: Message) -> anyhow::Result<()> {
            bot.send_message(
                msg.chat.id,
                format!(
                    "Chat id: <code>{id}</code>\nVersion: {version}",
                    id = msg.chat.id.0,
                    version = env!("CARGO_PKG_VERSION")
                ),
            )
            .await?;
            Ok(())
        }

        pub async fn handle_list(
            bot: BotType,
            msg: Message,
            chat_map: Arc<HashMap<i64, Vec<String>>>,
            channel_map: ConfigMappedUserState,
        ) -> anyhow::Result<()> {
            let Some(configs) = chat_map.get(&msg.chat.id.0) else {
                warn!("Deny unauthorized access chat {}", msg.chat.id);
                return Ok(());
            };
            let mut v = vec![];
            for config in configs {
                let Some(map) = channel_map.get(config).filter(|s| s.enabled()) else {
                    //warn!("State is empty, skip");
                    continue;
                };
                v.push(format!("{config}\n{}", map.read().await.unwrap()));
            }

            bot.send_message(msg.chat.id, v.join("\n\n")).await?;

            Ok(())
        }
    }
}

/* #[cfg(test)]
mod test {
    use std::collections::HashMap;

    #[test]
    fn test_iterator() {
        let mut m = HashMap::new();
        m.insert("11", vec![1, 9, 1, 9, 8, 1, 0]);
        m.insert("45", vec![]);
        m.insert("14", vec![1, 1, 4, 5, 1, 4]);

        let iter = m
            .iter()
            .map(|(k, v)| std::iter::zip(std::iter::repeat(k), v))
            .flatten()
            .collect::<Vec<_>>();

        assert_eq!(iter.len(), 13);
    }
} */

pub use thread::telegram_bootstrap;
pub use types::{BindTelegramHelper, TelegramHelper};
