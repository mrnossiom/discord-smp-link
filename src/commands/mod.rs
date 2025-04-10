//! `Discord` client commands

use crate::{
	states::{Context, ContextPolyfill, FrameworkError, InteractionError},
	translation::Translate,
};
use anyhow::{anyhow, Context as _};
use fluent::fluent_args;
use poise::{serenity_prelude, BoxFuture};
use uuid::Uuid;

mod classes;
mod groups;
mod information;
mod levels;
mod setup;

pub(crate) use classes::classes;
pub(crate) use groups::groups;
pub(crate) use information::information;
pub(crate) use levels::levels;
pub(crate) use setup::setup;
pub(crate) mod helpers;

/// Execute before each command
pub(crate) fn pre_command(ctx: Context) -> BoxFuture<()> {
	Box::pin(async move {
		tracing::info!(
			user_id = ctx.author().id.get(),
			username = &ctx.author().name,
			command_id = ctx.command().identifying_name,
			"Command invocation",
		);
	})
}

/// Execute on a error during code execution
#[allow(clippy::too_many_lines)]
pub(crate) fn command_on_error(error: FrameworkError) -> BoxFuture<()> {
	Box::pin(async move {
		let error = match error {
			FrameworkError::Command { error, ctx, .. } => handle_interaction_error(ctx, error)
				.await
				.context("failed to send error message"),

			FrameworkError::EventHandler { error, event, .. } => {
				tracing::error!(
					error = ?error,
					event = ?event,
					"event handler",
				);

				Ok(())
			}

			FrameworkError::CommandCheckFailed { ctx, error, .. } => {
				// TODO: handle no error
				if let Some(err) = error {
					handle_interaction_error(ctx, err)
						.await
						.context("failed to send error message")
				} else {
					Err(anyhow!("No error provided"))
				}
			}

			FrameworkError::MissingBotPermissions {
				ctx,
				missing_permissions,
				..
			} => ctx
				.shout(ctx.translate(
					"error-bot-missing-permissions",
					Some(fluent_args!["permissions" => missing_permissions.to_string()]),
				))
				.await
				.map(|_| ())
				.context("Failed to send missing bot permissions message"),

			FrameworkError::MissingUserPermissions {
				ctx,
				missing_permissions,
				..
			} => {
				let text = missing_permissions.map_or_else(
					|| ctx.translate("error-user-missing-unknown-permissions", None),
					|permission| {
						ctx.translate(
							"error-user-missing-permissions",
							Some(fluent_args!["permissions" => permission.to_string()]),
						)
					},
				);

				ctx.shout(text)
					.await
					.map(|_| ())
					.context("Failed to send missing user permissions message")
			}

			FrameworkError::NotAnOwner { ctx, .. } => ctx
				.shout(ctx.translate("error-not-an-owner", None))
				.await
				.map(|_| ())
				.context("Failed to send not an owner message"),

			FrameworkError::GuildOnly { ctx, .. } => ctx
				.shout(ctx.translate("error-guild-only", None))
				.await
				.map(|_| ())
				.context("Failed to send guild only message"),

			FrameworkError::DmOnly { ctx, .. } => ctx
				.shout(ctx.translate("error-dm-only", None))
				.await
				.map(|_| ())
				.context("Failed to send dm only message"),

			error => {
				tracing::error!(error = ?error, "framework");

				Ok(())
			}
		};

		if let Err(error) = error {
			tracing::error!(error = ?error);
		}
	})
}

/// Execute after every successful command
pub(crate) fn post_command(ctx: Context) -> BoxFuture<()> {
	Box::pin(async move {
		tracing::debug!(
			user_id = ctx.author().id.get(),
			username = &ctx.author().name,
			command_id = ctx.command().identifying_name,
			"Command invocation successful",
		);
	})
}

/// Handle our custom command interaction error
async fn handle_interaction_error(
	ctx: Context<'_>,
	error: InteractionError,
) -> serenity_prelude::Result<()> {
	let error_identifier = Uuid::new_v4().hyphenated().to_string();

	tracing::error!(
		user_id = ctx.author().id.get(),
		username = ctx.author().name,
		error_id = error_identifier,
		error = ?error,
		command_id = ctx.command().identifying_name,
		"interaction body or check",
	);

	ctx.shout(ctx.translate(
		"error-internal-with-id",
		Some(fluent_args!["id" => error_identifier]),
	))
	.await?;

	Ok(())
}
