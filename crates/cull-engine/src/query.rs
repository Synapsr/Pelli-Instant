//! Natural-language search queries → English CLIP prompts.
//!
//! The CLIP text tower only understands English, while photographers type
//! "mariée bouquet" or "erster Tanz". A compact built-in glossary of the
//! vocabulary of weddings, events and portraits (French, Spanish, German,
//! Italian), with colours, clothing and places, translates the known words
//! and phrases; unknown words (names, English terms) pass through unchanged.
//! Matching ignores case and accents. Colours are also matched inflected
//! ("bleues", "rotes") and placed before their noun, as CLIP reads them best:
//! "haut rouge" and "rotes Kleid" become "red top" and "red dress".
//!
//! A multilingual text encoder aligned with CLIP would remove the need for
//! this glossary (v2); the glossary costs nothing and covers the frequent
//! queries.

/// Phrases first (longest match wins), then single words.
const GLOSSARY: &[(&str, &str)] = &[
    // ---- multi-word phrases -------------------------------------------
    ("premiere danse", "first dance"),
    ("ouverture de bal", "first dance"),
    ("coucher de soleil", "sunset"),
    ("lever de soleil", "sunrise"),
    ("piece montee", "wedding cake"),
    ("gateau de mariage", "wedding cake"),
    ("vin d honneur", "cocktail reception"),
    ("echange des alliances", "exchange of wedding rings"),
    ("photo de groupe", "group photo"),
    ("photos de groupe", "group photo"),
    ("seance couple", "couple portrait"),
    ("demoiselle d honneur", "bridesmaid"),
    ("demoiselles d honneur", "bridesmaids"),
    ("garcon d honneur", "groomsman"),
    ("noir et blanc", "black and white"),
    ("lancer de riz", "rice toss"),
    ("lancer du bouquet", "bouquet toss"),
    ("robe de mariee", "wedding dress"),
    ("noeud papillon", "bow tie"),
    ("lunettes de soleil", "sunglasses"),
    ("hotel de ville", "town hall"),
    ("mairie", "town hall"),
    ("primer baile", "first dance"),
    ("puesta de sol", "sunset"),
    ("tarta nupcial", "wedding cake"),
    ("blanco y negro", "black and white"),
    ("dama de honor", "bridesmaid"),
    ("vestido de novia", "wedding dress"),
    ("gafas de sol", "sunglasses"),
    ("erster tanz", "first dance"),
    ("sonnenuntergang", "sunset"),
    ("schwarz weiss", "black and white"),
    ("primo ballo", "first dance"),
    ("torta nuziale", "wedding cake"),
    ("bianco e nero", "black and white"),
    ("abito da sposa", "wedding dress"),
    ("occhiali da sole", "sunglasses"),
    ("tramonto", "sunset"),
    // ---- people ---------------------------------------------------------
    ("mariee", "bride"),
    ("marie", "groom"),
    ("maries", "bride and groom"),
    ("couple", "couple"),
    ("enfant", "child"),
    ("enfants", "children"),
    ("bebe", "baby"),
    ("famille", "family"),
    ("invites", "guests"),
    ("invite", "guest"),
    ("pere", "father"),
    ("mere", "mother"),
    ("parents", "parents"),
    ("grand-mere", "grandmother"),
    ("grand-pere", "grandfather"),
    ("temoin", "best man"),
    ("temoins", "witnesses"),
    ("pretre", "priest"),
    ("homme", "man"),
    ("femme", "woman"),
    ("fille", "girl"),
    ("garcon", "boy"),
    ("amis", "friends"),
    ("chien", "dog"),
    ("perro", "dog"),
    ("hund", "dog"),
    ("cane", "dog"),
    ("neonato", "baby"),
    ("novia", "bride"),
    ("novio", "groom"),
    ("novios", "bride and groom"),
    ("nino", "child"),
    ("ninos", "children"),
    ("familia", "family"),
    ("braut", "bride"),
    ("brautigam", "groom"),
    ("brautpaar", "bride and groom"),
    ("kind", "child"),
    ("kinder", "children"),
    ("familie", "family"),
    ("gaste", "guests"),
    ("sposa", "bride"),
    ("sposo", "groom"),
    ("sposi", "bride and groom"),
    ("bambino", "child"),
    ("bambini", "children"),
    ("famiglia", "family"),
    ("ospiti", "guests"),
    // ---- things ---------------------------------------------------------
    ("bouquet", "bouquet of flowers"),
    ("fleurs", "flowers"),
    ("fleur", "flower"),
    ("alliance", "wedding ring"),
    ("alliances", "wedding rings"),
    ("bague", "ring"),
    ("bagues", "rings"),
    ("gateau", "cake"),
    ("champagne", "champagne"),
    ("verre", "glass"),
    ("verres", "glasses"),
    ("table", "table"),
    ("voiture", "car"),
    ("maquillage", "makeup"),
    ("coiffure", "hairstyle"),
    ("decoration", "decoration"),
    ("bougies", "candles"),
    ("bougie", "candle"),
    ("ramo", "bouquet of flowers"),
    ("flores", "flowers"),
    ("anillos", "rings"),
    ("anillo", "ring"),
    ("tarta", "cake"),
    ("coche", "car"),
    ("brautstrauss", "bridal bouquet"),
    ("blumen", "flowers"),
    ("ringe", "rings"),
    ("ring", "ring"),
    ("torte", "cake"),
    ("kuchen", "cake"),
    ("auto", "car"),
    ("fiori", "flowers"),
    ("anelli", "rings"),
    ("anello", "ring"),
    ("torta", "cake"),
    ("macchina", "car"),
    // ---- clothing -------------------------------------------------------
    ("robe", "dress"),
    ("voile", "veil"),
    ("costume", "suit"),
    ("chaussures", "shoes"),
    ("chaussure", "shoe"),
    ("haut", "top"),
    ("chemise", "shirt"),
    ("tee-shirt", "t-shirt"),
    ("veste", "jacket"),
    ("manteau", "coat"),
    ("pantalon", "trousers"),
    ("jupe", "skirt"),
    ("chapeau", "hat"),
    ("cravate", "tie"),
    ("lunettes", "glasses"),
    ("vestido", "dress"),
    ("camisa", "shirt"),
    ("camiseta", "t-shirt"),
    ("chaqueta", "jacket"),
    ("abrigo", "coat"),
    ("pantalones", "trousers"),
    ("falda", "skirt"),
    ("sombrero", "hat"),
    ("corbata", "tie"),
    ("pajarita", "bow tie"),
    ("gafas", "glasses"),
    ("velo", "veil"),
    ("zapatos", "shoes"),
    ("traje", "suit"),
    ("kleid", "dress"),
    ("brautkleid", "wedding dress"),
    ("oberteil", "top"),
    ("hemd", "shirt"),
    ("jacke", "jacket"),
    ("mantel", "coat"),
    ("hose", "trousers"),
    ("hut", "hat"),
    ("krawatte", "tie"),
    ("fliege", "bow tie"),
    ("brille", "glasses"),
    ("sonnenbrille", "sunglasses"),
    ("schleier", "veil"),
    ("schuhe", "shoes"),
    ("anzug", "suit"),
    ("abito", "dress"),
    ("vestito", "dress"),
    ("camicia", "shirt"),
    ("maglietta", "t-shirt"),
    ("giacca", "jacket"),
    ("cappotto", "coat"),
    ("pantaloni", "trousers"),
    ("gonna", "skirt"),
    ("cappello", "hat"),
    ("cravatta", "tie"),
    ("farfallino", "bow tie"),
    ("occhiali", "glasses"),
    ("scarpe", "shoes"),
    // ---- colours (also inflected, see `inflected_colour`) ---------------
    ("rouge", "red"),
    ("bleu", "blue"),
    ("vert", "green"),
    ("jaune", "yellow"),
    ("noir", "black"),
    ("blanc", "white"),
    ("blanche", "white"),
    ("violet", "purple"),
    ("violette", "purple"),
    ("gris", "gray"),
    ("marron", "brown"),
    ("brun", "brown"),
    ("dore", "gold"),
    ("argent", "silver"),
    ("argente", "silver"),
    ("rojo", "red"),
    ("roja", "red"),
    ("azul", "blue"),
    ("verde", "green"),
    ("amarillo", "yellow"),
    ("amarilla", "yellow"),
    ("negro", "black"),
    ("negra", "black"),
    ("blanco", "white"),
    ("blanca", "white"),
    ("rosa", "pink"),
    ("morado", "purple"),
    ("morada", "purple"),
    ("lila", "purple"),
    ("naranja", "orange"),
    ("dorado", "gold"),
    ("dorada", "gold"),
    ("plateado", "silver"),
    ("plateada", "silver"),
    ("rot", "red"),
    ("blau", "blue"),
    ("grun", "green"),
    ("gelb", "yellow"),
    ("schwarz", "black"),
    ("weiss", "white"),
    ("grau", "gray"),
    ("braun", "brown"),
    ("silber", "silver"),
    ("silbern", "silver"),
    ("rosso", "red"),
    ("rossa", "red"),
    ("blu", "blue"),
    ("azzurro", "blue"),
    ("azzurra", "blue"),
    ("giallo", "yellow"),
    ("gialla", "yellow"),
    ("nero", "black"),
    ("nera", "black"),
    ("bianco", "white"),
    ("bianca", "white"),
    ("viola", "purple"),
    ("arancione", "orange"),
    ("grigio", "gray"),
    ("grigia", "gray"),
    ("marrone", "brown"),
    ("oro", "gold"),
    ("argento", "silver"),
    // ---- places ---------------------------------------------------------
    ("eglise", "church"),
    ("plage", "beach"),
    ("mer", "sea"),
    ("montagne", "mountain"),
    ("foret", "forest"),
    ("jardin", "garden"),
    ("chateau", "castle"),
    ("salle", "hall"),
    ("ville", "city"),
    ("rue", "street"),
    ("champ", "field"),
    ("montagnes", "mountains"),
    ("parc", "park"),
    ("lac", "lake"),
    ("riviere", "river"),
    ("chapelle", "chapel"),
    ("scene", "stage"),
    ("iglesia", "church"),
    ("capilla", "chapel"),
    ("ayuntamiento", "town hall"),
    ("municipio", "town hall"),
    ("playa", "beach"),
    ("mar", "sea"),
    ("montana", "mountain"),
    ("bosque", "forest"),
    ("castillo", "castle"),
    ("parque", "park"),
    ("lago", "lake"),
    ("rio", "river"),
    ("calle", "street"),
    ("ciudad", "city"),
    ("campo", "field"),
    ("restaurante", "restaurant"),
    ("escenario", "stage"),
    ("kirche", "church"),
    ("kapelle", "chapel"),
    ("rathaus", "town hall"),
    ("standesamt", "town hall"),
    ("strand", "beach"),
    ("meer", "sea"),
    ("berg", "mountain"),
    ("berge", "mountains"),
    ("wald", "forest"),
    ("garten", "garden"),
    ("schloss", "castle"),
    ("burg", "castle"),
    ("see", "lake"),
    ("fluss", "river"),
    ("strasse", "street"),
    ("stadt", "city"),
    ("feld", "field"),
    ("buhne", "stage"),
    ("chiesa", "church"),
    ("cappella", "chapel"),
    ("spiaggia", "beach"),
    ("mare", "sea"),
    ("montagna", "mountain"),
    ("bosco", "forest"),
    ("giardino", "garden"),
    ("parco", "park"),
    ("castello", "castle"),
    ("fiume", "river"),
    ("strada", "street"),
    ("citta", "city"),
    ("ristorante", "restaurant"),
    ("palco", "stage"),
    // ---- moments & actions ------------------------------------------------
    ("ceremonie", "ceremony"),
    ("cortege", "wedding procession"),
    ("danse", "dancing"),
    ("danser", "dancing"),
    ("bisou", "kiss"),
    ("baiser", "kiss"),
    ("rire", "laughing"),
    ("rires", "laughing"),
    ("sourire", "smiling"),
    ("calin", "hug"),
    ("etreinte", "hug"),
    ("pleurs", "crying"),
    ("larmes", "tears"),
    ("emotion", "emotional moment"),
    ("discours", "speech"),
    ("toast", "toast"),
    ("repas", "dinner"),
    ("diner", "dinner"),
    ("soiree", "party"),
    ("fete", "party"),
    ("preparatifs", "getting ready"),
    ("habillage", "getting dressed"),
    ("portrait", "portrait"),
    ("groupe", "group of people"),
    ("ceremonia", "ceremony"),
    ("baile", "dancing"),
    ("beso", "kiss"),
    ("abrazo", "hug"),
    ("sonrisa", "smiling"),
    ("risa", "laughing"),
    ("trauung", "wedding ceremony"),
    ("tanz", "dancing"),
    ("kuss", "kiss"),
    ("umarmung", "hug"),
    ("lacheln", "smiling"),
    ("lachen", "laughing"),
    ("rede", "speech"),
    ("cerimonia", "ceremony"),
    ("ballo", "dancing"),
    ("bacio", "kiss"),
    ("abbraccio", "hug"),
    ("sorriso", "smiling"),
    ("risata", "laughing"),
    // ---- light & style ----------------------------------------------------
    ("nuit", "night"),
    ("soleil", "sun"),
    ("pluie", "rain"),
    ("neige", "snow"),
    ("contre-jour", "backlit"),
    ("silhouette", "silhouette"),
    ("noche", "night"),
    ("atardecer", "sunset"),
    ("nieve", "snow"),
    ("nacht", "night"),
    ("schnee", "snow"),
    ("notte", "night"),
    ("neve", "snow"),
];

