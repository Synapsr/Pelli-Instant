# Pelli API used by Pelli Instant

The HTTP contract the desktop app relies on. The server side lives in Pelli (not part of this repository); `scripts/fake-pelli.mjs` implements the same contract for local development, and a change to the contract lands on both sides together.

- Base URL: `https://app.pelli.io`. Routes are `POST {base}/api/cull/<route>` with a JSON body (send `{}` when a route takes no input), except `GET offers`; every answer is JSON.
- Another Pelli (staging, local, the fake): set `PELLI_API_URL`. **Release builds read it when they are compiled, never at run time**, so an official build always talks to the Pelli it was built for; debug builds (`pnpm tauri dev`, `pnpm app:fake-pelli`, the dev bridge) also read it at run time (`cull_engine::pelli::api_url`).
- `Accept-Language`: the app language (`en`, `fr`, `es`, `de`, `it`, `pt`, `nl`, `pl`, `sv`); it localises `account.planLabel` and the offers. Without it, the account language is used.
- Bearer routes take `Authorization: Bearer <accessToken>` (the token from `auth/check-session`, valid 30 days). There is no refresh token: on `401` sign in again.

## Errors

Every error is JSON with a meaningful HTTP status:

```json
{ "code": "quota_exceeded", "message": "The monthly photo quota of this plan is reached.", "error": "…same text…", "type": "HTTPException" }
```

The machine code is `code`; `message` is an English developer hint, never UI text (`type` may be absent; some routes add fields, listed below). Codes:

| Status | `code` | Meaning |
|---|---|---|
| 400 | `invalid_request` | Body failed validation; `message` names the field |
| 401 | `missing_token`, `invalid_token` | No / bad / expired / non-Instant bearer token → sign in again |
| 403 | `account_unavailable` | Account banned, archived or pending review (every Bearer route) |
| 403 | `quota_exceeded` | `usage/reserve`: not enough photos left this month; body has `monthlyQuota`, `usedThisMonth`, `remaining`, `resetsAt` |
| 403 | `subscription_required` | Account behind the Pelli paywall (no plan, no trial): sorting works, sending to a gallery does not (`entitlements.sendToGallery` is `false`) |
| 403 | `gallery_limit_reached` | The plan's gallery limit is reached |
| 404 | `gallery_not_found` | Unknown, deleted or not owned by this account |
| 409 | `request_conflict` | `requestId` reused with different values |
| 409 | `folder_exists` | A folder with this name (case/accent-insensitive) already exists |
| 429 | `rate_limited` | Body has `retryAfter` (seconds), header `Retry-After` |

Rate limits: session creation 20 / hour / IP; `check-session` 60 / minute / session (poll every `interval` = 3 s); `offers` 120 / hour / IP; Bearer routes 180 reads and 120 writes (`galleries/create`, `folders/create`, `upload-token`, `usage/reserve`, `usage/release`) per minute per account.

What the app does with them (the `code` of command errors and `analysis-error` / `upload-error` events, see `src/lib/contract.ts`): `missing_token`, `invalid_token` and `account_unavailable` sign the app out and read as `unauthorized`; no network, a non-JSON answer or a 5xx read as `offline`; `quota_exceeded` becomes `quotaExceeded` with `details`; other codes are passed through.

## Sign-in

### `auth/create-session` (no auth)

```json
{ "deviceName": "MacBook Pro de Camille", "platform": "macos", "appVersion": "0.1.0" }
```

`deviceName`: 1–200 chars (shown on the consent page, trimmed to one line of 80 chars); `platform`: `macos` | `windows` | `linux`; `appVersion`: `[0-9A-Za-z][0-9A-Za-z.+-]{0,31}`.

```json
{
  "sessionId": "cull_<43 base64url chars>",
  "authUrl": "https://app.pelli.io/cull/authorize?session=cull_…",
  "expiresIn": 600,
  "interval": 3
}
```

Open `authUrl` in the default browser. The photographer signs in if needed, sees “Pelli Instant on «deviceName» wants to access your account” and allows or denies. Keep `sessionId` secret: it is what gets the token, and it never appears in `authUrl` (the consent page uses a separate public id, derived from it one way).

### `auth/check-session` (no auth)

