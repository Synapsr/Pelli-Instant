# Contributing

Thanks for helping! Pelli Instant aims to stay quick and trustworthy: every
rejection gives its reason, and the photographer's own marks always win over
the AI's. Before adding a setting, ask whether a better default would do.

## Setup

- [rustup](https://rustup.rs): the first `cargo` command installs the Rust
  version pinned in `rust-toolchain.toml` (the same one CI and releases use;
  bump it on purpose, in its own pull request)
- [Node.js](https://nodejs.org) 22 and [pnpm](https://pnpm.io) 10
- macOS: Xcode's command line tools (`xcode-select --install`), on an
  **Apple Silicon** Mac. ONNX Runtime ships no prebuilt library for Intel
  Macs; to build one yourself, see
  [`crates/cull-ml/README.md`](../crates/cull-ml/README.md#packaging-onnx-runtime-tauri).
- Windows: the [Tauri prerequisites](https://tauri.app/start/prerequisites/#windows)
  (Microsoft C++ Build Tools, WebView2).
- Linux, for engine work and tests only (the app isn't supported there):

  ```sh
  sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev
  ```

```sh
pnpm install
pnpm models        # the Standard AI models into models/ (about 223 MB, SHA-256 checked)
pnpm tauri dev     # the desktop app, with the real engine
```

The first Rust build downloads a prebuilt ONNX Runtime (cached in
`~/.cache/ort.pyke.io`) and takes several minutes; later ones take seconds.
`pnpm models` is idempotent: files already there with the right checksum are
skipped. `pnpm models --gpu` adds the fp16 CLIP model that DirectML runs
(+176 MB), `pnpm models --maximum` the Maximum pack into `models/maximum/`
for the command line (+777 MB; the app downloads its own copy), and
`pnpm models --check` only verifies. Every flag is in
[docs/models.md](models.md#scriptsfetch-modelsmjs). Without the models, the
app still starts: sharpness and exposure are analysed, similar shots are
grouped with a perceptual hash, but faces, eyes and search wait for the
models.

## Without a Pelli account

```sh
PELLI_INSTANT_NO_ACCOUNT=1 pnpm tauri dev
PELLI_INSTANT_NO_ACCOUNT=1 pnpm tauri build
```

The variable is read when the app is compiled: the build it produces never
asks for a Pelli account and has no monthly quota. Setting it when you launch
an installed app does nothing. Without Pelli Instant's update key, a
`pnpm tauri build` also needs the option described in
[Building an installer yourself](#building-an-installer-yourself).

## Working on the interface alone

`pnpm dev`, then open <http://localhost:1420>: the interface runs in a
browser against a simulated engine (`src/lib/api/mock`), with procedurally
drawn photos. No Rust build needed. Query parameters pick the state:
`mockAuth=signedIn` (or `signedOut`), `mockPlan=free` (`lite`, `pro`,
`ultra`, `trial`), `mockUsed=950` (photos already sorted this month),
`mockCount=5000` (size of the shoot), `mockTrial=1` (a free account that can
still start the Pelli trial), `mockNoAccount=1` (a build without account),
`mockModels=missing`, `mockPack=ready` (`paused`, `failed`; absent by
default), `mockDisk=low` (not enough space for the Maximum pack),
`mockUpdate=available` (`offline`, `disabled`), `mockLang=en` (`fr`, `es`,
`de`, `it`, `pt`, `nl`, `pl`, `sv`), `mockTheme=light` (`dark`, `system`),
`mockSpeed=4` (a faster analysis). They are all read in
`src/lib/api/mock/index.ts`.

## The real engine in a browser

The `pelli-cull` command line serves the engine over HTTP for the interface
(handy for Playwright and for debugging the engine without the desktop
shell):

```sh
cargo run --release -p cull-cli -- serve --port 4317 &
pnpm dev:bridge    # the interface, talking to http://127.0.0.1:4317 (.env.bridge)
```

## Signing in without Pelli

`scripts/fake-pelli.mjs` is a fake Pelli with no dependencies: it imitates
the API the app calls (`/api/cull/*`) and the consent page you see in the
browser when you sign in. You pick the account's plan on that page, and
<http://localhost:3100> switches it live or resets everything.

```sh
pnpm fake-pelli        # terminal 1: http://localhost:3100
pnpm app:fake-pelli    # terminal 2: the app, signed in against it
```

Each plan has its own test account, and the fake's home page switches an
account's plan, sets or resets its monthly count. `FAKE_PELLI_PLAN` picks the
account signed in by `FAKE_PELLI_AUTO_APPROVE=1` (sign-in without the consent
page, for scripts), `FAKE_PELLI_UPLOAD_FAIL` answers a share of uploads with
errors, `FAKE_PELLI_STORAGE_GB` sets the storage limit, `FAKE_PELLI_PORT` and
`FAKE_PELLI_DIR` move it. The full list is at the top of the script.

The app talks to `https://app.pelli.io`. In a development build
(`pnpm tauri dev`), `PELLI_API_URL` points it elsewhere at run time; that is
what `pnpm app:fake-pelli` does (the POSIX way: on Windows, set the variable in
your shell first). A release build reads `PELLI_API_URL` only when it is
compiled, never at run time, so an installed app can't be pointed at another
server.

## Sorting a folder in a terminal

```sh
cargo run --release -p cull-cli -- analyze ~/Pictures/Shoot
cargo run --release -p cull-cli -- analyze ~/Pictures/Shoot --sensitivity strict --keep 2
```

Prints each photo's score, verdict, sharpness, exposure, group (`*` marks the
best of its group) and reasons, then the throughput. `--json` prints JSON.
The library and cache go to `./.pelli-cull` (`--data-dir` to change it), the
models are read from `./models` (`--models-dir`). This is the quickest way to
see why a photo was rejected and to tune a threshold.

`--precision maximum` analyses with the Maximum pack, read from
`./models/maximum` (`pnpm models --maximum`; `--maximum-dir` to change it):
the first run prepares it, and a pack that isn't ready falls back to
Standard. `--threads N` sets the number of workers, `--background` runs them
like the app's background mode. The command line needs no Pelli account and
never counts photos.

## The welcome tour's voice-over

The voice-over is generated with [ElevenLabs](https://elevenlabs.io) from
[`scripts/voice/narration.json`](../scripts/voice/narration.json) (one line
per step and language, the voice of each language, the model), and the
generated `.mp3` files are committed in `src/assets/onboarding/voice/`: the
app ships them and never calls ElevenLabs.

```sh
ELEVENLABS_API_KEY=… pnpm voices            # only the lines that changed
ELEVENLABS_API_KEY=… pnpm voices --force    # everything
ELEVENLABS_API_KEY=… pnpm voices fr de      # some languages
```

The key is read from the environment and never written anywhere. A manifest
next to the files records what each one was made from, so a line is
generated again only when its text, voice or model changes. Credit a new
voice in [`src/assets/onboarding/CREDITS.md`](../src/assets/onboarding/CREDITS.md).

## Test photos and calibration

Test photos are never committed. Two scripts fetch freely licensed ones into
`samples/` (ignored by git):

```sh
scripts/fetch-samples.sh              # CC0 RAW files from raw.pixls.us, one per brand
scripts/fetch-calibration-photos.sh   # 39 CC0 / CC BY / CC BY-SA photos
```

Tests that need the models or the samples print `SKIP` and pass when they are
absent (`PELLI_CULL_MODELS` and `PELLI_CULL_SAMPLES` point elsewhere).
Decisions are calibrated with
`cargo run --release -p cull-analysis --example calibrate`, which prints the
measures and verdicts of every photo and of generated variants (blur, motion,
noise, ±EV, crops). See [`crates/cull-analysis/README.md`](../crates/cull-analysis/README.md#calibration-examplescalibraters).

## Checks

CI runs these on every pull request; run them before pushing:

```sh
pnpm typecheck && pnpm test && pnpm build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`pnpm build` comes first: the desktop shell embeds the built interface, so
Clippy and the tests of `src-tauri` need `dist/`. CI also runs Clippy on
Windows with DirectML (`--features cull-ml/directml`), for the code that only
builds there.

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org):
  `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, with the crate or area as
  scope when it helps (`fix(decode): …`). Say why, not only what.
- **Interface strings** live in all 9 files of `src/lib/i18n/locales`
  (`fr.ts` is the reference: the types come from it); a test fails if a key
  or a `{placeholder}` is missing from one of them. Plurals take `one` and
  `other`, plus `few` and `many` where the language needs them (Polish). A
  new language is a new file there, plus `CATALOGUES`, `LANGUAGES`,
  `LANGUAGE_NAMES` and `LOCALES` in `src/lib/i18n/index.ts`, `Language` in
  `src/lib/contract.ts`, `LANGUAGES` in `crates/cull-engine/src/settings.rs`,
  the installer languages (`bundle.windows.nsis.languages` in
  `src-tauri/tauri.conf.json`), and its voice and lines in
  `scripts/voice/narration.json` (then `pnpm voices <language>`). If
  photographers search in it, add its words to the glossary of
  `crates/cull-engine/src/query.rs`.
- **The interface and the engine** share one contract,
  `src/lib/contract.ts`. A new command goes in `COMMANDS` there, in
  `dispatch` (`crates/cull-engine/src/commands.rs`), in `COMMANDS` in
  `src-tauri/src/lib.rs`, and in the simulated engine.
- **Design.** The interface uses [HeroUI](https://heroui.com) and
  Tailwind CSS 4. The theme is in `hero.ts`: neutral greys with no colour
  cast, because photos are judged on screen; dark by default, like Lightroom;
  the violet accent `#5840f0` for focus and the brand; calm green, amber and
  red for the verdicts. Shared components are in `src/components/ui`, tokens
  and animations in `src/styles/globals.css`. Plain words, and every problem
  comes with what to do about it.
- **Decisions.** Thresholds and weights live in one table in
  `crates/cull-analysis/src/score.rs`. A pull request that changes them says
  what `calibrate` showed before and after.
- **Models.** Every model is listed in `models/manifest.json` with its URL at
  a pinned revision, size, SHA-256 and licence. Never add weights under a
  non-commercial, GPL or AGPL licence. A new model also goes in
  `THIRD_PARTY_NOTICES.md` and [docs/models.md](models.md).
- **Rust** is formatted with the `rustfmt.toml` at the root (120 columns).

## Releasing

Bump the version in `package.json`, `src-tauri/tauri.conf.json` and the
workspace `Cargo.toml`, then push a `vX.Y.Z` tag. The
[release workflow](../.github/workflows/release.yml) builds the macOS Apple
Silicon installer and the Windows x64 installer (DirectML), both with the
Standard models inside (the fp16 CLIP model only in the Windows one), and publishes them in a GitHub Release with
stable names (`PelliInstant-macos-arm64.dmg`,
`PelliInstant-windows-x64-setup.exe`), a `.sha256` next to each, the signed
update bundles with `latest.json`, a `SHA256SUMS` file and a build
attestation. Running it by hand (`workflow_dispatch`) builds the installers
without publishing them.

A tag with a hyphen (`v0.2.0-beta.1`) is published as a prerelease: it never
becomes the latest release, so neither the download links nor the in-app
updater offer it.

Before announcing a release, try the DMG and the installer on real machines:
open the app, sign in, sort a folder of RAW files, export XMP and check that
Lightroom Classic reads it.

### Signing

Everything a release ships is signed, the same way as
[Parler](https://github.com/Synapsr/Parler) and with the same secret names.
A release tag of this repository refuses to build without these secrets; a
fork builds unsigned, without update bundles.

| Secret | What it holds |
| --- | --- |
| `APPLE_CERTIFICATE` | The **Developer ID Application** certificate, `.p12` encoded in base64 |
| `APPLE_CERTIFICATE_PASSWORD` | Its password |
| `APPLE_API_KEY` | App Store Connect API key ID (notarization) |
| `APPLE_API_ISSUER` | App Store Connect API issuer ID |
| `APPLE_API_KEY_B64` | That key's `.p8` file, encoded in base64 |
| `TAURI_SIGNING_PRIVATE_KEY` | The private key that signs updates (its public half is in `src-tauri/tauri.conf.json`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password |

Tauri signs and notarizes the app before packing the DMG and the update
bundle, then the workflow notarizes and staples the DMG. Check a build on
another Mac with Gatekeeper on:
`spctl -a -vv "/Applications/Pelli Instant.app"`.

The update signing key is Pelli Instant's own: keep it, and its password,
in a password manager. Losing it means installed copies can no longer be
updated.

The Windows installer isn't signed with an Authenticode certificate, so
SmartScreen may warn about it the first time. Its update bundle is signed
with the key above. It installs for the current user, without administrator
rights.

### Building an installer yourself

macOS:

```sh
pnpm models
pnpm tauri build --config src-tauri/tauri.bundle.conf.json \
  --config '{"bundle":{"createUpdaterArtifacts":false}}' --features coreml
```

Windows (Git Bash): ONNX Runtime's Windows build links DirectML, so
`DirectML.dll` must sit next to the app. Build it once, copy it where the
bundle config expects it, then build:

```sh
pnpm models --gpu
cargo build --release -p cull-ml --features directml
mkdir -p src-tauri/windows
cp "$(find target/release -maxdepth 2 -name DirectML.dll | head -1)" src-tauri/windows/
pnpm tauri build --config src-tauri/tauri.bundle.conf.json --config src-tauri/tauri.bundle.windows.conf.json \
  --config '{"bundle":{"createUpdaterArtifacts":false}}' --features directml
```

`src-tauri/tauri.bundle.conf.json` puts the models inside the installer.
Without it, the app finds its models only in the `models/` folder of the
source tree, which is fine for development but not for an installer you
hand to someone. Add `PELLI_INSTANT_NO_ACCOUNT=1` in front to build without
the account.

`tauri.conf.json` asks for update bundles (`createUpdaterArtifacts`), which
are signed with Pelli Instant's private update key
(`TAURI_SIGNING_PRIVATE_KEY`): without that key, a plain `pnpm tauri build`
stops with an error after bundling. The
`--config '{"bundle":{"createUpdaterArtifacts":false}}'` above builds the
installer without them; it is what the release workflow does in a fork.

A release build you make this way keeps the updater: it checks this
repository's releases and offers them, and installing one replaces your
build with the official one. Launch it with
`PELLI_INSTANT_DISABLE_UPDATER=1` in its environment to keep your own.

### Updates

Release builds check `releases/latest/download/latest.json` eight seconds
after launch and every six hours (`src-tauri/src/updates.rs`). When a newer
version is out, the window offers it; once the photographer installs it, the
app downloads the bundle, checks its signature against the public key in
`src-tauri/tauri.conf.json`, waits until no analysis, export, upload or
Maximum pack download is under way (`Engine::busy`), then installs it and
restarts. It only installs bundles signed with the key above. Debug builds
never check; `PELLI_INSTANT_DISABLE_UPDATER=1` turns it off in a release
build, for packagers who update the app themselves.
