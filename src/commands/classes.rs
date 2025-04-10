//! Setup messages for roles interactions

use crate::{
	commands::levels::autocomplete_levels,
	constants,
	database::{
		models::{Class, Level, NewClass},
		prelude::*,
		schema,
	},
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use fluent::fluent_args;
use poise::{
	command,
	serenity_prelude::{self as serenity, EditRole, Permissions, Role, RoleId},
};

// TODO: possibility to modify a class, show specific informations on a role
/// Add or delete a [`Class`]
#[allow(clippy::unused_async)]
#[command(
	slash_command,
	subcommands("classes_add", "classes_remove", "classes_list"),
	default_member_permissions = "MANAGE_ROLES",
	required_bot_permissions = "MANAGE_ROLES"
)]
pub(crate) async fn classes(_: ApplicationContext<'_>) -> InteractionResult {
	Ok(())
}

/// Configure a new class role
#[command(slash_command, guild_only, rename = "add")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(crate) async fn classes_add(
	ctx: ApplicationContext<'_>,
	name: String,
	#[autocomplete = "autocomplete_levels"] level: String,
	role: Option<Role>,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();
	let mut connection = ctx.data.database.get().await?;

	// TODO: handle no matching level
	let Option::<i32>::Some(level_id) = Level::all_from_guild(guild_id)
		.filter(schema::levels::name.eq(&level))
		.select(schema::levels::id)
		.get_result(&mut connection)
		.await
		.optional()?
	else {
		ctx.shout(ctx.translate(
			"classes_add-no-such-level",
			Some(fluent_args! { "level" => level }),
		))
		.await?;

		return Ok(());
	};

	let nb_of_classes: i64 = Class::all_from_level(level_id)
		.count()
		.get_result(&mut connection)
		.await?;

	if nb_of_classes >= i64::from(constants::limits::MAX_CLASSES_PER_LEVEL) {
		ctx.shout(ctx.translate("classes_add-too-many-classes", None))
			.await?;

		return Ok(());
	}

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

	let new_class = NewClass {
		name: &name,
		level_id,
		guild_id: guild_id.get(),
		role_id: role.id.get(),
	};

	new_class.insert().execute(&mut connection).await?;

	ctx.shout(ctx.translate(
		"classes_add-success",
		Some(fluent_args! { "class" => name, "level" => level }),
	))
	.await?;

	Ok(())
}

// TODO: allow using a result instead of unwrapping everything
/// Autocompletes parameter for `classes` available in `Guild`.
#[allow(clippy::unwrap_used)]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
async fn autocomplete_classes<'a>(
	ctx: ApplicationContext<'_>,
	partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
	// TODO: cache this per guild, db query intensive, use <https://docs.rs/cached/latest/cached/macros/index.html>
	let classes: Vec<_> = Class::all_from_guild(ctx.interaction.guild_id.unwrap())
		.select(schema::classes::name)
		.get_results::<String>(&mut ctx.data.database.get().await.unwrap())
		.await
		.unwrap();

	classes
		.into_iter()
		.filter(move |level| level.contains(partial))
}

/// Delete a class role
#[command(slash_command, guild_only, rename = "remove")]
#[tracing::instrument(skip(ctx))]
pub(crate) async fn classes_remove(
	ctx: ApplicationContext<'_>,
	#[autocomplete = "autocomplete_classes"] name: String,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();
	let mut connection = ctx.data.database.get().await?;

	let Some((id, role_id)) = Class::all_from_guild(guild_id)
		.filter(schema::classes::name.eq(&name))
		.select((schema::classes::id, schema::classes::role_id))
		.first::<(i32, u64)>(&mut connection)
		.await
		.optional()?
	else {
		ctx.shout(ctx.translate("classes_remove-not-found", None))
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

	diesel::delete(Class::with_id(id))
		.execute(&mut connection)
		.await?;

	Ok(())
}

/// List all the available class roles
#[command(slash_command, guild_only, rename = "list")]
#[tracing::instrument(skip(ctx), fields(caller_id = %ctx.interaction.user.id))]
pub(crate) async fn classes_list(
	ctx: ApplicationContext<'_>,
	#[autocomplete = "autocomplete_classes"] filter: Option<String>,
) -> InteractionResult {
	let guild_id = ctx.guild_only_id();

	// TODO: use the cache from autocomplete context
	let classes: Vec<String> = Class::all_from_guild(guild_id)
		.select(schema::classes::name)
		.get_results::<String>(&mut ctx.data.database.get().await?)
		.await?;

	let classes = match filter {
		None => classes,
		Some(ref predicate) => classes
			.into_iter()
			.filter(move |class| class.contains(predicate.as_str()))
			.collect(),
	};

	if classes.is_empty() {
		let get = filter.as_ref().map_or_else(
			|| ctx.translate("classes_list-none", None),
			|filter| {
				ctx.translate(
					"classes_list-none-with-filter",
					Some(fluent_args!["filter" => filter.clone()]),
				)
			},
		);
		ctx.shout(get).await?;

		return Ok(());
	}

	let classes_string = if classes.len() == 1 {
		format!("`{}`", classes[0])
	} else {
		format!(
			"`{}` {} `{}`",
			classes[..classes.len() - 1].join("`, `"),
			ctx.translate("and", None),
			classes[classes.len() - 1]
		)
	};

	let message = format!(
		"**{}**:\n{}",
		filter.map_or_else(
			|| ctx.translate("classes_list-title", None),
			|filter| ctx.translate(
				"classes_list-title-with-filter",
				Some(fluent_args!["filter" => filter]),
			)
		),
		classes_string
	);
	ctx.shout(message).await?;

	Ok(())
}