```json
{ "sessionId": "cull_…" }
```

Poll every `interval` seconds until a final status, for at most `expiresIn` seconds:

```json
{ "status": "pending" }
{ "status": "approved", "accessToken": "<JWT>", "expiresIn": 2592000, "account": { …Account… } }
{ "status": "denied" }
{ "status": "expired" }
```

`approved` and `denied` are delivered **once**; afterwards (and for an unknown, malformed or timed-out session) the answer is `expired`. Store `accessToken` in the OS keychain.

### `me` (Bearer)

`{}` → `{ "account": Account }`. Call at start-up (plan and usage can change in Pelli).

```json
{
  "name": "Camille",
  "email": "camille@example.com",
  "slug": "camille",
  "avatarUrl": "https://pelli.s3.fr-par.scw.cloud/logo/…",
  "plan": "lite",
  "planLabel": "Lite",
  "trialEndsAt": null,
  "trialAvailable": false,
  "lifetime": false,
  "entitlements": {
    "analyze": true,
    "monthlyQuota": 1000,
    "usedThisMonth": 312,
    "resetsAt": 1798761600000,
    "sendToGallery": true
  }
}
```

- `plan`: `free` | `lite` | `pro` | `ultra` (the app reads any other value as `free`). `planLabel`: localised (“Gratuit”, “Pro”, “Essai Ultra”, “Ultra à vie”, …). `avatarUrl` may be `null`.
- `trialEndsAt`: end of the running Pelli trial (unix ms), `null` without trial; `trialAvailable`: the account can still start one; `lifetime`: lifetime deal.
- `analyze`: `false` for a blocked account. `monthlyQuota`: photos sorted per UTC calendar month — free 200, Lite 1 000, `null` (unlimited) for Pro, Ultra (lifetime deals included) and during any running trial. `usedThisMonth`: photos already counted this month; `resetsAt`: first instant of next month (UTC, unix ms).
- `sendToGallery`: `false` when the account is behind the Pelli paywall (no plan, no running trial). Gallery limits of the plan still apply on creation.

The app keeps the last account for offline use; every field is optional when it reads it back, so the account cached by an older version still loads.

## Monthly quota

Sorting runs on the photographer's computer, always; Pelli only counts how many photos are sorted. A photo counts **once**: the first time a project analyses it (a RAW + JPEG pair is one photo). Analysing it again — changed file, new models, previous error — review, export and sending to a gallery never count.

How the app uses the two routes, for each analysis:

1. After scanning the folder, it lists the photos it will analyse for the first time (no analysis stored for them yet). None: nothing is asked, no account needed.
2. It reserves them all at once (`usage/reserve`). Signed out → error `unauthorized`. Refused → error `quotaExceeded`, nothing is analysed.
3. Pelli unreachable: an account cached as unlimited (`monthlyQuota: null`) analyses without counting; a limited one cannot (error `quotaOffline`).
4. When the run ends (done, cancelled or failed), photos reserved on a limited plan but not analysed (cancelled before them, unreadable files) are given back with `usage/release`, best effort.
5. Each answer updates the cached account (`usedThisMonth`, `monthlyQuota`, `resetsAt`) and the app emits `auth-changed`.

The `quotaExceeded` / `quotaOffline` errors carry `details`:

```json
{ "code": "quotaExceeded", "message": "…", "details": { "monthlyQuota": 200, "usedThisMonth": 190, "remaining": 10, "resetsAt": 1798761600000, "needed": 39 } }
{ "code": "quotaOffline", "message": "…", "details": { "monthlyQuota": 200, "needed": 39 } }
```

### `usage/reserve` (Bearer)

```json
{ "photos": 39 }
```

`photos`: integer 1–50 000 (an analysis needing more asks for 50 000: no limited plan comes near). All or nothing.

```json
{ "granted": true, "unlimited": false, "monthlyQuota": 200, "usedThisMonth": 39, "remaining": 161, "resetsAt": 1798761600000 }
```

`usedThisMonth` already includes this reservation. Unlimited plans: `"unlimited": true`, `monthlyQuota` and `remaining` are `null`, the counter is not touched. Too few photos left: `403 quota_exceeded` with `monthlyQuota`, `usedThisMonth`, `remaining`, `resetsAt` (nothing counted). Errors: `403 account_unavailable`.

