# Credits of the welcome tour

## Photos

Photos: [Pexels](https://www.pexels.com). The welcome tour shows photos published on Pexels under the [Pexels licence](https://www.pexels.com/license/) (free to use and modify, attribution not required; credited here anyway). They are resized to 960 px at most and converted to WebP; `eyes-closed.webp` is a crop of `family-floor.webp`'s original.

`family-blurry.webp` (blurred), `too-dark.webp` (under-exposed) and `pumpkin-blurry.webp` (motion blur) are deliberately degraded copies of the originals listed below, made to show what Pelli Instant sets aside. `pumpkin-a.webp` and `pumpkin-b.webp` are a real near-identical pair, the kind of burst the analysis stacks.

| File | Photographer | Original |
| --- | --- | --- |
| `family-kiss.webp` | Andressa Chagas | `pexels-andressa-chagas-2152629641-33980309` |
| `too-dark.webp` (degraded copy) | Arina Krasnikova | `pexels-arina-krasnikova-5416635` |
| `family-floor.webp`, `eyes-closed.webp` | Arina Krasnikova | `pexels-arina-krasnikova-6338442` |
| `family-blurry.webp` (degraded copy) | Arina Krasnikova | `pexels-arina-krasnikova-6338498` |
| `wedding.webp` | Uday Ahir | `pexels-aumgraphy-39267906` |
| `pregnancy.webp` | Book Hut | `pexels-book-hut-440747141-32592246` |
| `pumpkin-a.webp` | Chris Shafer | `pexels-christafaari-14086496` |
| `pumpkin-b.webp`, `pumpkin-blurry.webp` | Chris Shafer | `pexels-christafaari-14086497` |
| `beach-sunset.webp` | Devyn Breslin | `pexels-devyn-breslin-368306288-15164846` |
| `golden-field.webp` | Dmitry Rodionov | `pexels-dmitry-rodionov-30660763` |
| `dish.webp` | Elizabeth Zernetska | `pexels-elizabeth-zernetska-86424040-9001223` |
| `table.webp` | Jessika Arraes | `pexels-jessikaarraes-39238777` |
| `road.webp` | Lorenzo Castellino | `pexels-lorenzo-castellino-61076802-39103195` |
| `horse.webp` | Maddy Freddie | `pexels-maddyfreddie-38132851` |
| `forest.webp` | mehmetakifarts | `pexels-mehmetakifarts-35596893` |

The number at the end of each original's name is its Pexels photo id: `https://www.pexels.com/photo/<id>/`.

## Voice-over

The welcome tour's voice-over (`voice/<language>/<step>.mp3`) was generated with [ElevenLabs](https://elevenlabs.io) (model Eleven v4) on a paid plan, which allows commercial use of the output. The voices come from the ElevenLabs voice library: Hope (English), Victoria (French), Cristina (Spanish), Carla Blum (German), Tiziana (Italian), Carla (Portuguese), Ruth (Dutch), Marta (Polish) and Malin (Swedish).

The lines are in `scripts/voice/narration.json`; `pnpm voices` (with `ELEVENLABS_API_KEY` set) generates again the lines that changed.
