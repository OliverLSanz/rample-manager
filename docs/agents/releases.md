# Releases, the website and messages to users

Read this before publishing a version, changing the website or sending users a
message. Oliver decides when a version ships: never tag a release unless he
asks for one.

## How users get Rample Manager

- Installers live on the public GitHub Releases of `OliverLSanz/rample-manager`
  under fixed names, so `releases/latest/download/<name>` always serves the
  newest one:
  - `Rample-Manager-macOS.dmg`: universal (Apple Silicon and Intel), macOS 14+,
    signed ad hoc, not notarized (first launch needs **Open Anyway**).
  - `Rample-Manager-Windows-x64-setup.exe`: NSIS, per user, unsigned.
- The website https://oliverlsanz.com/RampleManager/ (source in `site/`) links
  those files and reads the latest version from the GitHub API, so a release
  needs no website change.
- There is no automatic updater. Users install the new version over the old
  one; the Library, Patches and settings stay.
- From 3.1.0 on, release builds tell users about a newer Release with a quiet
  startup card (below). Users of 3.0.0 do not get that card.

## Publishing a version

1. Work happens on branches and `main` as usual. Nothing reaches users until a
   tag is pushed: builds of `main` only keep workflow artifacts.
2. Choose the version (semantic): patch for fixes only (`3.1.1`), minor for
   new features (`3.2.0`), major for breaking changes to the Library or the
   workflow.
3. Set it in every place, in one commit with the notes:
   `VERSION`, `package.json`, `package-lock.json` (the two top-level
   `version` fields), `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`,
   `crates/rample-core/Cargo.toml`, and the two workspace entries in
   `Cargo.lock` (`rample-core`, `rample-manager`). Check with
   `cargo metadata --locked --format-version 1 >/dev/null`.
4. Write `docs/releases/v<version>.md`, in English, for users:
   - Start with **short bullet points** (one line each, under ~140
     characters). The app's update card shows the first five bullets of this
     file as plain text, so they must make sense alone and come before any
     other list. Wrapped bullets are joined; longer ones are cut at a sentence.
   - Then anything else worth saying (upgrade notes, first-launch help).
5. Run the checks from `AGENTS.md` (build, tests, clippy) and `cargo fmt
   --all -- --check`, which CI also runs.
6. Commit, push `main`, then tag and push the tag:
   `git tag -a v<version> -m "Rample Manager <version>"` and
   `git push origin v<version>`.
7. The **Desktop builds** workflow (`.github/workflows/desktop.yml`) tests and
   builds both platforms, smoke-tests the Windows installer, then the
   `release` job checks that the tag matches `VERSION`, requires the notes
   file and publishes the GitHub Release with both installers. It takes about
   20–30 minutes. Watch it with
   `gh run watch <id> --repo OliverLSanz/rample-manager --exit-status`.
8. Verify: `gh release view v<version>` lists both files; both
   `https://github.com/OliverLSanz/rample-manager/releases/latest/download/<name>`
   URLs return 200; `api.github.com/repos/OliverLSanz/rample-manager/releases/latest`
   reports the new tag; the website shows "Version <version>".
9. Install it on Oliver's Mac with `./scripts/install.sh` (authorized).

If the workflow fails before the `release` job, fix the cause on `main`, delete
the tag locally and remotely (`git push origin :refs/tags/v<version>`), and tag
again. Never edit a published Release's files by hand.

## The startup notice (version card and messages)

Code: `frontend/notices.ts` (pure logic, fetching), `frontend/NoticeCard.tsx`
(the card), `open_link` in `src-tauri/src/main.rs` (https only). Tests:
`frontend/notices.test.ts`, `frontend/NoticeCard.test.tsx`.

- Only release builds ask (`import.meta.env.DEV` skips it), four seconds after
  startup, with a five-second timeout per request. Any failure (offline, error
  status, invalid JSON, timeout) shows nothing and logs nothing to the user.
- Sources, both public: the latest GitHub Release
  (`api.github.com/.../releases/latest`, shown when newer than the running
  app) and `https://oliverlsanz.com/RampleManager/messages.json`. The CSP
  `connect-src` in `tauri.conf.json` allows exactly these two origins; keep it
  that narrow.
- At most one card per launch: the version first, then messages in file order.
  It never takes focus. Closing it, **Skip this version** or following its
  link stores its id in `localStorage` (`rample-manager.dismissed-notices`)
  and it never appears again on that computer. An ignored card returns on the
  next launch.
- No tracking: no identifiers, analytics or per-user data. Keep it that way.

### Sending a message

Edit `site/messages.json` and publish it with `./scripts/deploy-site.sh`:

```json
{
  "messages": [
    {
      "id": "2026-12-video-tutorial",
      "title": "New video tutorial",
      "body": "Build a Kit in five minutes.",
      "url": "https://oliverlsanz.com/RampleManager/manual/",
      "linkLabel": "Watch",
      "minVersion": "3.1.0",
      "maxVersion": "3.9.9",
      "until": "2027-01-31"
    }
  ]
}
```

- `id` and `title` are required. The `id` must be new for each message:
  reusing one makes it invisible to everyone who dismissed the old message.
- `body`, `url` (https only; others are dropped), `linkLabel` (default "Read
  more"), `minVersion`, `maxVersion` and `until` (`YYYY-MM-DD`, last day
  shown) are optional.
- Keep messages rare and short. Remove expired ones when convenient.
- To test without reaching real users, publish a message with a `maxVersion`
  below every released version and run a local build whose `package.json`
  version is that low (restore it afterwards); remove the message when done.

## The website

`site/` is static (HTML, CSS, `assets/site.js`); preview with
`python3 -m http.server 8765 --directory site`, publish with
`./scripts/deploy-site.sh` (rsync to `~/vps/html/RampleManager` on
`oliverlsanz.com`, SSH port and key in `~/.ssh/config`). The VPS Nginx
config lives in `~/vps/nginx.conf` (repository on the server, with other
people's uncommitted work: commit only your own hunks). The container
bind-mounts that single file, so edit it in place (not `sed -i`, which
replaces the inode), then `docker compose exec -T reverse-proxy nginx -t` and
`nginx -s reload`. Screenshots in `site/assets/screens/` are WebP captures of
the real app on a demo Library, never Oliver's own Library.
