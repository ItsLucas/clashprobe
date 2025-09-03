use futures::prelude::*;
use teloxide::prelude::Requester;
use teloxide::Bot;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::probe_result::ProbeResult;
use crate::reporter::ProbeReporter;
use anyhow::Result;
use async_trait::async_trait;

pub struct TeloxideReporter {
    bot: Bot,
    chat_id: i64,
    message_id: i64,
}

impl TeloxideReporter {
    pub fn new(config: &Config) -> Self {
        let bot = Bot::new(config.teloxide.token.clone());
        let chat_id = 0;
        let message_id = 0;

        Self { bot, chat_id, message_id }
    }
}

#[async_trait]
impl ProbeReporter for TeloxideReporter {
    async fn report(&self, results: &[ProbeResult]) -> Result<()> {
        let message = format!("Probe result : {:?}", results);
        // TODO: implement an appropriate way to update existing messages
        Ok(())
    }

    fn name(&self) -> &str {
        "TeloxideReporter"
    }
    
    fn is_continuous(&self) -> bool {
        true
    }
}