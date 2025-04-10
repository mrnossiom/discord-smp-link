//! Command to link Discord and Google accounts together.
//! TODO (LARGE) : refactor this file

use crate::{
	auth::{GoogleAuthentificationError, GoogleUserMetadata},
	constants,
	database::{
		self,
		models::{Class, Guild, Level, Member, NewVerifiedMember, VerifiedMember},
		prelude::*,
		schema, DatabasePooledConnection,
	},
	polyfill::MessageComponentReplyHandle,
	states::{InteractionError, InteractionResult, MessageComponentContext},
	translation::Translate,
};
use anyhow::{anyhow, Context};
use diesel::dsl;
use fluent::fluent_args;
use poise::{
	serenity_prelude::{
		self as serenity, ComponentInteractionCollector, ComponentInteractionDataKind,
		CreateActionRow, CreateButton, CreateSelectMenu, CreateSelectMenuKind,
		CreateSelectMenuOption, GuildId, RoleId,
	},
	CreateReply,
};
use std::time::Duration;

// TODO: heist all requirements and move every database update or Discord call to the end
// TODO: document steps because it becomes messy here
/// Starts the auth process after the user clicked on the login button
#[allow(clippy::too_many_lines)]
#[tracing::instrument(skip_all, fields(caller_id = %ctx.interaction.user.id))]
pub(crate) async fn login(ctx: MessageComponentContext<'_>) -> InteractionResult {
	let mut connection = ctx.data.database.get().await?;
	let member = ctx.guild_only_member();

	if dsl::select(dsl::exists(VerifiedMember::with_ids(
		member.user.id,
		member.guild_id,
	)))
	.get_result(&mut connection)
	.await?
	{
		ctx.shout(ctx.translate("login.already-verified", None))
			.await?;

		return Ok(());
	}

	let (verified_role, levels, email_pattern) =
		match check_login_components(&mut connection, member.guild_id).await {
			Ok(components) => components,
			Err(err) => match err {
				CheckLoginComponentsError::Database(err) => return Err(err.into()),
				CheckLoginComponentsError::NoEmailPattern
				| CheckLoginComponentsError::NoVerifiedRole
				| CheckLoginComponentsError::NoLevels => {
					// TODO: translate errors

					ctx.shout(err.to_string()).await?;

					return Ok(());
				}
			},
		};

	let (oauth2_url, token_response) = ctx
		.data
		.auth
		.process_oauth2(
			member.user.name.clone(),
			member
				.guild_id
				.to_partial_guild(&ctx)
				.await?
				.icon_url()
				.unwrap_or_default(),
		)
		.await;

	let action_row = CreateActionRow::Buttons(vec![
		CreateButton::new_link(oauth2_url).label(ctx.translate("continue", None))
	]);

	let initial_response = ctx
		.send(
			CreateReply::default()
				.ephemeral(true)
				.content(ctx.translate("use-google-account-to-login", None))
				.components(vec![action_row]),
		)
		.await?;

	let token_response = match token_response.await {
		Ok(response) => response,
		Err(GoogleAuthentificationError::Timeout) => {
			ctx.shout(ctx.translate("did-not-finish-auth-process", None))
				.await?;

			return Ok(());
		}
		Err(error) => return Err(error).context("Failed to get token response")?,
	};

	let user_data = ctx
		.data
		.auth
		.query_google_user_metadata(&token_response)
		.await
		.context("Failed to query google user metadata")?;

	let mail_domain = user_data
		.mail
		.split('@')
		.next_back()
		.context("email returned by google is invalid")?;
	if mail_domain != email_pattern {
		ctx.shout(ctx.translate("event-login-email-domain-not-allowed", None))
			.await?;

		return Ok(());
	}

	let (level_id, class_id) =
		match ask_user_guild_and_levels(&ctx, &mut connection, &initial_response, levels).await? {
			Ok((level_id, class_id)) => (level_id, class_id),
			Err(msg) => {
				ctx.shout(msg).await?;

				return Ok(());
			}
		};

	let Some(verified_member_id) = Member::with_ids(member.user.id, member.guild_id)
		.select(schema::members::id)
		.first::<i32>(&mut connection)
		.await
		.optional()?
	else {
		ctx.shout(ctx.translate(
			"error-member-not-registered",
			Some(fluent_args!["user" => member.user.name.as_str()]),
		))
		.await?;

		return Ok(());
	};

	apply_changes(
		&ctx,
		&mut connection,
		&member,
		user_data,
		verified_member_id,
		verified_role,
		(level_id, class_id),
	)
	.await?;

	initial_response
		.edit(
			CreateReply::default()
				.content(ctx.translate("authentication-successful", None))
				.components(vec![]),
		)
		.await?;

	Ok(())
}

