#!/usr/bin/env bash
# Downloads the freely licensed photos used to calibrate cull-analysis
# (crates/cull-analysis/examples/calibrate.rs) into samples/photos/.
#
# The photos are NOT part of the repository (samples/ is gitignored): they
# stay on the developer's machine for calibration only. All are published on
# Flickr under licenses allowing commercial use and adaptation (CC0, Public
# Domain Mark, CC BY 2.0, CC BY-SA 2.0), found through Openverse
# (https://openverse.org). Authors and sources are written to
# samples/photos/MANIFEST.tsv; credit them if a picture is ever shown.
#
# Usage: scripts/fetch-calibration-photos.sh [destination_dir]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest="${1:-$here/../samples/photos}"
mkdir -p "$dest"
ua="PelliInstantCalibration/0.1 (+https://pelli.io; contact@pelli.io)"

# name <TAB> url <TAB> license <TAB> version <TAB> author <TAB> source page
photos=$(cat <<'TABLE'
wedding-reception-portrait	https://live.staticflickr.com/5147/5822112893_fd7bf3d217_b.jpg	by	2.0	Gareth1953 All Right Now	https://www.flickr.com/photos/40837632@N05/5822112893
wedding-dance	https://live.staticflickr.com/210/519364774_d577f2babc_b.jpg	by-sa	2.0	Dextera Photography	https://www.flickr.com/photos/89644425@N00/519364774
wedding-dance-floor	https://live.staticflickr.com/119/309480076_6c5f0318da_b.jpg	by	2.0	JimReeves	https://www.flickr.com/photos/39364848@N00/309480076
wedding-father-daughter-dance	https://live.staticflickr.com/6184/6151252251_1984060bd9_b.jpg	by	2.0	Tobyotter	https://www.flickr.com/photos/78428166@N00/6151252251
wedding-bride-buffet	https://live.staticflickr.com/1210/5170582792_e8fa23c0b4_b.jpg	by-sa	2.0	☺ Lee J Haywood	https://www.flickr.com/photos/45034206@N06/5170582792
event-dancing	https://live.staticflickr.com/21/30084676_e22698d002_b.jpg	by	2.0	objectfox	https://www.flickr.com/photos/44124372247@N01/30084676
wedding-yk	https://live.staticflickr.com/7553/15827145726_d57af0dc85_b.jpg	by	2.0	WriteAwaybyKarylle	https://www.flickr.com/photos/93018262@N04/15827145726
wedding-party-group	https://live.staticflickr.com/3773/11470779734_f6540776e3_b.jpg	by	2.0	D-Stanley	https://www.flickr.com/photos/79721788@N00/11470779734
event-three-generations	https://live.staticflickr.com/5451/7389268296_73ebf6b132_b.jpg	by	2.0	Gareth1953 All Right Now	https://www.flickr.com/photos/40837632@N05/7389268296
event-candid-portrait	https://live.staticflickr.com/7174/6396204843_e27e1926c7_b.jpg	by	2.0	Gareth1953 All Right Now	https://www.flickr.com/photos/40837632@N05/6396204843
wedding-family-group	https://live.staticflickr.com/49/146683298_3e28482243_b.jpg	by	2.0	jarrodlombardo	https://www.flickr.com/photos/62248962@N00/146683298
wedding-indian	https://live.staticflickr.com/2002/2046210703_213558c355_b.jpg	by	2.0	sLENGfJES	https://www.flickr.com/photos/74507678@N00/2046210703
wedding-dress-highkey	https://live.staticflickr.com/5290/5379488762_1dfd241396_b.jpg	by	2.0	exousiavampira	https://www.flickr.com/photos/11165134@N03/5379488762
wedding-gowns-white	https://live.staticflickr.com/982/40510687620_1c772ef328_b.jpg	by-sa	2.0	Martina H.	https://www.flickr.com/photos/157338256@N03/40510687620
portrait-bokeh-56mm-a	https://live.staticflickr.com/7467/16337869271_963a302d6d_b.jpg	by	2.0	Henry Söderlund	https://www.flickr.com/photos/46563758@N04/16337869271
portrait-bokeh-56mm-b	https://live.staticflickr.com/8577/16153446879_7887a0669e_b.jpg	by	2.0	Henry Söderlund	https://www.flickr.com/photos/46563758@N04/16153446879
portrait-self-bokeh	https://live.staticflickr.com/3602/3465146107_79356c90a0_b.jpg	by	2.0	georgeparrilla	https://www.flickr.com/photos/92444599@N00/3465146107
portrait-hard-to-focus	https://live.staticflickr.com/8585/15995497403_0f94908f22_b.jpg	by	2.0	JKohler	https://www.flickr.com/photos/34264401@N07/15995497403
detail-pouring-drink	https://live.staticflickr.com/8614/16216246750_082a593490_b.jpg	by	2.0	Henry Söderlund	https://www.flickr.com/photos/46563758@N04/16216246750
detail-lumix-14mm	https://live.staticflickr.com/8673/16217416259_e40f288dc9_b.jpg	by	2.0	Henry Söderlund	https://www.flickr.com/photos/46563758@N04/16217416259
concert-stage-lights	https://live.staticflickr.com/3498/3735171010_90582fc99c_b.jpg	by	2.0	Anirudh Koul	https://www.flickr.com/photos/84856173@N00/3735171010
concert-stage-gamescom	https://live.staticflickr.com/7434/9591269118_a8ef4101da_b.jpg	by-sa	2.0	Sergey Galyonkin	https://www.flickr.com/photos/22974618@N00/9591269118
concert-stage-festival	https://live.staticflickr.com/59/153701069_568495ef84_b.jpg	by-sa	2.0	Salim Virji	https://www.flickr.com/photos/44124427152@N01/153701069
landscape-glacier	https://live.staticflickr.com/7211/7155641089_93bd2a6c17_b.jpg	by	2.0	Richard Allaway	https://www.flickr.com/photos/16287002@N00/7155641089
landscape-snow-highkey	https://live.staticflickr.com/65535/52588539951_ddcd0db8c1_b.jpg	by	2.0	claralieu	https://www.flickr.com/photos/189561381@N07/52588539951
landscape-mountains-rock	https://live.staticflickr.com/1694/24030753220_b22fbea186_b.jpg	cc0	1.0	pixellaphoto	https://www.flickr.com/photos/137643065@N06/24030753220
landscape-mountains-trees	https://live.staticflickr.com/1698/24029434060_fd1ec9dbd3_b.jpg	cc0	1.0	pixellaphoto	https://www.flickr.com/photos/137643065@N06/24029434060
landscape-tropical-coast	https://live.staticflickr.com/65535/47980880218_e6259c35d7_b.jpg	by	2.0	Artem Beliaikin	https://www.flickr.com/photos/157635012@N07/47980880218
night-girls-night-out	https://live.staticflickr.com/2880/10260627553_42dd3007a1_b.jpg	cc0	1.0	romainguy	https://www.flickr.com/photos/24046097@N00/10260627553
night-rain-bokeh-intentional	https://live.staticflickr.com/3514/3908013145_d6718cdb03_b.jpg	cc0	1.0	romainguy	https://www.flickr.com/photos/24046097@N00/3908013145
night-street	https://live.staticflickr.com/107/299419187_ac814785b9_b.jpg	by	2.0	OiMax	https://www.flickr.com/photos/72396314@N00/299419187
night-las-vegas	https://live.staticflickr.com/3350/3557857253_5f09d8c676_b.jpg	by	2.0	Tim Pearce, Los Gatos	https://www.flickr.com/photos/42937777@N00/3557857253
night-vienna-lights	https://live.staticflickr.com/7432/16540648612_93797947ea_b.jpg	by	2.0	d26b73	https://www.flickr.com/photos/96106108@N07/16540648612
detail-wedding-rings-a	https://live.staticflickr.com/176/408909320_f752e4f880_b.jpg	by	2.0	firemedic58	https://www.flickr.com/photos/50841708@N00/408909320
detail-wedding-rings-b	https://live.staticflickr.com/3038/2420103611_8c18fc57dd_b.jpg	by	2.0	apdk	https://www.flickr.com/photos/62337512@N00/2420103611
event-speech	https://live.staticflickr.com/5231/7225286324_0cd18446b5_b.jpg	by	2.0	thekirbster	https://www.flickr.com/photos/17157315@N00/7225286324
event-spotlight-lowkey	https://live.staticflickr.com/3090/2816735522_5239f8923a_b.jpg	by	2.0	jurvetson	https://www.flickr.com/photos/44124348109@N01/2816735522
event-stage-talk	https://live.staticflickr.com/3844/15105407149_474562e80b_b.jpg	by	2.0	jurvetson	https://www.flickr.com/photos/44124348109@N01/15105407149
event-hall	https://live.staticflickr.com/3569/3296056687_ddd7b2a188_b.jpg	by	2.0	Tony Fischer Photography	https://www.flickr.com/photos/22714323@N06/3296056687
TABLE
)

manifest="$dest/MANIFEST.tsv"
printf 'file\tlicense\tauthor\tsource\n' > "$manifest"
ok=0
failed=0
while IFS=$'\t' read -r name url license version author source; do
  [ -z "$name" ] && continue
  file="$dest/$name.jpg"
  printf '%s.jpg\tCC %s %s\t%s\t%s\n' "$name" "$license" "$version" "$author" "$source" >> "$manifest"
  if [ -s "$file" ]; then
    ok=$((ok + 1))
    continue
  fi
  tmp="$file.part"
  if curl -fsSL --retry 3 --retry-delay 2 -A "$ua" -o "$tmp" "$url" \
    && [ "$(head -c 2 "$tmp" | od -An -tx1 | tr -d ' \n')" = "ffd8" ]; then
    mv "$tmp" "$file"
    ok=$((ok + 1))
    echo "ok      $name"
  else
    rm -f "$tmp"
    failed=$((failed + 1))
    echo "FAILED  $name ($url)" >&2
  fi
  sleep 0.3
done <<< "$photos"

echo "$ok photos in $dest ($failed failed). Attribution: $manifest"
