# Install Pelli Instant on Windows

*[Français](install-windows.fr.md)*

Pelli Instant runs on Windows 10 and Windows 11, 64-bit (x64). It installs for your user account only, without administrator rights, and needs no account.

1. [Download the installer](#1-download-the-installer)
2. [Check the download (optional)](#2-check-the-download-optional)
3. [Run the installer and get past SmartScreen](#3-run-the-installer)
4. [Updates](#4-updates)
5. [Install with winget (coming soon)](#install-with-winget-coming-soon)
6. [Uninstall](#uninstall)

## 1. Download the installer

Download **[PelliInstant-windows-x64-setup.exe](https://github.com/Synapsr/Pelli-Instant/releases/latest/download/PelliInstant-windows-x64-setup.exe)** from the [latest release](https://github.com/Synapsr/Pelli-Instant/releases/latest). It is about 360 MB: the Standard AI models come inside, so Pelli Instant works as soon as it is installed, offline included.

If your browser says the file isn't commonly downloaded, choose to keep it. In Microsoft Edge: **⋯ → Keep → Show more → Keep anyway**.

<!-- Screenshot to add (Ryan or Téo): the Edge download warning, if it shows → docs/images/windows/edge-keep.png -->

## 2. Check the download (optional)

Each release is built in public by the [release workflow](../.github/workflows/release.yml), and you can check that the file you have is the one it built.

**SHA-256 fingerprint.** Download [PelliInstant-windows-x64-setup.exe.sha256](https://github.com/Synapsr/Pelli-Instant/releases/latest/download/PelliInstant-windows-x64-setup.exe.sha256) into the same folder, open PowerShell in that folder and run:

```powershell
(Get-FileHash .\PelliInstant-windows-x64-setup.exe -Algorithm SHA256).Hash -eq ((Get-Content .\PelliInstant-windows-x64-setup.exe.sha256) -split '\s+')[0]
```

`True` means the file is intact.

**Build attestation.** With the [GitHub CLI](https://cli.github.com) signed in, run:

```powershell
gh attestation verify .\PelliInstant-windows-x64-setup.exe --repo Synapsr/Pelli-Instant
```

It succeeds only for a file built by Pelli Instant's release workflow on GitHub.

## 3. Run the installer

Double-click `PelliInstant-windows-x64-setup.exe`.

Windows may show a blue window, **Windows protected your PC**: *Microsoft Defender SmartScreen prevented an unrecognized app from starting.* Click **More info**, check that the app is `PelliInstant-windows-x64-setup.exe`, then click **Run anyway**. The publisher shows as *Unknown publisher*.

<!-- Screenshot to add (Ryan or Téo): SmartScreen, first window with "More info" → docs/images/windows/smartscreen-1.png -->
<!-- Screenshot to add (Ryan or Téo): SmartScreen after "More info", with "Run anyway" → docs/images/windows/smartscreen-2.png -->

**Why this warning?** The installer isn't signed with a code-signing certificate yet. SmartScreen warns about any new program from a publisher it doesn't know: the warning doesn't mean Windows found a problem in the file. Pelli Instant's code is open source, the installer is built by the public release workflow, and [step 2](#2-check-the-download-optional) checks that your copy is the one it built. Updates are different: the app installs only updates signed with Pelli Instant's own key.

Then follow the installer. Windows doesn't ask for an administrator password: Pelli Instant goes into your user folder (`%LOCALAPPDATA%\Pelli Instant`) and into the Start menu. The last page offers to create a desktop shortcut.

Pelli Instant displays its window with Microsoft Edge WebView2, which Windows 11 includes. If a Windows 10 PC doesn't have it, the installer downloads it from Microsoft, so keep the internet on while it installs.

<!-- Screenshot to add (Ryan or Téo): the installer's last page → docs/images/windows/installer-finish.png -->

## 4. Updates

Pelli Instant updates itself. When a new version is out, it is announced in the app with what's new. Install it in one click: the update is downloaded, its signature is checked, then a small progress window installs it without any question and the app restarts on the new version. *Settings → About* checks by hand. See [Updates](../README.md#updates).

## Install with winget (coming soon)

Pelli Instant isn't in winget yet. Once its package is accepted, this will install it from a terminal:

```powershell
winget install --id Pelli.PelliInstant --exact
```

<!-- TODO before merging: confirm the package id once microsoft/winget-pkgs accepts the package, or remove this section. -->

## Uninstall

Before you uninstall:

- **Plus**, to use it on another computer: *Settings → Pelli Instant Plus → Release this licence*, after copying your key. Reinstalling on the same computer just needs the key again.
- **Pelli account**: sign out in the app. Signing out deletes the account's token from the Windows Credential Manager; uninstalling doesn't.

Then:

- **Windows 11:** *Settings → Apps → Installed apps*, **⋯** next to Pelli Instant, **Uninstall**.
- **Windows 10:** *Settings → Apps → Apps & features*, Pelli Instant, **Uninstall**.

The uninstaller offers **Delete the application data**. Ticked, it also removes your library of sorted shoots and your choices, the Maximum models if you downloaded them, and the previews (the `io.pelli.instant` folders in `%APPDATA%` and `%LOCALAPPDATA%`). Leave it unticked to find everything again after a reinstall. Your photos, and the XMP files and folders you exported, are never touched.

<!-- Screenshot to add (Ryan or Téo): the uninstaller with "Delete the application data" → docs/images/windows/uninstall.png -->
