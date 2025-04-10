//! Fluent Project translation system

use crate::states::{ApplicationContext, Command, Context, MessageComponentContext};
use anyhow::anyhow;
use fluent::{bundle, FluentArgs, FluentMessage, FluentResource};
use fluent_syntax::ast::Pattern;
use intl_memoizer::concurrent::IntlLangMemoizer as ConcurrentIntlLangMemoizer;
use std::{
	borrow::Cow,
	collections::HashMap,
	fmt::{Debug, Formatter},
	fs::{read_dir, read_to_string},
	path::Path,
};
use unic_langid::LanguageIdentifier;

/// The concurrent Fluent bundle used to cache the language results
type FluentBundle = bundle::FluentBundle<FluentResource, ConcurrentIntlLangMemoizer>;

/// Manages the client internationalization
pub(crate) struct Translations {
	/// The fallback locale
	fallback: LanguageIdentifier,
	/// The available locales
	bundles: HashMap<LanguageIdentifier, FluentBundle>,
}

impl Debug for Translations {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Translations")
			.field("fallback", &self.fallback)
			.field("bundles", &self.bundles.keys())
			.finish()
	}
}

/// Reads and parses the given Fluent file
fn read_fluent_file(path: &Path) -> anyhow::Result<(LanguageIdentifier, FluentBundle)> {
	// Extract locale from filename
	let locale: LanguageIdentifier = path
		.file_stem()
		.ok_or_else(|| anyhow!("Invalid `.ftl` file"))?
		.to_str()
		.ok_or_else(|| anyhow!("Invalid UTF-8 filename"))?
		.parse()?;

	// Load .ftl resource
	let file_contents = read_to_string(path)?;
	let resource = FluentResource::try_new(file_contents)
		.map_err(|(_, e)| anyhow!("failed to parse {:?}: {:?}", path, e))?;

	// Associate .ftl resource with locale and bundle it
	let mut bundle = FluentBundle::new_concurrent(vec![locale.clone()]);
	bundle
		.add_resource(resource)
		.map_err(|e| anyhow!("failed to add resource to bundle: {:?}", e))?;

	Ok((locale, bundle))
}

impl Translations {
	/// Load all available translations from the given directory
	pub(crate) fn from_folder(folder: &str, fallback: LanguageIdentifier) -> anyhow::Result<Self> {
		let bundles: HashMap<LanguageIdentifier, FluentBundle> = read_dir(folder)?
			.map(Result::unwrap)
			.filter(|file| matches!(file.path().extension(), Some(ext) if ext == "ftl"))
			.map(|file| read_fluent_file(&file.path()))
			.collect::<Result<_, _>>()?;

		if !bundles.contains_key(&fallback) {
			return Err(anyhow!("fallback locale bundle not found"));
		}

		Ok(Self { fallback, bundles })
	}