// TODO: improve next function and remove this
/// Error type for the following function
#[derive(Debug, thiserror::Error)]
enum CheckLoginComponentsError {
	/// Verified role has not been setup yet
	#[error("Verified role has not been setup yet")]
	NoVerifiedRole,
	/// Email pattern has not been setup yet
	#[error("Email pattern has not been setup yet")]
	NoEmailPattern,
	/// No levels found in this guild
	#[error("No levels found in this guild")]
	NoLevels,

	/// An error from the database
	#[error(transparent)]
	Database(#[from] DieselError),
}

/// Extracted logic
async fn check_login_components(
	connection: &mut DatabasePooledConnection,
	guild_id: GuildId,
) -> Result<(RoleId, Vec<Level>, String), CheckLoginComponentsError> {
	let (verified_role, email_pattern) = {
		let (inner_role_id, email_pattern): (Option<u64>, Option<String>) =
			Guild::with_id(guild_id)
				.select((
					schema::guilds::verified_role_id,
					schema::guilds::verification_email_domain,
				))
				.first(connection)
				.await?;

		let inner_role = match inner_role_id {
			Some(role_id) => RoleId::new(role_id),
			None => {
				return Err(CheckLoginComponentsError::NoVerifiedRole);
			}
		};

		let Some(email_pattern) = email_pattern else {
			return Err(CheckLoginComponentsError::NoEmailPattern);
		};

		(inner_role, email_pattern)
	};

	let levels: Vec<Level> = Level::all_from_guild(guild_id)
		.get_results::<Level>(connection)
		.await?;

	if levels.is_empty() {
		return Err(CheckLoginComponentsError::NoLevels);
	}

	Ok((verified_role, levels, email_pattern))
}

/// Ask the user to select a level and then a guild
async fn ask_user_guild_and_levels<'a>(
	ctx: &'a MessageComponentContext<'a>,
	connection: &mut database::DatabasePooledConnection,
	initial_response: &MessageComponentReplyHandle<'a>,
	mut levels: Vec<Level>,
	// TODO: remove ugly as hell return type
) -> anyhow::Result<Result<(i32, i32), String>> {
	let levels = levels
		.iter_mut()
		.map(|cl| CreateSelectMenuOption::new(&cl.name, cl.id.to_string()))
		.collect::<Vec<_>>();

	let levels_select_menu = CreateSelectMenu::new(
		constants::events::AUTHENTICATION_SELECT_MENU_LEVEL_INTERACTION,
		CreateSelectMenuKind::String { options: levels },
	)
	.placeholder(ctx.translate("event-login-select-level", None));

	let action_row = CreateActionRow::SelectMenu(levels_select_menu);

	initial_response
		.edit(
			CreateReply::default()
				.ephemeral(true)
				.components(vec![action_row])
				// Empty the previous content
				.content(""),
		)
		.await?;

	let level_id = if let Some(interaction) = ComponentInteractionCollector::new(ctx)
		.message_id(initial_response.message().await?.id)
		.timeout(Duration::from_secs(60))
		.await
	{
		interaction.defer(&ctx).await?;

		let ComponentInteractionDataKind::StringSelect { values } = interaction.data.kind else {
			unreachable!()
		};

		<[String]>::first(&values)
			.ok_or_else(|| anyhow!("Something went wrong while parsing class id"))?
			.parse::<i32>()?
	} else {
		return Ok(Err(ctx.translate("error-user-timeout", None)));
	};

	let mut classes: Vec<Class> = Class::all_from_level(level_id)
		.get_results::<Class>(connection)
		.await?;

	if classes.is_empty() {
		return Ok(Err(ctx.translate("event-login-no-classes", None)));
	}

	let classes = classes
		.iter_mut()
		.map(|cl| CreateSelectMenuOption::new(&cl.name, cl.id.to_string()))
		.collect::<Vec<_>>();

	let classes_select_menu = CreateSelectMenu::new(
		constants::events::AUTHENTICATION_SELECT_MENU_CLASS_INTERACTION,
		CreateSelectMenuKind::String { options: classes },
	)
	.placeholder(ctx.translate("event-login-select-class", None));

	let action_row = CreateActionRow::SelectMenu(classes_select_menu);

	initial_response
		.edit(
			CreateReply::default()
				.ephemeral(true)
				.components(vec![action_row])
				// Empty the previous content
				.content(""),
		)
		.await?;

	let class_id = if let Some(interaction) = ComponentInteractionCollector::new(ctx)
		.message_id(initial_response.message().await?.id)
		.timeout(Duration::from_secs(60))
		.await
	{
		interaction.defer(&ctx).await?;

		let ComponentInteractionDataKind::StringSelect { values } = interaction.data.kind else {
			unreachable!()
		};

		<[String]>::first(&values)
			.ok_or_else(|| anyhow!("Something went wrong while parsing class id"))?
			.parse::<i32>()?
	} else {
		return Ok(Err(ctx.translate("error-user-timeout", None)));
	};

	Ok(Ok((level_id, class_id)))
}

