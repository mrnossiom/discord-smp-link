#![warn(
	clippy::unwrap_used,
	clippy::str_to_string,
	clippy::suspicious_operation_groupings,
	clippy::todo,
	clippy::too_many_lines,
	clippy::unicode_not_nfc,
	clippy::unused_async,
	clippy::use_self,
	clippy::dbg_macro,
	clippy::doc_markdown,
	clippy::else_if_without_else,
	clippy::future_not_send,
	clippy::implicit_clone,
	clippy::match_bool,
	clippy::missing_panics_doc,
	clippy::redundant_closure_for_method_calls,
	clippy::redundant_else,
	clippy::must_use_candidate,
	clippy::return_self_not_must_use,
	clippy::missing_docs_in_private_items,
	rustdoc::broken_intra_doc_links
)]

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
	states::{Data, Framework, FrameworkBuilder},
};
use anyhow::{anyhow, Context};
use poise::serenity_prelude::GatewayIntents;
use secrecy::ExposeSecret;
use states::ArcData;
use std::sync::Arc;
use tracing::instrument;

/// Build the `poise` [framework](poise::Framework)
#[instrument]
fn build_client(data: ArcData) -> FrameworkBuilder {
	Framework::builder()
		.token(data.config.discord_token.expose_secret())
		.intents(
			GatewayIntents::GUILDS
				| GatewayIntents::GUILD_VOICE_STATES
				| GatewayIntents::DIRECT_MESSAGES
				| GatewayIntents::GUILD_MESSAGES
				| GatewayIntents::GUILD_MEMBERS,
		)
		.user_data_setup({
			let data = Arc::clone(&data);
			move |_ctx, _ready, _framework| Box::pin(async move { Ok(data) })
		})
		.options(poise::FrameworkOptions {
			pre_command,
			on_error: command_on_error,
			post_command,
			listener: |ctx, event, fw, data| {
				Box::pin(async move { event_handler(ctx, event, fw, data).await })
			},
			commands: {
				use commands::*;

				#[rustfmt::skip]
				let mut commands = vec![
					setup(),
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let data = Arc::new(Data::new()?);

	setup_logging(Arc::clone(&data))?;
	let _handle = start_server(Arc::clone(&data))?;

	run_migrations(data.config.database_url.expose_secret()).context("failed to run migrations")?;

	if let Err(error) = build_client(Arc::clone(&data)).run().await {
		return Err(anyhow!("Client exited with error: {}", error));
	}

	Ok(())
}
