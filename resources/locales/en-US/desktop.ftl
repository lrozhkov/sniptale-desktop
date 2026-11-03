# Shared desktop UI messages. Keep identifiers and variable names unchanged.
# User values, file paths, protocol identifiers, and machine output are not translations.

-brand-name = Sniptale
sandbox-title = { -brand-name } — Native design sandbox · { $version }
sandbox-heading = { -brand-name } / Components
sandbox-description = Native specimen · in-memory fixtures · no capture or settings writes
sandbox-theme-label = Theme
sandbox-language-label = Language
theme-light = Light
theme-dark = Dark
theme-contrast = System contrast colors
theme-change-failed = The theme could not change. Your previous theme is still active. Try again.
language-system = System language
navigation-capture = Capture
navigation-video = Video
command-capture-fixture = Capture (fixture)
command-secondary = Menu fixture…
command-reset-counter = Reset counter
commands-heading = Commands · neutral hierarchy
values-heading = Draft values
field-include-cursor = Include cursor
field-pending-fixture = Simulate pending action
field-save-folder-fixture = Save folder (synthetic value)
field-hotkey-specimen = Shortcut · Escape cancels; Delete clears
shortcut-press = Press shortcut…
shortcut-accepted = Shortcut saved in memory. No global shortcut was registered.
shortcut-cancelled = Shortcut entry cancelled. Your previous value is unchanged.
shortcut-cleared = Shortcut cleared in this fixture.
shortcut-unset = Set shortcut…
shortcut-invalid = Add Ctrl, Shift, Alt, or Win to a letter or number.
shortcut-conflict = Ctrl+Shift+V is reserved in this fixture. Choose another shortcut.
feedback-ready = Ready. Try Tab, Space, press/drag/release, and theme changes.
feedback-counter-reset = Ready. Counter reset.
feedback-capture-selected = Capture page selected.
feedback-video-selected = Video page selected.
feedback-activation-count =
    { $count ->
        [one] Activated once. The fixture did not capture media.
       *[other] Activated { $count } times. The fixture did not capture media.
    }
sandbox-footer = { $version } · { $locale } · native component candidate · docs/design/foundations.md

menu-mode = Mode
menu-local = Local capture
menu-browser = Browser destination (fixture)
menu-unavailable = Unavailable in this fixture
menu-selected = Menu action selected. No product command was sent.
menu-cancelled = Menu dismissed. No product command was sent.

panel-all-screens = All screens
panel-busy = A capture is in progress. Please wait.
panel-capture = Take screenshot
panel-discard = Discard recording
panel-finalizing = Saving recording…
panel-menu = More options
panel-pause = Pause
panel-paused = Recording paused
panel-preparing = Preparing recording…
panel-ready = Ready to capture
panel-record = Start recording
panel-recording = Recording
panel-region = Region
panel-resume = Resume
panel-screen = Current screen
panel-screenshot = Screenshot
panel-select-region = Select a region after you start.
panel-select-window = Captures the window you used before opening this panel.
panel-settings = Settings
panel-stop = Stop and save
panel-target-label = Capture area
panel-unavailable = Capture is unavailable. Open settings to check your save location.
panel-video = Video
panel-window = Active window

sandbox-scene-label = Preview
sandbox-scene-components = Components
sandbox-scene-capture-panel = Capture panel

panel-screenshot-done = Screenshot complete (preview).
panel-saved = Recording saved (preview).
panel-discarded = Recording discarded (preview).

choice-unavailable = { $label } (unavailable)
encoder-prefer = Prefer hardware
encoder-software = Software
feedback-appearance-failed = The appearance preference could not be saved. Your previous preference is still active.
feedback-audio-unavailable = This audio source is unavailable. Choose another source.
feedback-hotkey-unavailable = This shortcut is unavailable. Choose another combination.
feedback-microphone-unavailable = The selected microphone is unavailable. Choose another device or refresh the list.
feedback-recording-busy = Stop the recording before applying these changes.
feedback-save-failed = Your changes could not be saved. Keep editing, then try Apply again.
feedback-shortcut-conflict = Choose another shortcut. “{ $action }” already uses this combination.
microphone-name = { $status ->
    [default] { $name } (default)
    [unavailable] { $name } (unavailable)
    [default-unavailable] { $name } (default, unavailable)
   *[available] { $name }
    }
