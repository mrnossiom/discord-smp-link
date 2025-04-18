//! Handles all the states of the bot and initial configuration

use crate::{
	auth::GoogleAuthentification, database::DatabasePool, polyfill, translation::Translations,
};
use anyhow::{anyhow, Context as _};
use diesel_async::{
	pooled_connection::{
		deadpool::{Pool, PoolError},
		AsyncDieselConnectionManager,
	},
	AsyncMysqlConnection,
};
use dotenvy::dotenv;
use oauth2::{ClientId, ClientSecret};
use poise::{
	async_trait, send_application_reply,
	serenity_prelude::{self as serenity, GuildId},
	CreateReply, ReplyHandle,
};
use secrecy::{ExposeSecret, SecretString};
use std::{
	env::{self, VarError},
	fmt,
	sync::Arc,
};
use unic_langid::LanguageIdentifier;

/// App global configuration
#[derive(Debug)]
pub(crate) struct Config {
	/// The token needed to access the `Discord` Api
	pub(crate) discord_token: SecretString,
	/// The guild on witch you can access development commands
	pub(crate) discord_development_guild: GuildId,
	/// The `Postgres` connection uri
	pub(crate) database_url: SecretString,
	/// The `Google` auth client id and secret pair
	pub(crate) google_client: (ClientId, ClientSecret),
	/// The `Discord` invite link to rejoin the support server
	pub(crate) discord_invite_code: String,
	/// The url of the `OAuth2` callback
	///
	/// Example: `dev-smp-link.some.domain`
	pub(crate) server_url: String,

	/// The default locale to use
	pub(crate) default_locale: LanguageIdentifier,
	/// Whether or not to use production defaults
	///
	/// Currently only affects logging
	pub(crate) production: bool,
}

/// Resolve an environment variable or return an appropriate error
fn required_env_var(name: &str) -> anyhow::Result<String> {
	match env::var(name) {
		Ok(val) => Ok(val),
		Err(VarError::NotPresent) => Err(anyhow!("{} must be set in the environnement", name)),
		Err(VarError::NotUnicode(_)) => {
			Err(anyhow!("{} does not contains Unicode valid text", name))
		}
	}
}

// IDEA: use the `figment` crate to parse config
impl Config {
	/// Parse the config from `.env` file
	fn from_dotenv() -> anyhow::Result<Self> {
		// Load the `.env` file ond error if not found
		dotenv()?;

		let discord_invite_code = required_env_var("DISCORD_INVITE_CODE")?;

		let discord_development_guild = required_env_var("DISCORD_DEV_GUILD")?
			.parse::<u64>()
			.map_err(|_| anyhow!("DISCORD_DEV_GUILD environnement variable must be a `u64`"))?;

		let production = env::var("PRODUCTION")
			.unwrap_or_else(|_| "false".into())
			.parse::<bool>()
			.map_err(|_| anyhow!("PRODUCTION environnement variable must be a `bool`"))?;

		let default_locale = required_env_var("DEFAULT_LOCALE")?
			.parse::<LanguageIdentifier>()
			.map_err(|_| {
				anyhow!("DEFAULT_LOCALE environnement variable must be a `LanguageIdentifier`")
			})?;

		Ok(Self {
			discord_token: SecretString::from(required_env_var("DISCORD_TOKEN")?),
			discord_development_guild: GuildId::new(discord_development_guild),
			database_url: SecretString::from(required_env_var("DATABASE_URL")?),
			google_client: (
				ClientId::new(required_env_var("GOOGLE_CLIENT_ID")?),
				ClientSecret::new(required_env_var("GOOGLE_CLIENT_SECRET")?),
			),
			discord_invite_code,
			server_url: required_env_var("SERVER_URL")?,

			default_locale,
			production,
		})
	}
}

/// App global data
pub(crate) struct Data {
	/// An access to the database
	pub(crate) database: DatabasePool,
	/// A instance of the auth provider
	pub(crate) auth: GoogleAuthentification,
	/// An instance of the parsed initial config
	pub(crate) config: Config,
	/// The translations for the client
	pub(crate) translations: Translations,
}

impl fmt::Debug for Data {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Data")
			.field("auth", &&self.auth)
			.field("config", &&self.config)
			.field("translations", &&self.translations)
			.finish_non_exhaustive()
	}
}

