//! Act on discord client metadata

use crate::states::{ApplicationContext, Command, InteractionResult};
use poise::{
	command,
	serenity_prelude::{self as serenity, GuildId, Http},
};

mod force;
mod refresh;
mod register;

use force::debug_force;
use refresh::debug_refresh;
use register::debug_register;

/// A set of commands restricted to owners
/// Can be registered with [`_register`] prefix command
#[allow(clippy::unused_async)]
#[command(
	slash_command,
	owners_only,
	hide_in_help,
	subcommands("debug_force", "debug_refresh", "debug_register")
)]
pub(crate) async fn debug(_: ApplicationContext<'_>) -> InteractionResult {
	Ok(())
}

/// Register all development slash commands
pub(crate) async fn register_(
	http: &Http,
	guild_id: &GuildId,
	commands: &Vec<Command>,
) -> Result<(), serenity::Error> {
	let mut commands_collector = Vec::new();

	for command in commands {
		if let Some(slash_command) = command.create_as_slash_command() {
			commands_collector.push(slash_command);
		}

		if let Some(context_menu_command) = command.create_as_context_menu_command() {
			commands_collector.push(context_menu_command);
		}
	}

	guild_id.set_commands(http, commands_collector).await?;

	Ok(())
}