/// Apply the changes to the user, updating the database and the Discord roles
async fn apply_changes(
	ctx: &MessageComponentContext<'_>,
	mut connection: &mut DatabasePooledConnection,
	member: &serenity::Member,
	user_data: GoogleUserMetadata,
	verified_member_id: i32,
	verified_role: RoleId,
	(level_id, class_id): (i32, i32),
) -> Result<(), InteractionError> {
	// Get Discord roles ids
	let level_role = {
		let id = Level::with_id(level_id)
			.select(schema::levels::role_id)
			.first::<u64>(&mut connection)
			.await?;

		RoleId::new(id)
	};
	let class_role = {
		let id = Class::with_id(class_id)
			.select(schema::classes::role_id)
			.first::<u64>(&mut connection)
			.await?;

		RoleId::new(id)
	};

	// Update Discord roles for new verified member
	match member.add_role(&ctx, verified_role).await {
		Ok(()) => {}
		Err(serenity::Error::Model(serenity::ModelError::RoleNotFound)) => {
			diesel::update(Guild::with_id(member.guild_id))
				.set(schema::guilds::verified_role_id.eq::<Option<u64>>(None))
				.execute(&mut connection)
				.await?;

			return Err(anyhow!("Verified role was deleted").into());
		}
		Err(error) => return Err(error.into()),
	}
	match member.add_role(&ctx, level_role).await {
		Ok(()) => {}
		Err(serenity::Error::Model(serenity::ModelError::RoleNotFound)) => {
			diesel::delete(Level::with_id(level_id))
				.execute(&mut connection)
				.await?;

			return Err(anyhow!("Level role was deleted").into());
		}
		Err(error) => return Err(error.into()),
	}
	match member.add_role(&ctx, class_role).await {
		Ok(()) => {}
		Err(serenity::Error::Model(serenity::ModelError::RoleNotFound)) => {
			diesel::delete(Class::with_id(class_id))
				.execute(&mut connection)
				.await?;

			return Err(anyhow!("Level role was deleted").into());
		}
		Err(error) => return Err(error.into()),
	}

	let new_verified_member = NewVerifiedMember {
		member_id: verified_member_id,
		first_name: &user_data.first_name,
		last_name: &user_data.last_name,
		mail: &user_data.mail,
		class_id,
	};

	new_verified_member
		.insert()
		.execute(&mut connection)
		.await?;

	Ok(())
}
