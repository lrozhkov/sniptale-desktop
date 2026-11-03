# Shared Desktop UI

This frontend supplies the application components and their browser sandbox. The Tauri client connects these components to the resident Rust owners. Sandbox fixtures do not capture the screen or write media.

## Run the sandbox

Use the Node version in the repository tool catalog. Install the locked dependencies once:

```bash
tooling/qa/installers/install-script-tools.sh
source tooling/qa/runtime/script-tools-env.sh
script_tools_env "$PWD" --require-node
node ui/node_modules/playwright-core/cli.js install chromium
```

Start the browser sandbox:

```bash
npm --prefix ui run sandbox
```

Open [the sandbox](http://127.0.0.1:5173/sandbox). Styling changes use Vite hot reload. They do not compile Rust or build an installer.

## Check the frontend

```bash
npm --prefix ui test
npm --prefix ui run build
npm --prefix ui run test:browser
```

Canonical checkpoint runs the frontend build, contract tests, and browser matrix. CI uses the same commands. The browser check uses the Chromium revision selected by the locked Playwright dependency. Reuse the existing browser cache when available. It writes screenshots and results under `.tmp/qa/ui-browser/`.

The shared components live in `src/components/`. `src/client/` owns the typed bridge and response validation. Only its Tauri transport imports the desktop API. `src/sandbox/` supplies controlled responses. Rust retains product state and command validation.

## Design assets

`src/styles/tokens.css` projects the Sniptale web semantic palette and toolbar conventions. Components use these roles. The bundled Golos UI font comes from Sniptale web's UI font owner. Its SHA-256 is `ec7584959184bcbd5b6cf65c5aacc2ad3ca3afc42ec3b0976f0260c33d1900a1`. Its [upstream license](src/styles/fonts/golos-ui/LICENSE.txt) is copied without modification.

The shared Fluent catalogs remain in `../resources/locales/`. The application and sandbox use the same messages. Expanded-text and RTL modes are layout stress tests. They do not establish support for another production language.

[ADR 0008](../docs/architecture/adr/0008-tauri-presentation-boundary.md) owns migration, lifecycle, and system-WebView requirements. The [delivery plan](../docs/design/delivery-plan.md) records unfinished work.