/// French/Spanish/German/Italian little words that carry no meaning for CLIP.
const STOP_WORDS: &[&str] = &[
    "le", "la", "les", "l", "un", "une", "des", "de", "du", "d", "et", "avec", "sur", "dans", "en", "au", "aux", "el",
    "los", "las", "una", "y", "con", "del", "der", "die", "das", "ein", "eine", "und", "mit", "im", "il", "lo", "gli",
    "e", "con", "nel", "della", "dei",
];

/// English colours of the prompt ("rose" and "beige" are also French): each
/// goes right before its noun.
const COLOURS: &[&str] = &[
    "red", "blue", "green", "yellow", "black", "white", "pink", "rose", "purple", "orange", "gray", "grey", "brown",
    "gold", "silver", "beige",
];

/// German colours come before their noun ("rotes Kleid"); French, Spanish
/// and Italian ones after it ("robe rouge").
const COLOURS_BEFORE: &[&str] =
    &["rot", "blau", "grun", "gelb", "schwarz", "weiss", "grau", "braun", "silber", "silbern"];

fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' | 'ì' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            '\'' | '’' | ',' | ';' | '.' | '!' | '?' | '"' | '«' | '»' | '(' | ')' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Keys that are also first names or English words once accents are folded
/// ("Marie", "scene"): they only match when the photographer typed the
/// accent ("marié", "scène").
const ACCENT_REQUIRED: &[&str] = &["marie", "maries", "scene"];

