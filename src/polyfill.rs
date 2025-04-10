#![allow(dead_code)]

//! Polyfill for the [`MessageComponentInteraction`](poise::serenity_prelude::MessageComponentInteraction) type

use poise::{
	serenity_prelude::{
		self as serenity, ComponentInteraction, CreateInteractionResponseFollowup,
		CreateInteractionResponseMessage, EditInteractionResponse, Member,
	},
	CreateReply,
};
use std::{
	borrow::Cow,
	sync::atomic::{AtomicBool, Ordering},
};

/// The [`poise::Context`] like for Message components interactions
#[derive(Copy, Clone)]
pub(crate) struct MessageComponentContext<'a, U: Send + Sync, E> {
	/// The underlying interaction
	pub(crate) interaction: &'a ComponentInteraction,
	/// The custom user data
	pub(crate) data: &'a U,
	/// The underlying serenity context
	pub(crate) discord: &'a serenity::Context,
	/// Read-only reference to the framework
	///
	/// Useful if you need the list of commands, for example for a custom help command
	pub(crate) framework: poise::FrameworkContext<'a, U, E>,
	/// Keeps track of whether an initial response has been sent.
	///
	/// Discord requires different HTTP endpoints for initial and additional responses.
	pub(crate) has_sent_initial_response: &'a AtomicBool,
}

impl<U: Send + Sync, E> AsRef<serenity::Http> for MessageComponentContext<'_, U, E> {
	fn as_ref(&self) -> &serenity::Http {
		&self.discord.http
	}
}

#[cfg(feature = "cache")]
impl<U: Send + Sync, E> AsRef<serenity::Cache> for MessageComponentContext<'_, U, E> {
	fn as_ref(&self) -> &serenity::Cache {
		&self.discord.cache
	}
}

impl<U: Send + Sync, E> AsRef<serenity::ShardMessenger> for MessageComponentContext<'_, U, E> {
	fn as_ref(&self) -> &serenity::ShardMessenger {
		&self.discord.shard
	}
}

impl<U: Send + Sync, E> serenity::CacheHttp for MessageComponentContext<'_, U, E> {
	fn http(&self) -> &serenity::Http {
		&self.discord.http
	}

	#[cfg(feature = "cache")]
	fn cache(&self) -> Option<&std::sync::Arc<serenity::Cache>> {
		Some(&self.discord.cache)
	}
}

impl<U: Send + Sync, E> MessageComponentContext<'_, U, E> {
	/// Send a message to the user
	pub(crate) async fn send(
		&self,
		reply: CreateReply,
	) -> Result<MessageComponentReplyHandle<'_>, serenity::Error> {
		let has_sent_initial_response = self.has_sent_initial_response.load(Ordering::SeqCst);

		let followup = if has_sent_initial_response {
			Some(Box::new(
				self.interaction
					.create_followup(
						self.discord,
						reply.to_slash_followup_response(
							CreateInteractionResponseFollowup::default(),
						),
					)
					.await?,
			))
		} else {
			self.interaction
				.create_response(
					self.discord,
					serenity::CreateInteractionResponse::Message(
						reply
							.to_slash_initial_response(CreateInteractionResponseMessage::default()),
					),
				)
				.await?;
			self.has_sent_initial_response
				.store(true, std::sync::atomic::Ordering::SeqCst);

			None
		};

		// ReplyHandle contains private fields, so we can't construct nor return it
		// We use our own copy of ReplyHandle
		Ok(MessageComponentReplyHandle {
			http: &self.discord.http,
			interaction: self.interaction,
			followup,
		})
	}

	/// Send an ephemeral message to the user
	#[inline]
	pub(crate) async fn shout(
		&self,
		content: impl Into<String> + Send,
	) -> Result<MessageComponentReplyHandle<'_>, serenity::Error> {
		self.send(
			CreateReply::default()
				.content(content.into())
				.ephemeral(true),
		)
		.await
	}

	/// Get the member who triggered the interaction
	///
	/// # Panics
	/// Panics if used in a non-guild context
	#[inline]
	#[track_caller]
	pub(crate) fn guild_only_member(&self) -> Member {
		self.interaction
			.member
			.clone()
			.expect("not in a guild context")
	}
}

/// Returned from [`MessageComponentContext::send()`] to operate on the sent message
///
/// Discord sometimes returns the [`serenity::Message`] object directly, but sometimes you have to
/// request it manually. This enum abstracts over the two cases
#[derive(Clone)]
pub(crate) struct MessageComponentReplyHandle<'a> {
	/// Serenity HTTP instance that can be used to request the interaction response message
	/// object
	http: &'a serenity::Http,
	/// Interaction which contains the necessary data to request the interaction response
	/// message object
	interaction: &'a serenity::ComponentInteraction,
	/// If this is a followup response, the Message object (which Discord only returns for
	/// followup responses, not initial)
	followup: Option<Box<serenity::Message>>,
}

impl MessageComponentReplyHandle<'_> {
	/// Retrieve the message object of the sent reply.
	///
	/// If you don't need ownership of Message, you can use [`Self::message`]
	///
	/// Only needs to do an HTTP request in the application command response case
	pub(crate) async fn into_message(self) -> Result<serenity::Message, serenity::Error> {
		self.interaction.get_response(self.http).await
	}

	/// Retrieve the message object of the sent reply.
	///
	/// Returns a reference to the known Message object, or fetches the message from the discord API.
	///
	/// To get an owned [`serenity::Message`], use [`Self::into_message()`]
	pub(crate) async fn message(&self) -> Result<Cow<'_, serenity::Message>, serenity::Error> {
		Ok(Cow::Owned(self.interaction.get_response(self.http).await?))
	}

	/// Edits the message that this [`Self`] points to
	pub(crate) async fn edit(&self, reply: CreateReply) -> Result<(), serenity::Error> {
		if let Some(followup) = &self.followup {
			self.interaction
				.edit_followup(
					self.http,
					followup.id,
					reply.to_slash_followup_response(CreateInteractionResponseFollowup::default()),
				)
				.await?;
		} else {
			self.interaction
				.edit_response(
					self.http,
					reply.to_slash_initial_response_edit(EditInteractionResponse::default()),
				)
				.await?;
		}

		Ok(())
	}
}