option-automatic = Automatic
quality-balanced = Balanced
quality-high = High
quality-low = Low
quality-ultra = Maximum
settings-apply = Apply
settings-apply-extension = Switch to extension mode
settings-apply-manual = Save and enable local capture
settings-ask-destination = Ask where to save after each capture
settings-audio = Audio
settings-audio-description = Choose which sounds to record.
settings-audio-microphone = Microphone
settings-audio-mixed = System audio and microphone
settings-audio-quality = Audio bitrate
settings-audio-source = Source
settings-audio-system = System audio
settings-cancel = Cancel
settings-capture = Capture
settings-change-folder = Change folder…
settings-choose-local-folder = Choose local folder…
settings-delivery = Result destination
settings-description = Capture, recording and shortcuts
settings-device-default = System default
settings-device-missing = Previously selected microphone (unavailable)
settings-dirty-description = Your changes have not been applied. Keep editing, or discard them.
settings-discard = Discard changes
settings-duration = Maximum duration
settings-encoder = Encoder
settings-extension-description = New capture and recording requests will use the connected browser's settings.
settings-extension-note = Local shortcuts are disabled in extension mode.
settings-extension-title = Use extension mode?
settings-frame-rate = Frame rate
settings-heading = Settings
settings-keep-editing = Keep editing
settings-local-description = Save captures locally, even when the browser is closed.
settings-local-folder = Local folder
settings-microphone = Microphone
settings-pointer-highlights = Collect cursor and click highlights
settings-quality = Quality
settings-refresh-browsers = Refresh browsers
settings-refresh-devices = Refresh devices
settings-save-delivery = Save destination choice
settings-save-location = Save location
settings-screenshot-cursor = Include pointer in screenshots
settings-screenshots = Screenshots
settings-setup-description = Choose a local folder before your first capture. You can also send results to a trusted browser.
settings-setup-note = Your local folder keeps results available when the browser is closed.
settings-setup-title = Choose where to save
settings-shortcuts = Shortcuts
settings-shortcuts-description = Select a field, then press a shortcut. Press Esc to cancel.
settings-system-theme = System
settings-theme = Theme
settings-use-extension = Use extension mode
settings-video = Video
settings-video-bitrate = Video bitrate
settings-video-cursor = Include pointer in video
settings-video-description = Choose recording quality and duration.
shortcut-capturing = Press a shortcut…
shortcut-not-set = Not set
shortcut-pause-resume = Pause or resume recording
shortcut-record-region = Record region
shortcut-record-screen = Record screen
shortcut-record-window = Record active window
shortcut-region = Capture region
shortcut-screen = Capture screen
shortcut-stop = Stop recording
shortcut-window = Capture active window
value-fps = { $value } fps
value-hours = { $count ->
    [one] { $count } hour
   *[other] { $count } hours
    }
value-kbps = { $value } kbps
value-mbps = { $value } Mbps
value-minutes = { $count ->
    [one] { $count } minute
   *[other] { $count } minutes
    }
value-seconds = { $count ->
    [one] { $count } second
   *[other] { $count } seconds
    }

settings-choose-folder = Choose a folder

shortcut-needs-modifier = Add a modifier

settings-title = Sniptale — Settings

panel-title = Sniptale capture
panel-needs-destination = Choose a save folder in Settings before capture.
panel-setup = Set up capture

# $state is localized recording status. $time is active recording time in mm:ss.
panel-recording-time = { $state } · { $time }

panel-no-window = Activate the window you want to capture, then reopen this panel.

