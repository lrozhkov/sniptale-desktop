//! Fluent UI messages. Localized output never becomes a protocol, path, or persisted value.

use std::fmt;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use unic_langid::LanguageIdentifier;

#[cfg(test)]
#[path = "localization/messages.rs"]
mod messages;
#[cfg(test)]
use messages::{FormattedText, Text, ValueUnit};

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocaleResource {
    pub(crate) tag: &'static str,
    #[cfg(test)]
    pub(crate) native_name: &'static str,
    #[cfg(test)]
    pub(crate) right_to_left: bool,
    source: &'static str,
}

pub(crate) const LOCALES: [LocaleResource; 2] = [
    LocaleResource {
        tag: "en-US",
        #[cfg(test)]
        native_name: "English",
        #[cfg(test)]
        right_to_left: false,
        source: include_str!("../../resources/locales/en-US/desktop.ftl"),
    },
    LocaleResource {
        tag: "ru",
        #[cfg(test)]
        native_name: "Русский",
        #[cfg(test)]
        right_to_left: false,
        source: include_str!("../../resources/locales/ru/desktop.ftl"),
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalizationError {
    InvalidCatalog,
}

pub(crate) struct Localizer {
    locale: &'static str,
    #[cfg(test)]
    right_to_left: bool,
    primary: Option<FluentBundle<FluentResource>>,
    fallback: FluentBundle<FluentResource>,
}

impl fmt::Debug for Localizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Localizer")
            .field("locale", &self.locale)
            .finish_non_exhaustive()
    }
}

impl Localizer {
    pub(crate) fn new(requested: &[&str]) -> Result<Self, LocalizationError> {
        Self::from_resources(requested, &LOCALES)
    }

    fn from_resources(
        requested: &[&str],
        resources: &[LocaleResource],
    ) -> Result<Self, LocalizationError> {
        let default = resources.first().ok_or(LocalizationError::InvalidCatalog)?;
        let available = resources
            .iter()
            .map(|entry| entry.tag.parse::<LanguageIdentifier>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| LocalizationError::InvalidCatalog)?;
        let default_id = available.first().ok_or(LocalizationError::InvalidCatalog)?;
        let requested: Vec<_> = requested
            .iter()
            .filter(|tag| tag.len() <= 64)
            .filter_map(|tag| tag.parse::<LanguageIdentifier>().ok())
            .collect();
        let negotiated = negotiate_languages(
            &requested,
            &available,
            Some(default_id),
            NegotiationStrategy::Lookup,
        );
        let selected = negotiated
            .first()
            .ok_or(LocalizationError::InvalidCatalog)?;
        let index = available
            .iter()
            .position(|id| id == *selected)
            .ok_or(LocalizationError::InvalidCatalog)?;
        let descriptor = resources
            .get(index)
            .ok_or(LocalizationError::InvalidCatalog)?;
        let fallback = bundle(default)?;
        let primary = if index == 0 {
            None
        } else {
            bundle(descriptor).ok()
        };
        let resolved = if index != 0 && primary.is_none() {
            default
        } else {
            descriptor
        };
        Ok(Self {
            locale: resolved.tag,
            #[cfg(test)]
            right_to_left: resolved.right_to_left,
            primary,
            fallback,
        })
    }

    #[cfg(test)]
    pub(crate) const fn locale(&self) -> &'static str {
        self.locale
    }
    #[cfg(test)]
    pub(crate) const fn right_to_left(&self) -> bool {
        self.right_to_left
    }

    pub(crate) fn window_title(&self, settings: bool) -> String {
        self.lookup(
            if settings {
                "native-settings-title"
            } else {
                "native-capture-title"
            },
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn text(&self, text: Text) -> String {
        self.lookup(text.id(), None)
    }

    #[cfg(test)]
    pub(crate) fn format(&self, text: FormattedText<'_>) -> String {
        let (id, args) = text.parts();
        self.lookup(id, Some(&args))
    }

    fn lookup(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        self.primary.as_ref().and_then(|primary| render(primary, id, args))
            .or_else(|| render(&self.fallback, id, args))
            // Unreachable for validated English keys and typed argument variants.
            .unwrap_or_else(|| "—".to_owned())
    }
}

fn bundle(resource: &LocaleResource) -> Result<FluentBundle<FluentResource>, LocalizationError> {
    let locale = resource
        .tag
        .parse::<LanguageIdentifier>()
        .map_err(|_| LocalizationError::InvalidCatalog)?;
    let resource = FluentResource::try_new(resource.source.to_owned())
        .map_err(|_| LocalizationError::InvalidCatalog)?;
    let mut bundle = FluentBundle::new(vec![locale]);
    bundle
        .add_resource(resource)
        .map_err(|_| LocalizationError::InvalidCatalog)?;
    bundle.set_use_isolating(true);
    Ok(bundle)
}

fn render(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs<'_>>,
) -> Option<String> {
    let message = bundle.get_message(id)?;
    let pattern = message.value()?;
    let mut errors = Vec::new();
    let result = bundle
        .format_pattern(pattern, args, &mut errors)
        .into_owned();
    errors.is_empty().then_some(result)
}

#[cfg(test)]
#[path = "localization/tests.rs"]
mod tests;