### `usage/release` (Bearer)

```json
{ "photos": 12 }
```

`photos`: integer 1–50 000, photos reserved but not analysed. → `{ "usedThisMonth": 27 }` (never below 0; no effect on an unlimited plan).

## Offers

### `GET offers?lang=fr` (no auth)

Pelli's plans as the app shows them (home screen, quota screens), so prices and links stay right when Pelli changes them. `lang`: an app language (default: `Accept-Language`). Cached 5 minutes by HTTP; the app keeps the last answer per language for offline use.

```json
{
  "currency": "EUR",
  "trialDays": 15,
  "plans": [
    { "slug": "free", "name": "Gratuit", "monthlyPrice": 0, "yearlyPrice": 0, "yearlyPricePerMonth": 0, "photosPerMonth": 200, "highlights": ["Essai de 15 jours de Pelli", "Sans carte bancaire"] },
    { "slug": "lite", "name": "Lite", "monthlyPrice": 5, "yearlyPrice": 60, "yearlyPricePerMonth": 5, "photosPerMonth": 1000, "highlights": ["…"] },
    { "slug": "pro", "name": "Pro", "monthlyPrice": 25, "yearlyPrice": 180, "yearlyPricePerMonth": 15, "photosPerMonth": null, "highlights": ["…"] },
    { "slug": "ultra", "name": "Ultra", "monthlyPrice": 50, "yearlyPrice": 420, "yearlyPricePerMonth": 35, "photosPerMonth": null, "highlights": ["…"] }
  ],
  "links": {
    "register": "https://app.pelli.io/register?utm_source=pelli-instant&utm_content=register",
    "pricing": "https://app.pelli.io/dashboard/billing?utm_source=pelli-instant&utm_content=pricing#plans-section",
    "trial": "https://app.pelli.io/register?utm_source=pelli-instant&utm_content=trial",
    "billing": "https://app.pelli.io/dashboard/billing?utm_source=pelli-instant&utm_content=billing"
  }
}
```

Prices per month billed monthly (`monthlyPrice`), per year (`yearlyPrice`) and the yearly price per month; `photosPerMonth`: `null` = unlimited; `highlights`: what the plan brings in Pelli itself, localised.

## Galleries

```json
{
  "id": "…",
  "title": "Mariage Léa & Tom",
  "slug": "mariage-lea-tom",
  "status": "draft",
  "imageCount": 312,
  "coverUrl": "https://pelli.s3.fr-par.scw.cloud/banner/….jpg",
  "createdAt": 1790845200000,
  "url": "https://app.pelli.io/dashboard/gallery/mariage-lea-tom"
}
```

`coverUrl` may be `null`; `createdAt` is unix ms; `url` opens the gallery in the Pelli dashboard (publish, share) — use it as `UploadReport.galleryUrl`.

### `galleries/list` (Bearer)

`{}` → `{ "galleries": Gallery[] }`: the account's own galleries (not shared ones, not deleted), most recent first, at most 200.

### `galleries/create` (Bearer)

```json
{ "requestId": "6f1d7b52-3c1e-4a8e-9a43-1b2c3d4e5f60", "title": "Mariage Léa & Tom", "type": "selection" }
```

`type`: `selection` (clients pick favourites, watermark) | `delivery` (download). → `{ "gallery": Gallery, "password": string | null }`.

Idempotent on `requestId` (a UUID you generate and **keep for retries**): replaying returns the same gallery; reusing it with another title or type, or after the photographer deleted that gallery in Pelli, is `409 request_conflict` (use a new `requestId`). The gallery is a draft with the photographer's dashboard defaults (price, expiry, cover, default password — returned in `password` when the account sets one). Errors: `403 subscription_required`, `403 gallery_limit_reached`.

### `folders/create` (Bearer)

```json
{ "requestId": "<uuid>", "galleryId": "…", "name": "Cérémonie" }
```

→ `{ "folder": { "id": "…", "name": "Cérémonie" } }`. Idempotent on `requestId`. Owner's galleries only (`404 gallery_not_found`); `409 folder_exists` when the name is taken (names differing only by case or accents are the same name).