/// Glossary entry of an inflected colour: "bleues" → "bleu", "rotes" → "rot",
/// "rojas" → "roja".
fn inflected_colour(word: &str) -> Option<(&'static str, &'static str)> {
    ["s", "e", "es", "en", "er", "em"].iter().find_map(|end| {
        let stem = word.strip_suffix(end)?;
        GLOSSARY.iter().copied().find(|&(key, en)| key == stem && COLOURS.contains(&en))
    })
}

/// Items of the prompt, each colour joined to its noun.
#[derive(Default)]
struct Items {
    list: Vec<String>,
    /// A colour waiting for the word after it.
    colour: Option<String>,
}

impl Items {
    fn push(&mut self, item: String, colour_first: bool) {
        let is_colour = |s: &str| COLOURS.contains(&s);
        if is_colour(&item) {
            match self.list.last_mut() {
                // "haut rouge" → "red top".
                Some(noun) if !colour_first && self.colour.is_none() && !is_colour(noun) => {
                    *noun = format!("{item} {noun}")
                }
                _ => {
                    self.list.extend(self.colour.take());
                    self.colour = Some(item);
                }
            }
        } else {
            // "rotes Kleid" → "red dress".
            let item = match self.colour.take() {
                Some(colour) => format!("{colour} {item}"),
                None => item,
            };
            self.list.push(item);
        }
    }

