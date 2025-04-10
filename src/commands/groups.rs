//! Setup messages for roles interactions

use crate::{
	constants,
	database::{
		models::{Group, NewGroup},
		prelude::*,
		schema,
	},
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use fluent::fluent_args;
use poise::{
	command,
	serenity_prelude::{self as serenity, EditRole, Permissions, ReactionType, Role, RoleId},
};

// TODO: possibility to modify a group
/// Add or delete a [`Group`]
#[allow(clippy::unused_async)]
#[command(
	slash_command,
	subcommands("groups_add", "groups_remove", "groups_list"),
	default_member_permissions = "MANAGE_ROLES",
	required_bot_permissions = "MANAGE_ROLES"
)]
pub(crate) async fn groups(_: ApplicationContext<'_>) -> InteractionResult {
	Ok(())
}

/// Configure a new group tag role
#[command(slash_command, guild_only, rename = "add")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(crate) async fn groups_add(
	ctx: ApplicationContext<'_>,
	name: String,
	role: Option<Role>,
	emoji: Option<String>,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();

	// TODO: ugly, find a better way to do this
	let emoji = if let Some(emoji) = emoji {
		if let Ok(emoji) = emoji.parse::<ReactionType>() {
			Some(emoji)
		} else {
			ctx.shout(ctx.translate("groups_add-invalid-emoji", None))
				.await?;

			return Ok(());
		}
	} else {
		None
	};

	let role = match role {
		Some(role) => role,
		None => {
			guild_id
				.create_role(
					&ctx.serenity_context,
					EditRole::new()
						.name(&name)
						.permissions(Permissions::empty())
						.mentionable(true),
				)
				.await?
		}
	};

	let emoji_data = emoji.map(|rt| format!("{rt}"));
	let new_group = NewGroup {
		name: &name,
		emoji: emoji_data.as_deref(),

		guild_id: guild_id.get(),
		role_id: role.id.get(),
	};

	new_group
		.insert()
		.execute(&mut ctx.data.database.get().await?)
		.await?;

	ctx.shout(ctx.translate("groups_add-success", Some(fluent_args! { "group" => name })))
		.await?;

	Ok(())
}

// TODO: allow using a result instead of unwrapping everything
/// Autocompletes parameter for `groups` available in `Guild`.
#[allow(clippy::unwrap_used)]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
async fn autocomplete_groups<'a>(
	ctx: ApplicationContext<'_>,
	partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
	// TODO: cache this per guild, db query intensive
	let groups: Vec<_> = Group::all_from_guild(ctx.interaction.guild_id.unwrap())
		.select(schema::groups::name)
		.get_results::<String>(&mut ctx.data.database.get().await.unwrap())
		.await
		.unwrap();

	groups
		.into_iter()
		.filter(move |group| group.contains(partial))
}

/// Delete a group tag role
#[command(slash_command, guild_only, rename = "remove")]
#[tracing::instrument(skip(ctx))]
pub(crate) async fn groups_remove(
	ctx: ApplicationContext<'_>,
	#[autocomplete = "autocomplete_groups"] name: String,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();

	let Some((id, role_id)) = Group::all_from_guild(guild_id)
		.filter(schema::groups::name.eq(&name))
		.select((schema::groups::id, schema::groups::role_id))
		.first::<(i32, u64)>(&mut ctx.data.database.get().await?)
		.await
		.optional()?
	else {
		ctx.shout(ctx.translate("groups_remove-not-found", None))
			.await?;

		return Ok(());
	};

	match guild_id
		.delete_role(&ctx.serenity_context, RoleId::new(role_id))
		.await
	{
		Ok(()) |
		// Ignore the error if the role is already deleted
		Err(serenity::Error::Http(_)) => {}
		Err(error) => return Err(error.into()),
	}

	diesel::delete(Group::with_id(id))
		.execute(&mut ctx.data.database.get().await?)
		.await?;

	Ok(())
}

/// List all available group tag roles
#[command(slash_command, guild_only, rename = "list")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(crate) async fn groups_list(
	ctx: ApplicationContext<'_>,
	#[autocomplete = "autocomplete_groups"] filter: Option<String>,
) -> InteractionResult {
	// TODO: cache this per guild, db query intensive, use <https://docs.rs/cached/latest/cached/macros/index.html>
	let guild_id = ctx.guild_only_id();
	let mut connection = ctx.data.database.get().await?;

	let nb_of_groups: i64 = Group::all_from_guild(guild_id)
		.count()
		.get_result(&mut connection)
		.await?;

	if nb_of_groups >= i64::from(constants::limits::MAX_GROUPS_PER_GUILD) {
		ctx.shout(ctx.translate("groups_add-too-many-groups", None))
			.await?;

		return Ok(());
	}

	// TODO: use the cache from autocomplete context
	let groups: Vec<String> = Group::all_from_guild(guild_id)
		.select(schema::groups::name)
		.get_results::<String>(&mut connection)
		.await?;

	let groups = match filter {
		None => groups,
		Some(ref predicate) => groups
			.into_iter()
			.filter(move |group| group.contains(predicate.as_str()))
			.collect(),
	};

	if groups.is_empty() {
		let get = filter.as_ref().map_or_else(
			|| ctx.translate("groups_list-none", None),
			|filter| {
				ctx.translate(
					"groups_list-none-with-filter",
					Some(fluent_args!["filter" => filter.clone()]),
				)
			},
		);
		ctx.shout(get).await?;

		return Ok(());
	}

	let groups_string = if groups.len() == 1 {
		format!("`{}`", groups[0])
	} else {
		format!(
			"`{}` {} `{}`",
			groups[..groups.len() - 1].join("`, `"),
			ctx.translate("and", None),
			groups[groups.len() - 1]
		)
	};

	let message = format!(
		"**{}**:\n{}",
		filter.map_or_else(
			|| ctx.translate("groups_list-title", None),
			|filter| ctx.translate(
				"groups_list-title-with-filter",
				Some(fluent_args!["filter" => filter]),
			)
		),
		groups_string
	);
	ctx.shout(message).await?;

	Ok(())
}
