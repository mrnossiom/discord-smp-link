//! `Discord` client commands

use crate::states::{Context, FrameworkError, Shout};
use anyhow::anyhow;
use console::style;
use poise::BoxFuture;
use uuid::Uuid;

mod setup;

pub use setup::setup;
pub mod helpers;
pub mod login;

/// Execute before each command
pub fn pre_command(ctx: Context) -> BoxFuture<()> {
	Box::pin(async move {
		log::debug!(
			"{} invoked by {}",
			ctx.invoked_command_name(),
			ctx.author().name,
		);
	})
}

/// Execute on a error during code execution
pub fn command_on_error(error: FrameworkError) -> BoxFuture<()> {
	Box::pin(async move {
		if let Err(error) = match error {
			FrameworkError::Command { error, ctx, .. } => {
				let error_identifier = Uuid::new_v4();

				log::error!(
					target: error_identifier.to_string().as_str(),
					"{} invoked `{}` but an error occurred: {}",
					ctx.author().name,
					ctx.invoked_command_name(),
					error,
				);

				let error_msg = format!(
					"An internal error occurred with the command execution. \
					If this error persist please contact the dev with the following code: `{}`",
					error_identifier.as_braced()
				);

				ctx.shout(error_msg)
					.await
					.map(|_| ())
					.map_err(|_| anyhow!("Failed to send internal error message"))
			}

			FrameworkError::ArgumentParse { error, .. } => {
				log::error!(target: "Argument Parse", "{}", error);

				Ok(())
			}

			FrameworkError::CommandStructureMismatch { description, .. } => {
				log::error!(target: "Command Structure Mismatch", "You should sync your commands : {} ", description);

				Ok(())
			}

			FrameworkError::CooldownHit {
				remaining_cooldown,
				ctx,
			} => ctx
				.shout(format!(
					"You can use this command again in {} seconds",
					remaining_cooldown.as_secs()
				))
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send cooldown hit message")),

			FrameworkError::MissingBotPermissions {
				ctx,
				missing_permissions,
			} => ctx
				.shout(format!(
					"The bot is missing the following permissions : {}",
					missing_permissions
				))
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send missing bot permissions message")),

			FrameworkError::MissingUserPermissions {
				ctx,
				missing_permissions,
			} => ctx
				.shout(format!(
					"You are missing the following permissions : {}",
					missing_permissions.unwrap_or_default()
				))
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send missing user permissions message")),

			FrameworkError::NotAnOwner { ctx } => ctx
				.shout("You are not the owner of this bot.".into())
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send not an owner message")),

			FrameworkError::GuildOnly { ctx } => ctx
				.shout("This command can only be used in a guild channel".into())
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send guild only message")),

			FrameworkError::DmOnly { ctx } => ctx
				.shout("This command can only be used in a DM channel".into())
				.await
				.map(|_| ())
				.map_err(|_| anyhow!("Failed to send dm only message")),

			FrameworkError::CommandCheckFailed { ctx, error } => {
				let error_identifier = Uuid::new_v4();

				log::error!(
					target: error_identifier.to_string().as_str(),
					"{} invoked `{}` but an error occurred in command check : {}",
					ctx.author().name,
					ctx.invoked_command_name(),
					error.unwrap_or_else(|| anyhow!("Unknown error"))
				);

				let error_msg = format!(
					"An internal error happened during the command execution. If this error persist please contact the dev with the following code : `{}`", error_identifier.as_braced()
				);

				ctx.shout(error_msg)
					.await
					.map(|_| ())
					.map_err(|_| anyhow!("Failed to send command check failed message"))
			}

			_ => Ok(()),
		} {
			log::error!("{}", error);
		};
	})
}

/// Execute after every successful command
pub fn post_command(ctx: Context) -> BoxFuture<()> {
	Box::pin(async move {
		log::info!(
			"{} invoked `{}` successfully!",
			style(&ctx.author().name).black().bright(),
			style(ctx.invoked_command_name()).black().bright(),
		);
	})
}