## Uploading photos

### `upload-token` (Bearer)

```json
{ "galleryId": "…" }
```

```json
{
  "uploadToken": "<JWT>",
  "uploadUrl": "https://api-upload.pelli.io/api/photo",
  "expiresIn": 172800,
  "expiresAt": 1793974800000,
  "maxFileSize": 104857600,
  "maxConcurrency": 3,
  "fileField": "file",
  "nameParam": "name",
  "folderParam": "folder",
  "acceptedExtensions": [".jpg", ".jpeg", ".jpe", ".png", ".gif", ".webp", ".tif", ".tiff", ".bmp", ".heic", ".heif", ".avif", ".arw", ".cr2", ".cr3", "…"],
  "galleryId": "…",
  "galleryTitle": "Mariage Léa & Tom",
  "storage": { "usedBytes": 21474836480, "limitBytes": 107374182400 }
}
```

The token is scoped to that one gallery and valid 2 days: request a new one per gallery and when it is about to expire. Owner's galleries only. Errors: `403 subscription_required`, `404 gallery_not_found`.

### The upload request (Pelli upload service)

Exactly what the Pelli dashboard uploader does (one request per photo):

```http
POST {uploadUrl}?name=<URL-encoded file name>&folder=<folder.id>
Authorization: <uploadToken>
Content-Type: multipart/form-data; boundary=…
X-Pelli-Upload-Batch: <batch id>     (optional, log correlation)
X-Pelli-Upload-File: <file id>       (optional)

--boundary
Content-Disposition: form-data; name="file"; filename="IMG_0001.jpg"
Content-Type: image/jpeg

<bytes>
--boundary--
```

- **`Authorization` carries the raw upload token, without `Bearer `.**
- `name` and `folder` are **query parameters**, not form fields. Omit `folder` to upload to the gallery root. The gallery comes from the token, never from the request: do not send a `galleryId`.
- One multipart field, named `file` (`fileField`). Max `maxFileSize` bytes (100 MB) per photo; larger files are refused (`413 file_too_large`).
- No `source` parameter (the dashboard sends none; the Lightroom plugin sends `source=lightroom`, which would mislabel Cull uploads; `source=cull` is not a value the upload service is known to accept).
- Prefer JPEGs (the JPEG twin of a RAW, or an export): RAW files are converted server-side and that conversion can fail (`422 raw_conversion_failed`).
- Before a batch, check that its total size fits in `storage.limitBytes - storage.usedBytes`, and stop when it no longer does.
- Concurrency: at most `maxConcurrency` (3) uploads in parallel; 1 on a slow connection.

Success: `200 { "success": true, "imageId": "…", "requestId"?: "…" }` — count a photo as uploaded only on a 2xx with `success: true`.

Errors (`code`, or the legacy `error` string):

| Status | Code | What to do |
|---|---|---|
| 401 | `missing_authorization`, `invalid_token` | Get a new `upload-token`, retry once |
| 403 | `upload_forbidden` | Stop (no access to the gallery) |
| 404 | `gallery_not_found` | Stop (gallery deleted) |
| 400 | `folder_not_found` | Create the folder again (`folders/create`) or upload to the root |
| 413 | `file_too_large` | Skip the file |
| 422 | `unsupported_format`, `raw_conversion_failed` | Skip the file (send its JPEG) |
| 408, 429, 5xx | `image_processing_timeout`, `server_error`… | Retry with growing delays (2 s, 5 s, 15 s) |

When the whole body was sent but no response arrived, do **not** resend automatically: the photo may already be in the gallery (duplicate). Report it as unconfirmed instead.

## Builds without account

`PELLI_INSTANT_NO_ACCOUNT=1 pnpm tauri build` (or `tauri dev`, `cargo build`; `1`, `true` or `yes`) compiles an app that never asks to sign in to analyse and never calls `usage/*`: no quota. Everything else is identical (sending to a gallery still signs in). The switch is read at compile time; the app reports it as `app_info.noAccount`. Official builds are compiled without it and, like any release build, cannot be pointed at another Pelli at run time (see the base URL above): lifting the quota means building from source.

The `pelli-cull analyze` command line (a developer tool) never counts photos either.
