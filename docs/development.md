# Developing Rample Manager

Rample Manager runs on macOS and Windows with a shared React interface and a
native Rust audio engine, in a Tauri 2 shell. Everything works locally and
offline; no account or server is required.

How to build, test, package and release the app, and how its website and
startup notices work. Agents: [AGENTS.md](../AGENTS.md) has the rules;
[agents/releases.md](agents/releases.md) the full release checklist. App behavior is described in
[how-it-works.md](how-it-works.md).

## Develop and build

Requires Node 22+, stable Rust and platform build tools: Xcode command-line tools
on Mac, Microsoft C++ Build Tools and WebView2 on Windows.

```sh
npm ci
npm run tauri dev
```

Validation:

```sh
npm run build
npm test
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Package on the target operating system:

```sh
# macOS
npm run tauri -- build --bundles app,dmg
./scripts/install.sh
# Windows
npm run tauri -- build --bundles nsis
```

The installer builds before closing the current app, signs locally, replaces the
bundle and reopens it. `--no-launch` skips reopening; `RAMPLE_INSTALL_DIR` changes
the installation directory. Mac builds are signed ad hoc, not notarized for other
computers. Windows installers are currently unsigned.

## Architecture and validation

`frontend/` holds the React UI and IPC: `App.tsx` is the Patch main screen,
with `KitList`, `PatchMenu` and `VoiceCard`, and `patchView.ts` its pure view
model. `crates/rample-core/` holds audio decoding, Rample preparation, Sample
Edit DSP, native `_save` reading, layer selection and the sample-clock
renderer. `src-tauri/` integrates the Library, Patches, native dialogs and
devices. CPAL outputs through CoreAudio on Mac and WASAPI on Windows.

Triggers are rendered against the device sample clock, independently of browser
or UI timers. Each track retriggers monophonically at the correct frame. WAVs
stream with bounded heads and background readers; decoding and disk I/O stay off
the callback. Matched sample rates preserve PCM exactly; other rates use a
32-tap windowed-sinc resampler. Device reconnection is retried automatically.

Headless tests check actual PCM, trigger boundaries, modes, mute/solo, resampling,
long-file streaming, Library snapshots, SD Import, Build Patch and UI behavior on both
operating systems. Actual-device smoke testing is separate:

```sh
cargo run -p rample-core --example audio_smoke --features native-audio
```

The Windows CI build verifies compilation, tests, packaging, native app startup,
graceful close, and a reinstall that leaves the Library unchanged
(`scripts/smoke-windows.ps1`). Startup readiness uses the native window and Library
store. This
cannot replace a listening test on physical Windows audio hardware. The original Swift sources
are archived under `legacy/swift` for reference and rollback.

Keep UI text in English and commit/push each atomic functional change with its
relevant tests and docs; see [AGENTS.md](../AGENTS.md).

## Releases

Pushing a version tag (`v3.0.0`, matching `VERSION`) makes the **Desktop builds**
workflow test, build and publish both installers as a GitHub Release, with the
notes in `docs/releases/<tag>.md`. Release files keep the same names across
versions, so `releases/latest/download/<file>` always serves the newest one.
Every build of `main` also keeps both installers as workflow artifacts.
Updates on Oliver's Mac remain fully automated through the local install script.
Hosted automatic updates are not enabled; the installers can upgrade an existing
installation while keeping preferences and sample libraries.

## Startup notices

A few seconds after startup, release builds ask two public sources whether
there is something to tell the user, and show at most one small card in the
corner, which never takes focus:

- **A new version.** The latest GitHub Release (`api.github.com`), when it is
  newer than the running app. The card lists the first five bullet points of
  `docs/releases/<tag>.md`, so put short user-facing bullets first. It appears
  only once a Release is published, never for work on `main`.
- **News.** `site/messages.json`, published with the website. Each message
  needs an `id` and a `title`; `body`, an https `url` with `linkLabel`,
  `minVersion`, `maxVersion` and `until` (`YYYY-MM-DD`) are optional. Edit the
  file and run `./scripts/deploy-site.sh` to send one.

The version notice comes first, then the messages in file order. Closing a
card, **Skip this version** or following its link dismisses it for good on that
computer. Any failure (offline, a timeout of five seconds, an error response
or invalid data) shows nothing. Development builds never ask. The requests
carry no identifier; the servers only see the computer's IP address.

## Website

`site/` holds the landing page and the visual manual: static HTML, CSS and a
small script, with no build step. Screenshots in `site/assets/screens/` are
WebP captures of the real app on a demo Library; retake them when the screens
they show change. Preview locally with
`python3 -m http.server 8765 --directory site` and publish with
`./scripts/deploy-site.sh`, which copies `site/` to `~/vps/html/RampleManager`
on the VPS, where its Nginx serves `/RampleManager/`. Download buttons link
`releases/latest/download/<file>` and the page reads the version from the
GitHub API, so a new release needs no site change.
