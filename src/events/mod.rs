//! `Discord` client events handlers

use crate::{
	commands::helpers::register_,
	constants::events,
	database::{
		models::{Guild, Member, NewGuild, NewMember},
		prelude::*,
		schema::{guilds, members},
	},
	states::{ArcData, FrameworkContext, InteractionResult, MessageComponentContext},
};
use anyhow::Context;
use poise::serenity_prelude::{self, ComponentInteractionDataKind, FullEvent, Interaction};
use std::sync::atomic::AtomicBool;

mod groups;
mod login;
mod logout;

/// Serenity listener to react to `Discord` events
#[allow(clippy::too_many_lines)]
pub(crate) async fn event_handler(
	ctx: &serenity_prelude::Context,
	event: &FullEvent,
	framework: FrameworkContext<'_>,
	data: &ArcData,
) -> InteractionResult {
	match event {
		FullEvent::Ready { data_about_bot } => {
			register_(
				&ctx.http,
				&data.config.discord_development_guild,
				&framework.options.commands,
			)
			.await
			.context("Could not register guild commands")?;

			tracing::info!("`{}` is ready!", data_about_bot.user.name);

			Ok(())
		}

		FullEvent::GuildMemberAddition { new_member } => {
			let mut connection = data.database.get().await?;

			if let Ok(user) = members::table
				.filter(members::discord_id.eq(new_member.user.id.get()))
				.filter(members::guild_id.eq(new_member.guild_id.get()))
				.first::<Member>(&mut connection)
				.await
			{
				tracing::warn!(
					guild_id = user.discord_id,
					"User `{}` already exists in the database",
					user.username,
				);
			} else {
				let new_user = NewMember {
					guild_id: new_member.guild_id.get(),
					username: new_member.user.name.as_str(),
					discord_id: new_member.user.id.get(),
				};

				tracing::info!(
					user_id = new_user.discord_id,
					"Adding user `{}` to database",
					new_user.username,
				);

				diesel::insert_into(members::table)
					.values(&new_user)
					.execute(&mut connection)
					.await?;
			}

			Ok(())
		}

		FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
			tracing::info!(guild_id = guild_id.get(), "Deleting member `{}`", user.name);

			diesel::delete(
				members::table
					.filter(members::guild_id.eq(guild_id.get()))
					.filter(members::discord_id.eq(user.id.get())),
			)
			.execute(&mut data.database.get().await?)
			.await?;

			Ok(())
		}

		FullEvent::GuildCreate { guild, .. } => {
			let mut connection = data.database.get().await?;

			if let Ok(guild) = guilds::table
				.filter(guilds::id.eq(guild.id.get()))
				.first::<Guild>(&mut connection)
				.await
			{
				tracing::warn!(
					guild_id = guild.id,
					"Guild `{}` already exists in the database",
					guild.name,
				);
			} else {
				let new_guild = NewGuild {
					id: guild.id.get(),
					name: guild.name.as_str(),
					owner_id: guild.owner_id.get(),
					verification_email_domain: None,
					login_message_id: None,
					groups_message_id: None,
					verified_role_id: None,
				};

				tracing::info!(
					guild_id = guild.id.get(),
					"Adding guild `{}` to database",
					guild.name
				);

				diesel::insert_into(guilds::table)
					.values(&new_guild)
					.execute(&mut connection)
					.await?;
			}

			Ok(())
		}

		FullEvent::GuildDelete { incomplete, .. } => {
			tracing::warn!("Deleting guild ({})", incomplete.id);

			diesel::delete(guilds::table.filter(guilds::id.eq(incomplete.id.get())))
				.execute(&mut data.database.get().await?)
				.await?;

			Ok(())
		}

		FullEvent::InteractionCreate {
			interaction: Interaction::Component(interaction),
		} => {
			let ctx = MessageComponentContext {
				interaction,
				framework,
				data,
				discord: ctx,
				has_sent_initial_response: &AtomicBool::new(false),
			};

			tracing::info!(
				user_id = ctx.interaction.user.id.get(),
				custom_id = ctx.interaction.data.custom_id,
				"`{}` interacted with a component",
				ctx.interaction.user.name,
			);

			match interaction.data.custom_id.as_str() {
				events::LOGIN_BUTTON_INTERACTION => login::login(ctx).await,
				events::LOGOUT_BUTTON_INTERACTION => logout::logout(ctx).await,

				events::GROUPS_SELECT_MENU_INTERACTION => {
					let ComponentInteractionDataKind::StringSelect { values } =
						&interaction.data.kind
					else {
						unreachable!()
					};

					groups::groups(ctx, values).await
				}

				_ => Ok(()),
			}
		}

		_ => {
			tracing::trace!(event = ?event, "missed event");

			Ok(())
		}
	}
}
