//! Commands to act on bot data

use crate::states::{CommandResult, Context};
use poise::{
	builtins::register_application_commands, command, serenity_prelude::ApplicationCommand,
};

/// Register all slash commands to discord
#[command(prefix_command, owners_only, hide_in_help)]
pub async fn register(ctx: Context<'_>, #[flag] global: bool) -> CommandResult {
	match register_application_commands(ctx, global).await {
		Ok(_) => {}
		Err(error) => {
			log::error!("{}", &error);
			ctx.say(format!("Something went wrong: {}!", error)).await?;
		}
	};

	Ok(())
}

/// Unregister all slash commands from discord
#[command(prefix_command, owners_only, hide_in_help)]
pub async fn reset_global(ctx: Context<'_>) -> CommandResult {
	ApplicationCommand::set_global_application_commands(ctx.discord(), |b| b).await?;

	Ok(())
}
