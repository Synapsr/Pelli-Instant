# Security policy

Pelli Instant opens your photo folders, reads files from every kind of
camera, writes next to your photos when you export, keeps a sign-in token
for your Pelli account, downloads AI models when you ask for the Maximum
precision, and installs its own updates. We take security reports seriously. Thank you
for helping keep it safe.

## Reporting a vulnerability

**Please don't open a public issue.** Email **[contact@pelli.io](mailto:contact@pelli.io)**
with `Security` in the subject.

Helpful details:

- The Pelli Instant version (Settings → About) or the commit you built from,
  and your operating system and version.
- What an attacker could do, and what they need to do it.
- Steps to reproduce, or a proof of concept. For a file that crashes or
  misleads the decoder, the smallest file that shows it.
- Whether you've shared the issue with anyone else.

Never include a Pelli access token in a report, nor photos you don't have the
right to share.

## What happens next

Pelli Instant is maintained by a small team, so we can't promise fixed
deadlines. We'll do our best to:

- acknowledge your report within a week;
- tell you whether we can reproduce it, and keep you posted while we fix it;
- publish the fix in a new release and credit you in its notes, if you'd like.

Please give us a reasonable time to ship a fix before disclosing the issue
publicly.

## Supported versions

Security fixes go into the latest release only, which installed copies are
offered as an in-app update. Before the first release, they go into the
`main` branch.

## Scope

In scope: the code in this repository, including:

- reading untrusted files: folder scanning, RAW containers, EXIF, embedded
  JPEG previews (`crates/cull-decode`);
- what the app writes: XMP sidecars, copies of the selection, rejects moved
  to a sub-folder (`crates/cull-analysis/src/xmp.rs`,
  `crates/cull-engine/src/export.rs`);
- the sign-in flow, how the token is stored (macOS Keychain, Windows
  Credential Manager) and the requests sent to the Pelli API;
- the desktop shell: commands exposed to the interface, the asset protocol
  scope that serves previews (`src-tauri`);
- the model files' checksum verification at load (`crates/cull-ml`);
- the download of the Maximum pack: HTTP range resume, SHA-256 of each file
  against the manifest compiled into the app, the check of every model
  against reference outputs (`crates/cull-engine/src/modelpack.rs`,
  `crates/cull-ml/src/maximum.rs`), and `PELLI_MODELS_BASE_URL`, which only
  debug builds read;
- the in-app updater: how `latest.json` is read, the check of each update's
  signature against the public key in `src-tauri/tauri.conf.json`, and when
  it installs (`src-tauri/src/updates.rs`);
- the release workflow, the installers it produces, and how it signs them
  and the update bundles (the private update key is a CI secret, never in
  the repository).

Out of scope, please report them to the right place:

- The Pelli web platform (`app.pelli.io`) and your Pelli account.
- ONNX Runtime, the AI models, Tauri's updater plugin and other
  dependencies: report to their maintainers. Tell us too if Pelli Instant is
  affected.
- Hugging Face and GitHub, which host models and releases: a file they
  serve is only used once its SHA-256 or its signature matches.
- Builds made by someone else from this code, such as builds without the
  Pelli account.
- Attacks that require an already compromised computer or user account.
