use super::*;

#[test]
fn locale_negotiation_uses_language_fallback_and_ignores_invalid_tags()
-> Result<(), LocalizationError> {
    assert_eq!(Localizer::new(&["ru-RU"])?.locale(), "ru");
    assert_eq!(Localizer::new(&["de-DE", "ru-KZ"])?.locale(), "ru");
    assert_eq!(
        Localizer::new(&["unsupported-lang", "\0", ""])?.locale(),
        "en-US"
    );
    assert_eq!(Localizer::new(&[])?.locale(), "en-US");
    assert!(!Localizer::new(&["ru"])?.right_to_left());
    Ok(())
}

#[test]
fn russian_plural_rules_cover_singular_few_many_and_teens() -> Result<(), LocalizationError> {
    let localizer = Localizer::new(&["ru"])?;
    for (count, expected) in [
        (0, "действий"),
        (1, "действие."),
        (2, "действия"),
        (5, "действий"),
        (11, "действий"),
        (21, "действие."),
    ] {
        let text = localizer.format(FormattedText::ActivationCount(count));
        assert!(text.contains(expected), "{count}: {text}");
        assert!(text.contains(&count.to_string()), "{count}: {text}");
    }
    Ok(())
}

#[test]
fn missing_or_broken_translation_falls_back_without_losing_english() -> Result<(), LocalizationError>
{
    let mut resources = LOCALES;
    resources[1].source = "sandbox-heading = Перевод\n";
    let localizer = Localizer::from_resources(&["ru"], &resources)?;
    assert_eq!(localizer.text(Text::SandboxHeading), "Перевод");
    assert_eq!(localizer.text(Text::ThemeLight), "Light");
    resources[1].source = "malformed message\n";
    let localizer = Localizer::from_resources(&["ru"], &resources)?;
    assert_eq!(localizer.locale(), "en-US");
    assert_eq!(localizer.text(Text::ThemeLight), "Light");
    resources[0].source = "broken english\n";
    assert!(Localizer::from_resources(&["ru"], &resources).is_err());
    Ok(())
}

#[test]
fn every_shipped_message_formats_with_its_typed_arguments() -> Result<(), LocalizationError> {
    for descriptor in LOCALES {
        let bundle = bundle(&descriptor)?;
        for message in Text::ALL {
            assert!(
                render(&bundle, message.id(), None).is_some(),
                "{} {}",
                descriptor.tag,
                message.id()
            );
        }
        for message in [
            FormattedText::SandboxTitle { version: "spike-1" },
            FormattedText::SandboxFooter {
                version: "spike-1",
                locale: descriptor.tag,
            },
            FormattedText::ActivationCount(2),
            FormattedText::RecordingTime {
                state: "Recording",
                time: "01:23",
            },
        ] {
            let (id, args) = message.parts();
            assert!(
                render(&bundle, id, Some(&args)).is_some(),
                "{} {id}",
                descriptor.tag
            );
        }
        assert_ne!(descriptor.native_name, "");
    }
    Ok(())
}

#[test]
fn settings_values_keep_custom_numbers_and_localize_units_and_plural_forms()
-> Result<(), LocalizationError> {
    use super::ValueUnit;
    for descriptor in LOCALES {
        let copy = Localizer::new(&[descriptor.tag])?;
        for unit in [
            ValueUnit::Mbps,
            ValueUnit::Kbps,
            ValueUnit::Fps,
            ValueUnit::Hours,
            ValueUnit::Minutes,
            ValueUnit::Seconds,
        ] {
            let message = FormattedText::SettingsValue { value: 40, unit };
            let (id, args) = message.parts();
            assert!(render(&bundle(&descriptor)?, id, Some(&args)).is_some());
            assert!(copy.format(message).contains("40"));
        }
        for is_default in [false, true] {
            for available in [false, true] {
                let message = FormattedText::MicrophoneName {
                    name: "Studio { $status }",
                    is_default,
                    available,
                };
                let (id, args) = message.parts();
                assert!(render(&bundle(&descriptor)?, id, Some(&args)).is_some());
                let label = copy.format(message);
                assert!(label.contains("Studio { $status }"));
            }
        }
        for message in [
            FormattedText::UnavailableChoice { label: "Studio" },
            FormattedText::ShortcutConflict { action: "Capture" },
        ] {
            let (id, args) = message.parts();
            assert!(render(&bundle(&descriptor)?, id, Some(&args)).is_some());
        }
    }
    let russian = Localizer::new(&["ru"])?;
    for (value, word) in [(21, "час"), (22, "часа"), (25, "часов")] {
        assert!(
            russian
                .format(FormattedText::SettingsValue {
                    value,
                    unit: ValueUnit::Hours
                })
                .ends_with(word)
        );
    }
    Ok(())
}

#[test]
fn native_captions_follow_the_selected_app_locale() -> Result<(), LocalizationError> {
    let english = Localizer::new(&["en-US"])?;
    let russian = Localizer::new(&["ru"])?;
    assert_eq!(english.window_title(true), "Sniptale Settings");
    assert_eq!(english.window_title(false), "Sniptale Capture");
    assert_eq!(russian.window_title(true), "Настройки Sniptale");
    assert_eq!(russian.window_title(false), "Захват Sniptale");
    assert_eq!(
        Localizer::new(&["fr"])?.window_title(true),
        english.window_title(true)
    );
    Ok(())
}
