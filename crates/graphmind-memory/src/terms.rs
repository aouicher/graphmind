//! Search terms: the tokenizer and stopword list memory search uses for queries and entries.

use crate::identifiers::looks_like_name;

/// Function words that carry no meaning in a search query, English then French. Prompt filler
/// such as "fix", "add" or "run" is not here: in a deliberate search those can be the topic.
const STOPWORDS: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "am", "an", "and", "any", "are", "as", "at", "be",
    "because", "been", "before", "both", "but", "by", "can", "could", "did", "do", "does", "doing",
    "done", "each", "else", "for", "from", "got", "had", "has", "have", "he", "her", "here",
    "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just", "let", "lets", "like",
    "may", "me", "might", "more", "most", "much", "must", "my", "no", "not", "now", "of",
    "ok", "on", "one", "only", "or", "other", "our", "out", "over", "please", "pls", "same", "she",
    "should", "so", "some", "still", "such", "than", "that", "the", "their", "them", "then", "there",
    "these", "they", "this", "those", "to", "too", "u", "ur", "up", "us", "very",
    "was", "way", "we", "were", "what", "when", "where", "which", "while", "who", "why",
    "will", "with", "would", "yes", "you", "your",
    // French
    "au", "aussi", "avec", "avoir", "bien", "ca", "ça", "ce", "comme", "dans", "de", "deja", "déjà", "des",
    "dois", "doit", "du", "elle", "en", "encore", "est", "et", "être", "fais", "faire", "fait", "ici", "il",
    "ils", "jamais", "je", "la", "le", "les", "leur", "ma", "mais", "maintenant", "mes", "moi",
    "mon", "ne", "non", "nous", "ont", "ou", "où", "oui", "par", "pas", "peu", "peut", "peux",
    "plus", "pour", "qu", "qui", "que", "rien", "sa", "se", "ses", "si", "son", "sont", "sur",
    "toujours", "tout", "trop", "tu", "un", "une", "veux", "vous",
];

/// A function word, or a hyphenated one made only of them (`est-ce`, `peut-être`).
pub(crate) fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word) || (word.contains('-') && word.split('-').all(|p| p.is_empty() || STOPWORDS.contains(&p)))
}

/// Split text into raw tokens: runs of characters that can appear in names and paths.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    let is_part = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '$');
    text.split(|c: char| !is_part(c))
        .map(|t| t.trim_matches(|c: char| matches!(c, '.' | ':' | '-' | '/' | '@' | '$')))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// One search term, lower-cased.
pub(crate) struct Term {
    pub(crate) text: String,
    /// Written like a name (path, camelCase, ENV_VAR, ...), judged on the token as typed.
    pub(crate) name: bool,
}

impl Term {
    /// Under 3 characters: matches whole words only.
    pub(crate) fn short(&self) -> bool {
        self.text.chars().count() < 3
    }

    /// Scoring this term needs the entry's tokens (whole-word matches and name matches).
    pub(crate) fn needs_tokens(&self) -> bool {
        self.name || self.short()
    }
}

/// The distinct query tokens of two or more characters that aren't stopwords, in query order.
pub(crate) fn query_terms(query: &str) -> Vec<Term> {
    let mut terms: Vec<Term> = Vec::new();
    for raw in tokens(query) {
        let text = raw.to_lowercase();
        if text.chars().count() < 2 || is_stopword(&text) {
            continue;
        }
        let name = looks_like_name(&raw);
        match terms.iter_mut().find(|t| t.text == text) {
            Some(seen) => seen.name |= name,
            None => terms.push(Term { text, name }),
        }
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_strip_surrounding_punctuation() {
        assert_eq!(tokens("(see `a/b.ts`), SOURCE={}."), ["see", "a/b.ts", "SOURCE"]);
    }

    #[test]
    fn query_terms_drop_english_and_french_function_words() {
        let terms: Vec<(String, bool)> = query_terms("Où est le fichier qui gère la retry-policy ? Is it fetchInBatches?")
            .into_iter()
            .map(|t| (t.text, t.name))
            .collect();
        assert_eq!(
            terms,
            [
                ("fichier".to_string(), false),
                ("gère".to_string(), false),
                ("retry-policy".to_string(), true),
                ("fetchinbatches".to_string(), true),
            ]
        );
    }

    #[test]
    fn french_elisions_and_hyphenated_function_words_are_stopwords() {
        let terms: Vec<String> = query_terms("qu'est-ce que la retry policy, peut-être mais par si").into_iter().map(|t| t.text).collect();
        assert_eq!(terms, ["retry", "policy"]);
    }

    #[test]
    fn prompt_filler_is_not_a_stopword() {
        let terms: Vec<String> = query_terms("get use using want need").into_iter().map(|t| t.text).collect();
        assert_eq!(terms, ["get", "use", "using", "want", "need"]);
    }
}
