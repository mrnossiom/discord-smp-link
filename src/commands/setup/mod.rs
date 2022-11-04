//! A set of commands to manage the bot.

use crate::{
	database::{models::Guild, prelude::*, schema},
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use fluent::fluent_args;
use poise::{
	command,
	serenity_prelude::{Permissions, Role},
};

mod message;

use message::setup_message;

/// A set of commands to setup the bot
#[allow(clippy::unused_async)]
#[command(
	slash_command,
	rename = "setup",
	subcommands("setup_message", "setup_role", "setup_pattern"),
	default_member_permissions = "ADMINISTRATOR"
)]
pub(crate) async fn setup(_ctx: ApplicationContext<'_>) -> InteractionResult {
	Ok(())
}

/// Setup the role to apply to verified members.
#[command(slash_command, guild_only, rename = "role")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user().id))]
pub(crate) async fn setup_role(ctx: ApplicationContext<'_>, role: Role) -> InteractionResult {
	let guild_id = ctx.guild_only_id();

	// TODO: check verified role permissions
	if role.has_permission(Permissions::ADMINISTRATOR) {
		let translate = ctx.translate("setup_role-role-admin", None);
		ctx.shout(translate).await?;

		return Ok(());
	}

	// Update the verified role
	diesel::update(Guild::with_id(&guild_id))
		.set(schema::guilds::verified_role_id.eq(role.id.0))
		.execute(&mut ctx.data.database.get().await?)
		.await?;

	let get = ctx.translate("done", None);
	ctx.shout(get).await?;

	Ok(())
}

/// Setup the role to apply to verified members.
#[command(slash_command, guild_only, rename = "pattern")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user().id))]
pub(crate) async fn setup_pattern(
	ctx: ApplicationContext<'_>,
	pattern: String,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();

	// Update the verification email domain
	diesel::update(Guild::with_id(&guild_id))
		.set(schema::guilds::verification_email_domain.eq(&pattern))
		.execute(&mut ctx.data.database.get().await?)
		.await?;

	let get = ctx.translate(
		"setup_pattern-done",
		Some(&fluent_args!["pattern" => pattern]),
	);
	ctx.shout(get).await?;

	Ok(())
}
