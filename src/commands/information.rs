//! Context Command for informations about a verified member.

use crate::{
	database::models::{Member, VerifiedMember},
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use diesel::prelude::*;
use poise::{command, serenity_prelude::User};

/// Show informations about a registered member
#[command(context_menu_command = "Informations", guild_only)]
pub(crate) async fn information(ctx: ApplicationContext<'_>, user: User) -> InteractionResult {
	let discord_guild_id = match ctx.interaction.guild_id() {
		Some(x) => x,
		None => {
			let get = ctx.get("not-in-guild", None);
			ctx.shout(get).await?;

			return Ok(());
		}
	};

	let member = {
		use crate::database::schema::members::dsl::*;

		members
			.filter(discord_id.eq(user.id.0))
			.filter(guild_id.eq(discord_guild_id.0))
			.first::<Member>(&mut ctx.data.database.get()?)?
	};

	let maybe = VerifiedMember::belonging_to(&member).first(&mut ctx.data.database.get()?);

	let verified_member: VerifiedMember = match maybe {
		Ok(member) => member,
		Err(error) => {
			println!("{:?}", error);

			return Ok(());
		}
	};

	ctx.send(|builder| {
		builder.ephemeral(true).embed(|embed| {
			embed
				.title(format!(
					"{} {}",
					verified_member.first_name, verified_member.last_name
				))
				.field("Mail", verified_member.mail, false)
				.color(0x00FF00)
		})
	})
	.await?;

	Ok(())
}