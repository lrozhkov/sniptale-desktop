use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserFamily {
    Chrome,
    Edge,
    Chromium,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryHive {
    CurrentUser,
    LocalMachine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeHostRegistryLocation {
    pub(crate) hive: RegistryHive,
    pub(crate) browser: BrowserFamily,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeHostRegistryEntry {
    pub(crate) location: NativeHostRegistryLocation,
    pub(crate) manifest_path: PathBuf,
}

pub(crate) fn native_host_registry_subkey(browser: BrowserFamily, host_name: &str) -> String {
    format!(
        r"{}\{}",
        browser_native_messaging_hosts_subkey(browser),
        host_name
    )
}

pub(super) const fn native_host_registry_locations() -> [NativeHostRegistryLocation; 6] {
    [
        NativeHostRegistryLocation {
            hive: RegistryHive::CurrentUser,
            browser: BrowserFamily::Chrome,
        },
        NativeHostRegistryLocation {
            hive: RegistryHive::LocalMachine,
            browser: BrowserFamily::Chrome,
        },
        NativeHostRegistryLocation {
            hive: RegistryHive::CurrentUser,
            browser: BrowserFamily::Edge,
        },
        NativeHostRegistryLocation {
            hive: RegistryHive::LocalMachine,
            browser: BrowserFamily::Edge,
        },
        NativeHostRegistryLocation {
            hive: RegistryHive::CurrentUser,
            browser: BrowserFamily::Chromium,
        },
        NativeHostRegistryLocation {
            hive: RegistryHive::LocalMachine,
            browser: BrowserFamily::Chromium,
        },
    ]
}

const fn browser_native_messaging_hosts_subkey(browser: BrowserFamily) -> &'static str {
    match browser {
        BrowserFamily::Chrome => r"Software\Google\Chrome\NativeMessagingHosts",
        BrowserFamily::Edge => r"Software\Microsoft\Edge\NativeMessagingHosts",
        BrowserFamily::Chromium => r"Software\Chromium\NativeMessagingHosts",
    }
}