impl Data {
	/// Parse the bot data from
	pub(crate) fn new() -> anyhow::Result<Self> {
		let config = Config::from_dotenv()?;

		let manager = AsyncDieselConnectionManager::<AsyncMysqlConnection>::new(
			config.database_url.expose_secret(),
		);
		let database = Pool::builder(manager)
			.build()
			.context("failed to create database pool")?;

		let translations = Translations::from_folder("translations", config.default_locale.clone())
			.context("failed to load translations")?;

		Ok(Self {
			database,
			auth: GoogleAuthentification::new(&config)?,
			config,
			translations,
		})
	}
}

/// Trait for sending ephemeral messages
#[async_trait]
pub(crate) trait ApplicationContextPolyfill<'a>: Send + Sync {
	/// Send a message to the user
	#[allow(dead_code)]
	async fn send(self, reply: CreateReply) -> Result<ReplyHandle<'a>, serenity::Error>;

	/// Send an ephemeral message to the user
	async fn shout(
		&self,
		content: impl Into<String> + Send,
	) -> Result<ReplyHandle<'_>, serenity::Error>;

	/// Get a [`GuildId`] in a `guild_only` interaction context
	///
	/// # Panics
	/// If used in a non `guild_only` interaction context
	fn guild_only_id(&self) -> GuildId;
}

#[async_trait]
impl<'a> ApplicationContextPolyfill<'a> for ApplicationContext<'a> {
	#[inline]
	async fn send(self, builder: CreateReply) -> Result<ReplyHandle<'a>, serenity::Error> {
		send_application_reply(self, builder).await
	}

	#[inline]
	async fn shout(
		&self,
		content: impl Into<String> + Send,
	) -> Result<ReplyHandle<'_>, serenity::Error> {
		self.send(CreateReply::default().content(content).ephemeral(true))
			.await
	}

	#[inline]
	fn guild_only_id(&self) -> GuildId {
		if self.command.guild_only {
			self.interaction.guild_id.expect("guild_only interactions")
		} else {
			panic!("Should be used only in guild_only interactions")
		}
	}
}

/// Trait for sending ephemeral messages
#[async_trait]
pub(crate) trait ContextPolyfill: Send + Sync {
	/// Send an ephemeral message to the user
	async fn shout(
		&self,
		content: impl Into<String> + Send,
	) -> Result<ReplyHandle<'_>, serenity::Error>;
}

#[async_trait]
impl ContextPolyfill for Context<'_> {
	#[inline]
	async fn shout(
		&self,
		content: impl Into<String> + Send,
	) -> Result<ReplyHandle<'_>, serenity::Error> {
		self.send(CreateReply::default().content(content).ephemeral(true))
			.await
	}
}

/// Common wrapper for the [`Data`]
pub(crate) type ArcData = Arc<Data>;
/// Common interaction or event error type
pub(crate) type InteractionError = Error;
/// Common interaction or event return type
pub(crate) type InteractionResult = Result<(), InteractionError>;

/// A [`poise::Command`] type alias with our common types
pub(crate) type Command = poise::Command<ArcData, InteractionError>;
/// A [`poise::Context`] type alias with our common types, provided to each command
pub(crate) type Context<'a> = poise::Context<'a, ArcData, InteractionError>;
/// A [`poise::ApplicationContext`] type alias with our common types, provided to each command, provided to each slash command
pub(crate) type ApplicationContext<'a> = poise::ApplicationContext<'a, ArcData, InteractionError>;
/// A [`polyfill::MessageComponentContext`] type alias with our common types, provided to each message component interaction
pub(crate) type MessageComponentContext<'a> =
	polyfill::MessageComponentContext<'a, ArcData, InteractionError>;

/// A [`poise::Framework`] type alias with our common types
pub(crate) type Framework = poise::Framework<ArcData, InteractionError>;
/// A [`poise::FrameworkContext`] type alias with our common types
pub(crate) type FrameworkContext<'a> = poise::FrameworkContext<'a, ArcData, InteractionError>;
/// A [`poise::FrameworkError`] type alias with our common types
pub(crate) type FrameworkError<'a> = poise::FrameworkError<'a, ArcData, InteractionError>;

/// An error in an interaction or an event
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
	/// A serenity error
	#[error(transparent)]
	Serenity(#[from] serenity::Error),
	/// A database error
	#[error(transparent)]
	Pool(#[from] PoolError),
	/// A diesel error
	#[error(transparent)]
	Diesel(#[from] diesel::result::Error),
	/// Collects any other general purpose error
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}