    fn finish(mut self) -> Vec<String> {
        self.list.extend(self.colour.take());
        self.list
    }
}

/// English prompt for CLIP. Returns the query unchanged when nothing was
/// recognised (already English, or names).
pub fn to_clip_prompt(query: &str) -> String {
    let folded = fold(query);
    let words: Vec<&str> = folded.split_whitespace().collect();
    let originals: Vec<String> = query
        .split(|c: char| c.is_whitespace() || "'’,;.!?\"«»()".contains(c))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    // Typed with an accent: folding changed the word.
    let accented = |i: usize| originals.get(i).is_some_and(|w| w.as_str() != words[i]);
    let mut items = Items::default();
    let mut translated = false;
    let mut i = 0;
    'outer: while i < words.len() {
        // Longest glossary phrase starting at i (up to 4 words).
        for n in (1..=4.min(words.len() - i)).rev() {
            let phrase = words[i..i + n].join(" ");
            if n == 1 && ACCENT_REQUIRED.contains(&phrase.as_str()) && !accented(i) {
                continue;
            }
            let entry = GLOSSARY.iter().copied().find(|&(key, _)| key == phrase);
            if let Some((key, en)) = entry.or_else(|| inflected_colour(&phrase)) {
                items.push(en.to_string(), COLOURS_BEFORE.contains(&key));
                translated = true;
                i += n;
                continue 'outer;
            }
        }
        if !STOP_WORDS.contains(&words[i]) {
            items.push(words[i].to_string(), false);
        } else {
            translated = true;
        }
        i += 1;
    }
    let out = items.finish();
    if !translated || out.is_empty() {
        return query.trim().to_string();
    }
    format!("a photo of {}", out.join(", "))
}

