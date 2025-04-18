//! The request handlers that serves content

use super::{AcceptLanguage, ServerError};
use crate::{auth::PendingAuthRequest, states::ArcData};
use anyhow::{anyhow, Context};
use oauth2::AuthorizationCode;
use rocket::{response::Redirect, FromForm, Request, State};
use rocket_dyn_templates::{context, Template};

/// The parameters for the `OAuth2` callback endpoint
#[derive(FromForm)]
pub(crate) struct OAuth2Params {
	/// The code returned by the `OAuth2` provider
	code: String,
	/// The CSRF token to identify the request
	state: String,
}

// TODO: show more comprehensive errors to the user
/// Handle requests to `/oauth2` endpoints
#[rocket::get("/oauth2?<params..>")]
pub(super) async fn handle_oauth2(
	data: &State<ArcData>,
	_lang: AcceptLanguage,
	params: OAuth2Params,
) -> Result<Template, ServerError> {
	// let msg = data.translations.translate_checked(&lang, "", None)?;

	let PendingAuthRequest {
		guild_image_source,
		tx,
		username,
	} = {
		let mut queue = data.auth.pending.write().await;

		queue.remove(&params.state).ok_or(ServerError::User(
			"The given 'state' wasn't queued anymore".into(),
		))?
	};

	let http_client = reqwest::ClientBuilder::new()
		// Following redirects opens the client up to SSRF vulnerabilities.
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.context("could not build http client")?;

	let token_response = data
		.auth
		.client
		.exchange_code(AuthorizationCode::new(params.code))
		.request_async(&http_client)
		.await
		.context("could not get oauth2 token")?;

	tx.send(token_response)
		.map_err(|_| ServerError::Other(anyhow!("the receiver was dropped")))?;

	Ok(Template::render(
		"auth",
		context! {
			username,
			guild_image_source: format!("{guild_image_source}?size=2048")
		},
	))
}

/// Serve the index page
#[rocket::get("/")]
pub(super) fn index() -> Template {
	Template::render("index", context! {})
}

/// Serve the Contact page
#[rocket::get("/contact")]
pub(super) fn contact() -> Template {
	Template::render("contact", context! {})
}

/// Serve the Privacy Policy page
#[rocket::get("/privacy-policy")]
pub(super) fn privacy_policy() -> Template {
	Template::render("privacy-policy", context! {})
}

/// Serve the Terms and Conditions page
#[rocket::get("/terms-and-conditions")]
pub(super) fn terms_and_conditions() -> Template {
	Template::render("terms-and-conditions", context! {})
}

/// Redirects to the main discord server
#[rocket::get("/discord")]
pub(super) fn discord_redirect(data: &State<ArcData>) -> Redirect {
	Redirect::to(format!(
		"https://discord.gg/{}",
		data.config.discord_invite_code
	))
}

/// Catch the `404` status code
#[rocket::catch(404)]
pub(super) fn catch_404(req: &Request<'_>) -> Template {
	Template::render(
		"404",
		context! { ressource_path: req.uri().path().to_string() },
	)
}

/// Catch the `500` status code
#[rocket::catch(500)]
pub(super) fn catch_500() -> Template {
	Template::render("500", context! { message: "Internal Server Error" })
}
