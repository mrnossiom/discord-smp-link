//! Handle oauth2 flow with users

use crate::states::Config;
use futures::Future;
use oauth2::{
	basic::BasicClient, url::Url, AuthUrl, CsrfToken, RedirectUrl, RefreshToken, RevocationUrl,
	Scope, TokenUrl,
};
use std::{
	collections::HashMap,
	pin::Pin,
	sync::{Arc, RwLock},
	task::{Context, Poll},
	time::{Duration, Instant},
};

#[derive(Debug)]
pub struct AuthLink {
	pub client: BasicClient,
	pub queue: Arc<RwLock<HashMap<String, Option<RefreshToken>>>>,
}

impl AuthLink {
	#[must_use]
	pub fn new(config: &Config) -> Self {
		let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())
			.expect("invalid auth url");
		let token_url = TokenUrl::new("https://www.googleapis.com/oauth2/v3/token".into())
			.expect("invalid token url");

		let oauth_client = BasicClient::new(
			config.google_client.0.clone(),
			Some(config.google_client.1.clone()),
			auth_url,
			Some(token_url),
		)
		.set_redirect_uri(if config.production {
			RedirectUrl::new("http://somedumbdomain.lol/oauth2".into())
				.expect("invalid redirect url")
		} else {
			RedirectUrl::new(format!("http://localhost:{}/oauth2", config.port))
				.expect("invalid redirect url")
		})
		.set_revocation_uri(
			RevocationUrl::new("https://oauth2.googleapis.com/revoke".into())
				.expect("invalid revoke url"),
		);

		Self {
			client: oauth_client,
			queue: Default::default(),
		}
	}

	#[must_use]
	pub fn get_url_and_future(&self) -> (Url, AuthProcess) {
		let (authorize_url, csrf_state) = self
			.client
			.authorize_url(CsrfToken::new_random)
			.add_scopes([
				Scope::new("https://www.googleapis.com/auth/userinfo.email".into()),
				Scope::new("https://www.googleapis.com/auth/userinfo.profile".into()),
				// Scope::new("https://www.googleapis.com/auth/classroom.courses.readonly".into()),
			])
			.url();

		(
			authorize_url,
			AuthProcess::new(
				Duration::from_secs(60 * 5),
				Arc::clone(&self.queue),
				csrf_state.secret().clone(),
			),
		)
	}
}

pub struct AuthProcess {
	wait_until: Instant,
	queue: Arc<RwLock<HashMap<String, Option<RefreshToken>>>>,
	csrf_state: String,
}

impl AuthProcess {
	#[must_use]
	fn new(
		wait: Duration,
		queue: Arc<RwLock<HashMap<String, Option<RefreshToken>>>>,
		csrf_state: String,
	) -> Self {
		let queue2 = queue.clone();
		let mut map = queue2.write().unwrap();
		map.insert(csrf_state.clone(), None);

		Self {
			wait_until: Instant::now() + wait,
			queue,
			csrf_state,
		}
	}
}

impl Future for AuthProcess {
	type Output = Option<RefreshToken>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut queue = self.queue.write().unwrap();

		if Instant::now() > self.wait_until {
			Poll::Ready(None)
		} else if queue.get(&self.csrf_state).unwrap().is_some() {
			let value = queue.remove(&self.csrf_state).unwrap().unwrap();

			Poll::Ready(Some(value))
		} else {
			cx.waker().clone().wake();

			Poll::Pending
		}
	}
}
