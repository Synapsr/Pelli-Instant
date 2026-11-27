# Installer Pelli Instant sur Windows

*[English](install-windows.md)*

Pelli Instant fonctionne sur Windows 10 et Windows 11, 64 bits (x64). Il s'installe pour votre seul compte utilisateur, sans droits d'administrateur, et ne demande aucun compte.

1. [Télécharger l'installeur](#1-télécharger-linstalleur)
2. [Vérifier le téléchargement (facultatif)](#2-vérifier-le-téléchargement-facultatif)
3. [Lancer l'installeur et passer SmartScreen](#3-lancer-linstalleur)
4. [Mises à jour](#4-mises-à-jour)
5. [Installer avec winget (bientôt)](#installer-avec-winget-bientôt)
6. [Désinstaller](#désinstaller)

## 1. Télécharger l'installeur

Téléchargez **[PelliInstant-windows-x64-setup.exe](https://github.com/Synapsr/Pelli-Instant/releases/latest/download/PelliInstant-windows-x64-setup.exe)** depuis la [dernière version](https://github.com/Synapsr/Pelli-Instant/releases/latest). Il pèse environ 360 Mo : les modèles d'IA Standard sont inclus, donc Pelli Instant marche dès qu'il est installé, même hors ligne.

Si votre navigateur dit que ce fichier n'est pas téléchargé couramment, choisissez de le conserver. Dans Microsoft Edge : **⋯ → Conserver → Afficher plus → Conserver quand même**.

<!-- Capture à ajouter (Ryan ou Téo) : l'avertissement de téléchargement d'Edge, s'il apparaît → docs/images/windows/edge-keep.fr.png -->

## 2. Vérifier le téléchargement (facultatif)

Chaque version est fabriquée en public par le [workflow de publication](../.github/workflows/release.yml), et vous pouvez vérifier que votre fichier est bien celui qu'il a fabriqué.

**Empreinte SHA-256.** Téléchargez [PelliInstant-windows-x64-setup.exe.sha256](https://github.com/Synapsr/Pelli-Instant/releases/latest/download/PelliInstant-windows-x64-setup.exe.sha256) dans le même dossier, ouvrez PowerShell dans ce dossier et lancez :

```powershell
(Get-FileHash .\PelliInstant-windows-x64-setup.exe -Algorithm SHA256).Hash -eq ((Get-Content .\PelliInstant-windows-x64-setup.exe.sha256) -split '\s+')[0]
```

`True` veut dire que le fichier est intact.

**Attestation de build.** Avec la [GitHub CLI](https://cli.github.com) connectée, lancez :

```powershell
gh attestation verify .\PelliInstant-windows-x64-setup.exe --repo Synapsr/Pelli-Instant
```

Elle ne réussit que pour un fichier fabriqué par le workflow de publication de Pelli Instant sur GitHub.

## 3. Lancer l'installeur

Double-cliquez sur `PelliInstant-windows-x64-setup.exe`.

Windows peut afficher une fenêtre bleue, **Windows a protégé votre ordinateur** : *Microsoft Defender SmartScreen a empêché le démarrage d'une application non reconnue.* Cliquez sur **Informations complémentaires**, vérifiez que l'application est `PelliInstant-windows-x64-setup.exe`, puis cliquez sur **Exécuter quand même**. L'éditeur apparaît comme *Éditeur inconnu*.

<!-- Capture à ajouter (Ryan ou Téo) : SmartScreen, première fenêtre avec « Informations complémentaires » → docs/images/windows/smartscreen-1.fr.png -->
<!-- Capture à ajouter (Ryan ou Téo) : SmartScreen après « Informations complémentaires », avec « Exécuter quand même » → docs/images/windows/smartscreen-2.fr.png -->

**Pourquoi cet avertissement ?** L'installeur n'est pas encore signé par un certificat de signature de code. SmartScreen avertit pour tout nouveau programme d'un éditeur qu'il ne connaît pas : l'avertissement ne veut pas dire que Windows a trouvé un problème dans le fichier. Le code de Pelli Instant est open source, l'installeur est fabriqué par le workflow de publication public, et l'[étape 2](#2-vérifier-le-téléchargement-facultatif) vérifie que votre copie est bien celle qu'il a fabriquée. Les mises à jour, elles, sont signées : l'app n'installe que celles signées avec la clé propre à Pelli Instant.

Suivez ensuite l'installeur. Windows ne demande pas de mot de passe administrateur : Pelli Instant s'installe dans votre dossier utilisateur (`%LOCALAPPDATA%\Pelli Instant`) et dans le menu Démarrer. La dernière page propose de créer un raccourci sur le bureau.

Pelli Instant affiche sa fenêtre avec Microsoft Edge WebView2, inclus dans Windows 11. Si un PC sous Windows 10 ne l'a pas, l'installeur le télécharge chez Microsoft : gardez internet pendant l'installation.

<!-- Capture à ajouter (Ryan ou Téo) : la dernière page de l'installeur → docs/images/windows/installer-finish.fr.png -->

## 4. Mises à jour

Pelli Instant se met à jour tout seul. Quand une nouvelle version sort, l'app l'annonce avec ses nouveautés. Un clic suffit : la mise à jour est téléchargée, sa signature est vérifiée, puis une petite fenêtre de progression l'installe sans rien demander et l'app redémarre sur la nouvelle version. *Réglages → À propos* permet de vérifier à la main. Voir [Mises à jour](../README.fr.md#mises-à-jour).

## Installer avec winget (bientôt)

Pelli Instant n'est pas encore dans winget. Une fois son paquet accepté, cette commande l'installera depuis un terminal :

```powershell
winget install --id Pelli.PelliInstant --exact
```

<!-- À faire avant fusion : confirmer l'identifiant du paquet une fois accepté par microsoft/winget-pkgs, ou retirer cette section. -->

## Désinstaller

Avant de désinstaller :

- **Plus**, pour l'utiliser sur un autre ordinateur : *Réglages → Pelli Instant Plus → Libérer cette licence*, après avoir copié votre clé. Pour réinstaller sur le même ordinateur, il suffit de ressaisir la clé.
- **Compte Pelli** : déconnectez-vous dans l'app. La déconnexion supprime le jeton du compte du Gestionnaire d'identification de Windows ; la désinstallation, non.

Ensuite :

- **Windows 11 :** *Paramètres → Applications → Applications installées*, **⋯** à côté de Pelli Instant, **Désinstaller**.
- **Windows 10 :** *Paramètres → Applications → Applications et fonctionnalités*, Pelli Instant, **Désinstaller**.

Le programme de désinstallation propose **Supprimer les données de l'application**. Cochée, cette case supprime aussi votre bibliothèque de séances triées et vos choix, les modèles de la précision Maximale si vous les avez téléchargés, et les aperçus (les dossiers `io.pelli.instant` de `%APPDATA%` et `%LOCALAPPDATA%`). Laissez-la décochée pour tout retrouver après une réinstallation. Vos photos, et les fichiers XMP et dossiers que vous avez exportés, ne sont jamais touchés.

<!-- Capture à ajouter (Ryan ou Téo) : le programme de désinstallation avec « Supprimer les données de l'application » → docs/images/windows/uninstall.fr.png -->
