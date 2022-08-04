//! `Discord` client events handlers

use crate::{
	database::{
		models::{Guild, Member, NewGuild, NewMember},
		schema::{guilds, members},
	},
	states::{Data, Framework, STATE},
};
use anyhow::Result;
use diesel::prelude::*;
use poise::{serenity_prelude::Context, Event};

/// Serenity listener to react to `Discord` events
pub fn event_handler(
	_ctx: &Context,
	event: &Event,
	_framework: &Framework,
	_data: &Data,
) -> Result<()> {
	match event {
		Event::Ready { data_about_bot } => {
			tracing::info!("{} is ready!", data_about_bot.user.name);

			Ok(())
		}

		Event::GuildMemberAddition { new_member } => {
			if let Ok(user) = members::table
				.filter(members::discord_id.eq(new_member.user.id.0))
				.filter(members::guild_id.eq(new_member.guild_id.0))
				.first::<Member>(&STATE.database.get()?)
			{
				tracing::warn!(
					"User `{}` ({}) already exists in the database",
					user.username,
					user.discord_id
				);
			} else {
				let new_user = NewMember {
					guild_id: new_member.guild_id.0,
					username: new_member.user.name.as_str(),
					discord_id: new_member.user.id.0,
				};

				tracing::info!(
					"Adding user `{}` ({}) to database",
					new_user.username,
					new_user.discord_id
				);

				diesel::insert_into(members::table)
					.values(&new_user)
					.execute(&STATE.database.get()?)?;
			}

			Ok(())
		}

		Event::GuildMemberRemoval { guild_id, user, .. } => {
			tracing::info!("Deleting member ({})", guild_id.0);

			diesel::delete(
				members::table
					.filter(members::guild_id.eq(guild_id.0))
					.filter(members::discord_id.eq(user.id.0)),
			)
			.execute(&STATE.database.get()?)?;

			Ok(())
		}

		Event::GuildCreate { guild, .. } => {
			if let Ok(guild) = guilds::table
				.filter(guilds::id.eq(guild.id.0))
				.first::<Guild>(&STATE.database.get()?)
			{
				tracing::warn!(
					"Guild `{}` ({}) already exists in the database",
					guild.name,
					guild.id
				);
			} else {
				let new_guild = NewGuild {
					id: guild.id.0,
					name: guild.name.as_str(),
					owner_id: guild.owner_id.0,
					setup_message_id: None,
				};

				tracing::info!("Adding guild `{}` ({}) to database", guild.name, guild.id);

				diesel::insert_into(guilds::table)
					.values(&new_guild)
					.execute(&STATE.database.get()?)?;
			}

			Ok(())
		}

		Event::GuildDelete { incomplete, .. } => {
			tracing::warn!("Deleting guild ({})", incomplete.id);

			diesel::delete(guilds::table.filter(guilds::id.eq(incomplete.id.0)))
				.execute(&STATE.database.get()?)?;

			Ok(())
		}

		_ => {
			tracing::debug!("You didn't handle this event : {:?}", event);

			Ok(())
		}
	}
}
