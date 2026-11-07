//! Natural-language search queries → English CLIP prompts.
//!
//! The CLIP text tower only understands English, while photographers type
//! "mariée bouquet" or "erster Tanz". A compact built-in glossary of the
//! vocabulary of weddings, events and portraits (French, Spanish, German,
//! Italian) translates the known words and phrases; unknown words (names,
//! English terms) pass through unchanged. Matching ignores case and accents.
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
    ("mairie", "town hall"),
    ("primer baile", "first dance"),
    ("puesta de sol", "sunset"),
    ("tarta nupcial", "wedding cake"),
    ("blanco y negro", "black and white"),
    ("dama de honor", "bridesmaid"),
    ("erster tanz", "first dance"),
    ("sonnenuntergang", "sunset"),
    ("schwarz weiss", "black and white"),
    ("primo ballo", "first dance"),
    ("torta nuziale", "wedding cake"),
    ("bianco e nero", "black and white"),
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
    ("robe", "dress"),
    ("voile", "veil"),
    ("costume", "suit"),
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
    ("chaussures", "shoes"),
    ("chaussure", "shoe"),
    ("maquillage", "makeup"),
    ("coiffure", "hairstyle"),
    ("decoration", "decoration"),
    ("bougies", "candles"),
    ("bougie", "candle"),
    ("ramo", "bouquet of flowers"),
    ("flores", "flowers"),
    ("vestido", "dress"),
    ("anillos", "rings"),
    ("anillo", "ring"),
    ("tarta", "cake"),
    ("coche", "car"),
    ("brautstrauss", "bridal bouquet"),
    ("blumen", "flowers"),
    ("kleid", "dress"),
    ("ringe", "rings"),
    ("ring", "ring"),
    ("torte", "cake"),
    ("kuchen", "cake"),
    ("auto", "car"),
    ("fiori", "flowers"),
    ("abito", "dress"),
    ("anelli", "rings"),
    ("anello", "ring"),
    ("torta", "cake"),
    ("macchina", "car"),
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
    ("iglesia", "church"),
    ("playa", "beach"),
    ("kirche", "church"),
    ("strand", "beach"),
    ("berge", "mountains"),
    ("garten", "garden"),
    ("schloss", "castle"),
    ("chiesa", "church"),
    ("spiaggia", "beach"),
    ("montagna", "mountain"),
    ("giardino", "garden"),
    ("castello", "castle"),
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
    ("risa", "laughing"),
    ("trauung", "wedding ceremony"),
    ("tanz", "dancing"),
    ("kuss", "kiss"),
    ("lachen", "laughing"),
    ("rede", "speech"),
    ("cerimonia", "ceremony"),
    ("ballo", "dancing"),
    ("bacio", "kiss"),
    ("risata", "laughing"),
    // ---- light & style ----------------------------------------------------
    ("nuit", "night"),
    ("soleil", "sun"),
    ("pluie", "rain"),
    ("neige", "snow"),
    ("contre-jour", "backlit"),
    ("silhouette", "silhouette"),
    ("noche", "night"),
    ("nacht", "night"),
    ("notte", "night"),
];

/// French/Spanish/German/Italian little words that carry no meaning for CLIP.
const STOP_WORDS: &[&str] = &[
    "le", "la", "les", "l", "un", "une", "des", "de", "du", "d", "et", "avec", "sur", "dans", "en", "au", "aux", "el",
    "los", "las", "una", "y", "con", "del", "der", "die", "das", "ein", "eine", "und", "mit", "im", "il", "lo", "gli",
    "e", "con", "nel", "della", "dei",
];

fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ß' => 's',
            '\'' | '’' | ',' | ';' | '.' | '!' | '?' | '"' | '«' | '»' | '(' | ')' => ' ',
            c => c,
        })
        .collect()
}

/// Keys that are also first names once accents are folded ("Marie"): they
/// only match when the photographer typed the accent ("marié").
const ACCENT_REQUIRED: &[&str] = &["marie", "maries"];

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
    let accented = |i: usize| originals.get(i).is_some_and(|w| w.contains('é'));
    let mut out: Vec<String> = Vec::new();
    let mut translated = false;
    let mut i = 0;
    'outer: while i < words.len() {
        // Longest glossary phrase starting at i (up to 4 words).
        for n in (1..=4.min(words.len() - i)).rev() {
            let phrase = words[i..i + n].join(" ");
            let ambiguous = n == 1 && ACCENT_REQUIRED.contains(&phrase.as_str()) && !accented(i);
            if let Some((_, en)) = GLOSSARY.iter().find(|(k, _)| *k == phrase).filter(|_| !ambiguous) {
                out.push((*en).to_string());
                translated = true;
                i += n;
                continue 'outer;
            }
        }
        if !STOP_WORDS.contains(&words[i]) {
            out.push(words[i].to_string());
        } else {
            translated = true;
        }
        i += 1;
    }
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
    fn keeps_english_and_names() {
        assert_eq!(to_clip_prompt("a bride holding flowers"), "a bride holding flowers");
        assert_eq!(to_clip_prompt("Léa et Thomas"), "a photo of lea, thomas");
        assert_eq!(to_clip_prompt("Marie et Paul"), "a photo of marie, paul");
        assert_eq!(to_clip_prompt("le marié"), "a photo of groom");
    }
}