# Shared web presentation. The browser sandbox and Tauri use these messages.
web-theme = Appearance
web-system = System
web-language = Language
web-capture-toolbar = Capture toolbar
web-capture-kind = Capture type
web-move-panel = Move capture toolbar
web-hide-panel = Hide capture toolbar
web-not-supported = Capture is not supported by this backend. No capture will start
web-connection-failed = Cannot reach the desktop agent. Reopen the panel to reconnect
web-phase-selecting = Select an area…
web-phase-starting = Starting…
web-phase-saving = Saving…
web-paused = Paused
web-stop = Stop
web-outcome-saved = Capture saved
web-outcome-discarded = Recording discarded
web-outcome-permission-denied = Capture permission was denied. Allow screen capture in system settings, then try again
web-outcome-save-failed = Capture could not be saved. Check the destination and available space, then try again
web-discard-title = Discard this recording?
web-discard-body = This recording will not be saved. You can continue recording instead.
web-design-system = Design system
web-sandbox = Component sandbox
web-sandbox-note = Simulated backend. No screen capture or file writes.
web-toolbar-title = Capture toolbar
web-toolbar-description = Choose a type and source. Start capture from one floating toolbar.
web-scenario = Scenario
web-scenario-success = Successful capture
web-scenario-permission-denied = Permission denied
web-scenario-save-failed = Save failure
web-scenario-unsupported = Unsupported backend
web-reset = Reset preview
web-open-panel = Show toolbar
web-preview = Preview
web-preview-description = The application imports this toolbar and its components.
web-contract-title = One interface, two clients.
web-contract-description = This preview uses controlled fixtures. The desktop uses a typed Tauri client. Rust owns capture, recording, and saved files.
web-desktop-required = Open this interface in the desktop application, or use /sandbox for simulated components.

web-text-preview = Text preview
web-text-normal = Normal text
web-text-expanded = Expanded text
web-text-rtl = RTL layout (test)
web-text-expanded-rtl = Expanded RTL (test)
web-text-scale = Panel scale

web-scenario-pending = Pending capture

web-switch-video = Switch to video
web-switch-screenshot = Switch to screenshot
web-capture-options = Capture options
web-recording-quality = Video quality
web-quality-next-recording = Applies to the next recording
web-app-settings = Application settings
web-app-settings-description = Appearance and language for the whole application
web-preferences-failed = Settings could not be saved. Try again

web-outcome-capture-failed = Capture failed. Try again
web-outcome-cancelled = Capture cancelled

web-general = General
web-general-description = Appearance and language apply immediately
web-settings-sections = Settings sections
web-bitrate-automatic = Leave empty to use automatic quality settings
web-seconds-short = sec
web-pointer-highlights-detail = Optional pointer movement and click events
web-settings-unavailable = Settings are unavailable · your draft is kept
web-settings-saving = Applying changes…
web-settings-unsaved = Unsaved changes
web-settings-up-to-date = Changes applied
web-settings-preview = Settings preview · simulated data
web-preview-microphone = Preview microphone
web-settings-scenario-ready = Ready
web-settings-scenario-save-failed = Save failure
web-settings-scenario-pending = Applying
web-settings-scenario-unavailable = Unavailable

web-settings-stale = Settings changed elsewhere. Reopen settings before applying your changes
web-settings-invalid = Check the folder, numeric values, and shortcuts, then try Apply again
web-settings-load-failed = Settings could not be loaded
web-settings-loading = Loading settings…
web-retry = Retry

web-delivery-description = Choose a registered browser destination for local capture results
web-delivery-empty = Connect the Sniptale extension to add a browser destination
web-delivery-missing = Destination no longer available
web-preview-browser = Preview browser — Personal

web-setup-required = Choose a save folder in Application settings before capture
web-command-busy = Another capture action is running. Try again when it finishes
web-command-rejected = The capture state changed. Check the current controls and try again
web-command-unavailable = This capture action is unavailable. Try again
web-capture-unavailable = Capture is temporarily unavailable
web-scenario-setup-required = Save folder required
web-scenario-command-refused = Command refused
web-scenario-connection-failed = Connection failure

web-window-action-failed = Window action unavailable

web-window-unavailable = Activate the window you want to capture, then return to this panel
web-command-source-unavailable = No capture window is available. Activate the target window and try again
web-scenario-window-lost = Window became unavailable

native-settings-title = Sniptale Settings
native-capture-title = Sniptale Capture
