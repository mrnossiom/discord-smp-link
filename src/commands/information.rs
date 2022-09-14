//! Context Command for informations about a verified member.

use crate::{
	database::models::VerifiedMember,
	states::{ApplicationContext, ApplicationContextPolyfill, InteractionResult},
	translation::Translate,
};
use anyhow::anyhow;
use diesel::prelude::*;
use fluent::fluent_args;
use poise::{command, serenity_prelude::User};

/// Show informations about a registered member
#[command(context_menu_command = "Informations", guild_only)]
pub(crate) async fn information(ctx: ApplicationContext<'_>, user: User) -> InteractionResult {
	let guild_id = ctx
		.interaction
		.guild_id()
		.ok_or_else(|| anyhow!("guild only command"))?;

	let verified_member: VerifiedMember = {
		use crate::database::schema::{
			members::dsl as members, verified_members::dsl as verified_members,
		};

		let verified_member = verified_members::verified_members
			.inner_join(members::members)
			.filter(members::discord_id.eq(user.id.0))
			.filter(members::guild_id.eq(guild_id.0))
			.select((
				verified_members::member_id,
				verified_members::mail,
				verified_members::first_name,
				verified_members::last_name,
				verified_members::class_id,
			))
			.first::<VerifiedMember>(&mut ctx.data.database.get()?);

		match verified_member {
			Ok(x) => x,
			Err(_) => {
				let get = ctx.get(
					"error-member-not-verified",
					Some(&fluent_args!["user" => user.name]),
				);
				ctx.shout(get).await?;

				return Ok(());
			}
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
				.footer(|footer| footer.text("Discord SMP Link © 2023"))
		})
	})
	.await?;

	Ok(())
}
