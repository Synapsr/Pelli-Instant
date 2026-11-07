#!/usr/bin/env bash
# Download a small, representative set of camera RAW samples from
# https://raw.pixls.us (all CC0 / public domain) into samples/raw/.
#
# The samples feed the integration tests and the decode benchmark of
# crates/cull-decode. They are NOT committed (samples/ is gitignored).
#
# Usage: scripts/fetch-samples.sh [--small]
#   --small   only fetch one file per container family (~150 MB instead of ~330 MB)
#
# Re-running the script skips files that are already present with the right
# checksum.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/samples/raw"
BASE="https://raw.pixls.us/getfile.php"
SMALL=0
[[ "${1:-}" == "--small" ]] && SMALL=1

mkdir -p "$DEST"

# id | sha256 | local file name | upstream file name | in --small set
SAMPLES=(
  "983|02b23f9cc6dc4c99fb39aa4d1f65ac227804d06461ac1bd0f842aa2f3beb8fe4|canon-5d-mark-iv.cr2|Canon - EOS 5D Mark IV - RAW (3:2).CR2|1"
  "4662|81fab349ae9080fd0172583b7fd5ea36806e95ffa434909d2ccc6527dde34ee6|canon-eos-r6.cr3|Canon - EOS R6 - 3:2.CR3|1"
  "7882|b3c08a5c7a97299dfd58896a12b939e7b43ec78682761812524825e6e5d087be|canon-eos-r5-mark-ii-craw.cr3|Canon - EOS R5 Mark II - CRAW (3:2).CR3|0"
  "3581|8ca94584578b35a422744c1a610174ce37df0691588edfdecaec3111b3fbbb02|nikon-z6.nef|Nikon - Z 6 - 14bit 14bit compressed (3:2).NEF|1"
  "898|f90deb2819863f5a6ca9913c255fdd83a94963f4284eb88df9d389da7a6c7665|nikon-d750.nef|Nikon - D750 - 14bit 14bit compressed (Lossless) (3:2).NEF|0"
  "2414|250784580ea527442c09004417bb0eead484f2bf3ee8f9121a776ac65bb50d0f|sony-a7m3.arw|Sony - ILCE-7M3 - 14bit 14bit compressed (3:2).ARW|1"
  "6936|639a6d4db881f1359e3ea7e1137b314e59417d24e6d4d862b7202111a2c823b2|sony-a7m4.arw|Sony - ILCE-7M4 - 14bit (3:2).ARW|0"
  "2783|95b33021160b239ceb1a09d46a1b29cb60cb7dd47581feb126b337c219091754|fujifilm-x-t3.raf|Fujifilm - X-T3 - 14bit 14bit compressed (3:2).RAF|1"
  "3115|05513ee72f7cac4165534c9f64995412084f414a033c80c473c79fd94154292e|ricoh-gr-iii.dng|Ricoh - GR III - 14bit (3:2).DNG|1"
  "5961|3a859e917a91698a3e15ce3b3721a48a1fb1296873e0697d3bf7c2793297200d|google-pixel-6-pro.dng|Google - Pixel 6 Pro - 16bit (4:3).dng|0"
  "1993|0b433019e5fa61548d7eb0eb7bb90894251542c056da72355202382696d0ceb8|olympus-e-m1-mark-ii.orf|Olympus - E-M1MarkII - 16bit (4:3).ORF|1"
  "1517|dae2281df393d8c3c1b8c513d00985f1834ae0e69ce18fbd571370c25d95d22b|panasonic-gh5.rw2|Panasonic - DC-GH5 - 4:3.RW2|1"
  "7133|fe2330254c5313d4dde97cbd01e2987f6f1137f1c9c687d30e951f4668979381|pentax-k-1-mark-ii.pef|Pentax - K-1 Mark II - 14bit (3:2).PEF|1"
  "3670|3a1aeeb631b15dfa5be1bebe0e9c96f660ec96021eb6c45c073e33e982ab61b7|samsung-nx500.srw|Samsung - NX500 - 14bit 14bit normal compression (3:2).SRW|0"
)

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

urlencode() {
  # Upstream names only contain letters, digits, spaces, dots, dashes,
  # colons, commas and parentheses.
  local s="$1"
  s="${s// /%20}"
  s="${s//(/%28}"
  s="${s//)/%29}"
  s="${s//:/%3A}"
  s="${s//,/%2C}"
  printf '%s' "$s"
}

failed=0
for entry in "${SAMPLES[@]}"; do
  IFS='|' read -r id sha name upstream small <<<"$entry"
  if [[ $SMALL -eq 1 && "$small" != "1" ]]; then
    continue
  fi
  out="$DEST/$name"
  if [[ -f "$out" ]] && [[ "$(sha256_of "$out")" == "$sha" ]]; then
    echo "ok       $name"
    continue
  fi
  echo "fetching $name"
  tmp="$out.part"
  if curl -fL --retry 3 --retry-delay 2 --connect-timeout 20 -sS \
      -o "$tmp" "$BASE/$id/nice/$(urlencode "$upstream")"; then
    if [[ "$(sha256_of "$tmp")" == "$sha" ]]; then
      mv "$tmp" "$out"
    else
      echo "checksum mismatch for $name" >&2
      rm -f "$tmp"
      failed=1
    fi
  else
    echo "download failed for $name" >&2
    rm -f "$tmp"
    failed=1
  fi
done

echo "samples in $DEST:"
ls -la "$DEST"
exit $failed
