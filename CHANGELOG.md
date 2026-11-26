# Changelog

What changes from one version of Pelli Instant to the next. Each section
becomes the text of its GitHub release and the `notes` of `latest.json`: the
in-app announcement of a new version shows its first points, in English, or
in French from the `Français` block.

## Unreleased

- **Maximum precision now installs on Windows, and on the Macs where it failed.** It downloaded, then stopped at the preparation step on every Windows computer and on some Macs: one of its models gave results that depended on the processor, wrong on many of them. It is replaced by its full-precision version, exact on every processor (the pack is now about 1 GB instead of 777 MB). If Maximum precision was already installed, *Settings → Analysis* offers to download the new file (347 MB); until then analyses run in Standard, and the previous files stay on disk until the new one is checked.
- When Maximum precision cannot work on a computer, the app now says so clearly instead of "The download failed", and no longer tries again at every launch. *Try again* downloads the faulty file again.
- The app keeps a log file (`pelli-instant.log`, in `~/Library/Logs/io.pelli.instant` on Mac and `%LOCALAPPDATA%\io.pelli.instant\logs` on Windows) about Maximum precision, its models and updates, to help us when something goes wrong: no photo or folder name, no user name, no licence.
- With usage statistics on, a failed installation of Maximum precision is reported anonymously (the step, the error code and the model only).
- After each completed sort, the free version shows a small card on the result screen to follow Pelli on Instagram or leave a star on GitHub. Never a pop-up and never in the way of the review: it closes with its × until the next sort, and Pelli Instant Plus removes it.

<details><summary>Français</summary>

- **La précision maximale s'installe maintenant sur Windows, et sur les Mac où elle échouait.** Elle se téléchargeait, puis s'arrêtait à l'étape de préparation sur tous les PC Windows et sur certains Mac : un de ses modèles donnait des résultats qui dépendaient du processeur, faux sur beaucoup d'entre eux. Il est remplacé par sa version en pleine précision, exacte sur tous les processeurs (le pack fait maintenant environ 1 Go au lieu de 777 Mo). Si la précision maximale était déjà installée, *Réglages → Analyse* propose de télécharger le nouveau fichier (347 Mo) ; en attendant, les analyses se font en Standard, et les anciens fichiers restent sur le disque jusqu'à ce que le nouveau soit vérifié.
- Quand la précision maximale ne peut pas fonctionner sur un ordinateur, l'app le dit clairement au lieu de « Le téléchargement a échoué », et ne réessaie plus à chaque lancement. *Réessayer* télécharge à nouveau le fichier en cause.
- L'app tient un fichier journal (`pelli-instant.log`, dans `~/Library/Logs/io.pelli.instant` sur Mac et `%LOCALAPPDATA%\io.pelli.instant\logs` sur Windows) sur la précision maximale, ses modèles et les mises à jour, pour nous aider quand quelque chose ne va pas : aucun nom de photo ni de dossier, ni nom d'utilisateur, ni licence.
- Avec les statistiques d'utilisation activées, l'échec de l'installation de la précision maximale est signalé anonymement (l'étape, le code d'erreur et le modèle seulement).
- Après chaque tri terminé, la version gratuite affiche sur l'écran de résultat une petite carte pour suivre Pelli sur Instagram ou laisser une étoile sur GitHub. Jamais une fenêtre surgissante ni un frein à la revue : elle se ferme avec sa croix jusqu'au tri suivant, et Pelli Instant Plus la retire.

</details>

## 1.1.0 — 2026-11-17

- **Send your selection to a Pelli gallery**, straight from the export: a new gallery named after the shoot, one folder per moment, the camera JPEGs (never the RAW files). Sign in with Pelli, or try Pelli free for 15 days.
- **Detailed statistics with Plus**: your keep rate month by month, why photos were rejected, how often you changed the AI's call, your cameras, bursts, the measured speed and every shoot. Calculated on your computer, never sent.
- The free version shows the photos you sorted and your shoots in the new Statistics view.
- A gallery upload goes on in the background, with its progress in the title bar.
- Sending to Pelli keeps a single copy of each photo in memory.
- A new installer: on Mac, a window that shows where to drag Pelli Instant; on Windows, Pelli Instant's image on the first and last pages.

