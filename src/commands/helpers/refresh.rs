//! A set of commands to refresh the database

use crate::{
	database::{
		models::{Member, NewMember},
		prelude::*,
	},
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use fluent::fluent_args;
use poise::{command, serenity_prelude as serenity};

/// A set of commands to refresh the database
#[allow(clippy::unused_async)]
#[command(
	slash_command,
	owners_only,
	hide_in_help,
	rename = "refresh",
	subcommands("debug_refresh_member", "debug_refresh_members")
)]
pub(super) async fn debug_refresh(_: ApplicationContext<'_>) -> InteractionResult {
	Ok(())
}

/// Loads a guild member in the database
#[command(slash_command, owners_only, hide_in_help, rename = "member")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(super) async fn debug_refresh_member(
	ctx: ApplicationContext<'_>,
	member: serenity::Member,
) -> InteractionResult {
	let mut connection = ctx.data.database.get().await?;

	if let Ok(member) = Member::with_ids(member.user.id, member.guild_id)
		.first::<Member>(&mut connection)
		.await
	{
		ctx.shout(ctx.translate(
			"debug_refresh_member-already-in-database",
			Some(fluent_args!["user" => member.username]),
		))
		.await?;
	} else {
		let new_member = NewMember {
			guild_id: member.guild_id.get(),
			username: member.user.name.as_str(),
			discord_id: member.user.id.get(),
		};

		new_member.insert().execute(&mut connection).await?;

		ctx.shout(ctx.translate(
			"debug_refresh_member-added",
			Some(fluent_args!["user" => new_member.username]),
		))
		.await?;
	}

	Ok(())
}

// Requires the `GUILD_MEMBERS` intent to fetch all members
/// Loads every guild member in the database
#[command(
	slash_command,
	owners_only,
	hide_in_help,
	guild_only,
	rename = "members"
)]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(super) async fn debug_refresh_members(ctx: ApplicationContext<'_>) -> InteractionResult {
	let mut connection = ctx.data.database.get().await?;
	let guild_id = ctx.guild_only_id();

	let mut count = 0;
	let mut last_member_id = None;

	loop {
		let members = guild_id
			.members(&ctx.serenity_context, None, last_member_id)
			.await?;
		let len = members.len();

		if let Some(member) = members.last() {
			last_member_id = Some(member.user.id);
		}

		for member in members {
			if member.user.bot {
				continue;
			}

			let new_member = NewMember {
				guild_id: member.guild_id.get(),
				username: member.user.name.as_str(),
				discord_id: member.user.id.get(),
			};

			match new_member.insert().execute(&mut connection).await {
				Ok(_) => count += 1,
				Err(DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {}
				Err(error) => return Err(error.into()),
			};
		}

		if len < 1000 {
			break;
		}
	}

	ctx.shout(ctx.translate(
		"debug_refresh_members-added",
		Some(fluent_args!["count" => count]),
	))
	.await?;

	Ok(())
}