	/// Formats the given message with the given arguments
	fn format<'bundle>(
		bundle: &'bundle FluentBundle,
		pattern: &'bundle Pattern<&str>,
		args: Option<&'bundle FluentArgs>,
	) -> Cow<'bundle, str> {
		let mut errors = Vec::new();

		let formatted = bundle.format_pattern(pattern, args, &mut errors);

		for error in errors {
			tracing::error!("fluent format pattern error {}", error);
		}

		formatted
	}

	/// Get a translation from the given key or an error
	pub(crate) fn translate_checked<'bundle>(
		&'bundle self,
		locale: &LanguageIdentifier,
		key: &'bundle str,
		args: Option<&'bundle FluentArgs>,
	) -> anyhow::Result<Cow<'bundle, str>> {
		let bundle = self.bundles.get(locale).unwrap_or_else(|| {
			self.bundles
				.get(&self.fallback)
				.expect("failed to load fallback locale bundle")
		});

		bundle.get_message(key).map_or_else(
			|| Err(anyhow!("unknown fluent key `{}`", key)),
			|message| {
				message.value().map_or_else(
					|| Err(anyhow!("message `{}` has no value", key)),
					|pattern| Ok(Self::format(bundle, pattern, args)),
				)
			},
		)
	}

	/// Apply translations to the given command tree
	pub(crate) fn apply_translations_to_interactions(
		&self,
		commands: &mut [Command],
		parent_name: Option<&String>,
	) {
		for command in &mut *commands {
			// Skip prefix commands
			if command.prefix_action.is_some() {
				continue;
			}

			self.apply_translations_to_interaction(command, parent_name.cloned());
		}
	}

	/// Apply translations to the given group or command
	pub(crate) fn apply_translations_to_interaction(
		&self,
		command: &mut Command,
		parent_name: Option<String>,
	) {
		let full_command_name = match parent_name {
			Some(parent_name) => format!("{}_{}", parent_name, command.name),
			None => command.name.clone(),
		};

		for (locale, bundle) in &self.bundles {
			let Some(command_translation) = bundle.get_message(&full_command_name) else {
				tracing::error!(
					"translation for command `{}` with locale `{}` does not exist",
					full_command_name,
					locale
				);

				continue;
			};

			match command_translation.value() {
				Some(name) => {
					command
						.name_localizations
						.insert(locale.to_string(), Self::format(bundle, name, None).into());
				}
				None => {
					tracing::error!(
						"translation for command `{}` with locale `{}` does not have a name",
						full_command_name,
						locale
					);
				}
			}

			// Skip subcommands groups
			if !command.subcommands.is_empty() {
				continue;
			}

			Self::apply_translations_to_slash_command(
				locale,
				bundle,
				&command_translation,
				command,
				&full_command_name,
			);
		}

		self.apply_translations_to_interactions(&mut command.subcommands, Some(&full_command_name));
	}

	/// Apply translations to the given slash command
	fn apply_translations_to_slash_command(
		locale: &LanguageIdentifier,
		bundle: &FluentBundle,
		command_translation: &FluentMessage,
		command: &mut Command,
		full_command_name: &String,
	) {
		let apply_attribute =
			|attribute: &str, hash_map: &mut HashMap<String, String>, description: &str| {
				command_translation.get_attribute(attribute).map_or_else(
					|| {
						tracing::error!(
							"translation for command `{}` with locale `{}` does not have a {}",
							full_command_name,
							locale,
							description
						);
					},
					|description| {
						hash_map.insert(
							locale.to_string(),
							Self::format(bundle, description.value(), None).into(),
						);
					},
				);
			};

		apply_attribute(
			"description",
			&mut command.description_localizations,
			"description",
		);

		for parameter in &mut command.parameters {
			apply_attribute(
				&parameter.name,
				&mut parameter.name_localizations,
				format!("name for the parameter `{}`", parameter.name).as_str(),
			);

			apply_attribute(
				&format!("{}-description", parameter.name),
				&mut parameter.description_localizations,
				&format!("description for the parameter `{}`", parameter.name),
			);

			for choice in &mut parameter.choices {
				apply_attribute(
					&format!("{}-choice", choice.name),
					&mut choice.localizations,
					&format!("translation for the choice `{}`", choice.name),
				);
			}
		}
	}
}

/// Trait for client internationalisation
pub(crate) trait Translate {
	/// Get the translation for the given message with a locale provided by self context
	fn translate_checked<'bundle>(
		&'bundle self,
		key: &'bundle str,
		args: Option<&'bundle FluentArgs>,
	) -> anyhow::Result<Cow<'bundle, str>>;

	/// Get a translated key of the key itself in case it is not found
	fn translate<'b>(&'b self, key: &'b str, args: Option<FluentArgs<'b>>) -> String {
		match self.translate_checked(key, args.as_ref()) {
			Ok(string) => string.into_owned(),
			Err(error) => {
				tracing::error!(key = key, args = ?args, error = ?error, "translation error");
				key.to_owned()
			}
		}
	}
}

impl Translate for ApplicationContext<'_> {
	fn translate_checked<'bundle>(
		&'bundle self,
		key: &'bundle str,
		args: Option<&'bundle FluentArgs>,
	) -> anyhow::Result<Cow<'bundle, str>> {
		let locale: LanguageIdentifier = self.interaction.locale.parse()?;

		self.data.translations.translate_checked(&locale, key, args)
	}
}

impl Translate for Context<'_> {
	fn translate_checked<'bundle>(
		&'bundle self,
		key: &'bundle str,
		args: Option<&'bundle FluentArgs>,
	) -> anyhow::Result<Cow<'bundle, str>> {
		let locale: LanguageIdentifier = match self.locale() {
			Some(locale) => locale.parse()?,
			None => self.data().translations.fallback.clone(),
		};

		self.data()
			.translations
			.translate_checked(&locale, key, args)
	}
}

impl Translate for MessageComponentContext<'_> {
	fn translate_checked<'bundle>(
		&'bundle self,
		key: &'bundle str,
		args: Option<&'bundle FluentArgs>,
	) -> anyhow::Result<Cow<'bundle, str>> {
		let locale: LanguageIdentifier = self.interaction.locale.parse()?;

		self.data.translations.translate_checked(&locale, key, args)
	}
}
