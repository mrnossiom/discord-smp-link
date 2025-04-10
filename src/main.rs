//! Discord SMP Bot

mod auth;
mod commands;
mod constants;
mod database;
mod events;
mod logging;
mod polyfill;
mod server;
mod states;
mod translation;

use crate::{
	commands::{command_on_error, post_command, pre_command},
	database::run_migrations,
	events::event_handler,
	logging::setup_logging,
	server::start_server,
	states::{ArcData, Data, Framework},
};
use anyhow::{anyhow, Context};
use poise::serenity_prelude::{ClientBuilder, GatewayIntents};
use secrecy::ExposeSecret;
use std::sync::Arc;
use tracing::instrument;

/// Build the `poise` [framework](poise::Framework)
#[instrument]
fn build_framework(data: ArcData) -> Framework {
	Framework::builder()
		.setup({
			let data = Arc::clone(&data);
			move |_ctx, _ready, _framework| Box::pin(async move { Ok(data) })
		})
		.options(poise::FrameworkOptions {
			pre_command,
			on_error: command_on_error,
			post_command,
			event_handler: |ctx, event, fw, data| Box::pin(event_handler(ctx, event, fw, data)),
			commands: {
				use commands::{classes, groups, helpers, information, levels, setup};

				#[rustfmt::skip]
				let mut commands = vec![
					setup(),
					levels(),
					classes(),
					groups(),
					information(),
					helpers::debug(),
				];

				data.translations
					.apply_translations_to_interactions(&mut commands, None);

				commands
			},
			..Default::default()
		})
		.initialize_owners(true)
		.build()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let data = Arc::new(Data::new()?);

	setup_logging(&data)?;
	let _handle = start_server(Arc::clone(&data))?;

	run_migrations(data.config.database_url.expose_secret()).context("failed to run migrations")?;

	let mut client = ClientBuilder::new(
		data.config.discord_token.expose_secret(),
		GatewayIntents::GUILDS
			| GatewayIntents::GUILD_VOICE_STATES
			| GatewayIntents::DIRECT_MESSAGES
			| GatewayIntents::GUILD_MESSAGES
			| GatewayIntents::GUILD_MEMBERS,
	)
	.framework(build_framework(Arc::clone(&data)))
	.await?;

	if let Err(error) = client.start().await {
		return Err(anyhow!("Client exited with error: {}", error));
	}

	Ok(())
}