#[cfg(test)]
mod tests {
    use super::to_clip_prompt;

    #[test]
    fn translates_photographer_queries() {
        assert_eq!(to_clip_prompt("mariée bouquet"), "a photo of bride, bouquet of flowers");
        assert_eq!(to_clip_prompt("Première danse des mariés"), "a photo of first dance, bride and groom");
        assert_eq!(to_clip_prompt("gâteau"), "a photo of cake");
        assert_eq!(to_clip_prompt("erster Tanz"), "a photo of first dance");
        assert_eq!(to_clip_prompt("la novia con flores"), "a photo of bride, flowers");
        assert_eq!(to_clip_prompt("coucher de soleil sur la plage"), "a photo of sunset, beach");
    }

    #[test]
    fn colours_clothing_and_places() {
        assert_eq!(to_clip_prompt("haut rouge"), "a photo of red top");
        assert_eq!(to_clip_prompt("robe bleue"), "a photo of blue dress");
        assert_eq!(to_clip_prompt("église"), "a photo of church");
        assert_eq!(to_clip_prompt("plage coucher de soleil"), "a photo of beach, sunset");
        assert_eq!(to_clip_prompt("mariée en robe blanche"), "a photo of bride, white dress");
        assert_eq!(to_clip_prompt("nœud papillon noir"), "a photo of black bow tie");
        assert_eq!(to_clip_prompt("chaussures noires et costume beige"), "a photo of black shoes, beige suit");
        assert_eq!(to_clip_prompt("robe rose"), "a photo of rose dress");
        assert_eq!(to_clip_prompt("rouge"), "a photo of red");
        assert_eq!(to_clip_prompt("scène"), "a photo of stage");
        assert_eq!(to_clip_prompt("câlin dans la neige"), "a photo of hug, snow");
        assert_eq!(to_clip_prompt("vestido de novia"), "a photo of wedding dress");
        assert_eq!(to_clip_prompt("camisa blanca y corbata roja"), "a photo of white shirt, red tie");
        assert_eq!(to_clip_prompt("perro en la playa"), "a photo of dog, beach");
        assert_eq!(to_clip_prompt("rotes Kleid"), "a photo of red dress");
        assert_eq!(to_clip_prompt("Braut im weißen Kleid"), "a photo of bride, white dress");
        assert_eq!(to_clip_prompt("Brautstrauß"), "a photo of bridal bouquet");
        assert_eq!(to_clip_prompt("Standesamt"), "a photo of town hall");
        assert_eq!(to_clip_prompt("abito rosso"), "a photo of red dress");
        assert_eq!(to_clip_prompt("camicia bianca e cravatta nera"), "a photo of white shirt, black tie");
        assert_eq!(to_clip_prompt("sposa con il cane"), "a photo of bride, dog");
    }

    #[test]
    fn glossary_keys_are_folded_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (key, _) in super::GLOSSARY {
            assert_eq!(&super::fold(key), key, "{key}");
            assert!(seen.insert(key), "duplicate key {key}");
        }
    }

    #[test]
    fn keeps_english_and_names() {
        assert_eq!(to_clip_prompt("a bride holding flowers"), "a bride holding flowers");
        assert_eq!(to_clip_prompt("Léa et Thomas"), "a photo of lea, thomas");
        assert_eq!(to_clip_prompt("Marie et Paul"), "a photo of marie, paul");
        assert_eq!(to_clip_prompt("le marié"), "a photo of groom");
        assert_eq!(to_clip_prompt("red roses"), "red roses");
        assert_eq!(to_clip_prompt("night scene"), "night scene");
    }
}