<details><summary>Français</summary>

- **Envoyez votre sélection vers une galerie Pelli**, directement depuis l'export : une nouvelle galerie au nom de la séance, un dossier par moment, les JPEG de l'appareil (jamais les RAW). Connectez-vous avec Pelli, ou essayez Pelli gratuitement pendant 15 jours.
- **Statistiques détaillées avec Plus** : votre taux de sélection mois par mois, les raisons des rejets, la part des choix de l'IA que vous avez modifiés, vos boîtiers, les rafales, la vitesse mesurée et chaque séance. Calculées sur votre ordinateur, jamais envoyées.
- La version gratuite montre vos photos triées et vos séances dans la nouvelle vue Statistiques.
- L'envoi vers une galerie continue en arrière-plan, avec son avancement dans la barre du haut.
- L'envoi vers Pelli ne garde qu'une copie de chaque photo en mémoire.
- Un nouvel installeur : sur Mac, une fenêtre qui montre où glisser Pelli Instant ; sur Windows, l'image de Pelli Instant sur la première et la dernière page.

</details>

## 1.0.1 — 2026-11-17

- **Not enough disk space? The app now says so**, with the space needed: when downloading the Maximum precision, during an analysis or an export.
- **"See the selection"** at the end of a sort.
- **With Plus, usage statistics are linked to your licence or your Pelli account**; the free version stays anonymous. Turned off in Settings › Privacy, nothing is sent.

<details><summary>Français</summary>

- **Plus assez d'espace disque ? L'app le dit clairement**, avec la place nécessaire : au téléchargement de la précision maximale, pendant une analyse ou un export.
- **« Voir le tri »** à la fin d'un tri.
- **Avec Plus, les statistiques d'usage sont associées à votre licence ou à votre compte Pelli** ; la version gratuite reste anonyme. Désactivées dans Réglages › Confidentialité, rien n'est envoyé.

</details>

## 1.0.0 — 2026-11-17

- **Free and unlimited, no account.** Sort as many shoots as you like with the Standard precision, 100% on your computer.
- **Pelli Instant Plus**, 25 € (or $25) once per computer: the Maximum precision, AI search and no Pelli promotion. Included with Pelli Pro and Ultra.
- **Lightroom:** colour labels in your Lightroom's language, and selected or rejected photos arrive flagged Picked or Rejected (Lightroom Classic 13.2 and later).
- **Faster review:** walk through a burst with ⌥← ⌥→ without unstacking it, choose its best frame with S, full screen with F. The loupe bar no longer covers the photo.
- **New versions announced** in the middle of the window, and a new welcome tour with real photos.
- A "See all" view of your projects; the cover of a stack is its best frame; the frames of a burst follow the grid's sort.
- Anonymous usage statistics (never a photo or a file name), turned off in Settings › Privacy.

<details><summary>Français</summary>

- **Gratuit et illimité, sans compte.** Triez autant de séances que vous voulez en précision Standard, 100 % sur votre ordinateur.
- **Pelli Instant Plus**, 25 € une fois par ordinateur : la précision maximale, la recherche IA et aucune promotion Pelli. Inclus avec Pelli Pro et Ultra.
- **Lightroom :** étiquettes de couleur dans la langue de votre Lightroom, et photos marquées Retenue ou Rejetée (Lightroom Classic 13.2 et suivants).
- **Revue plus rapide :** parcourez une rafale avec ⌥← ⌥→ sans la dépiler, choisissez la meilleure avec S, plein écran avec F. La barre de la loupe ne cache plus la photo.
- **Les nouvelles versions s'annoncent** au milieu de la fenêtre, et un nouvel accueil avec de vraies photos.
- Une vue « Tout voir » de vos projets ; la couverture d'une pile est sa meilleure photo ; les photos d'une rafale suivent le tri de la grille.
- Statistiques d'usage anonymes (jamais une photo ni un nom de fichier), désactivables dans Réglages › Confidentialité.

</details>

## 0.1.1 — 2026-11-07

- Easier review of bursts, ratings visible on the grid, and the stars given in the camera kept.
- No longer asks for model files that do not exist.

## 0.1.0 — 2026-11-07

- First public release: AI photo culling on your computer, for Mac (Apple Silicon) and Windows.
