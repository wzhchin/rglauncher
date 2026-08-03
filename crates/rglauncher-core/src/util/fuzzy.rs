//! Shared fuzzy matching via `nucleo-matcher` (same engine as Zed/Helix).

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Score `haystack` against a fuzzy `pattern`. Returns `None` if no match.
#[inline]
pub fn fuzzy_score(matcher: &mut Matcher, pattern: &Atom, haystack: &str, buf: &mut Vec<char>) -> Option<u16> {
    pattern.score(Utf32Str::new(haystack, buf), matcher)
}

/// Best fuzzy score across multiple haystacks.
pub fn fuzzy_score_best(
    matcher: &mut Matcher,
    pattern: &Atom,
    haystacks: &[&str],
    buf: &mut Vec<char>,
) -> Option<u16> {
    let mut best = None;
    for h in haystacks {
        if h.is_empty() {
            continue;
        }
        if let Some(s) = fuzzy_score(matcher, pattern, h, buf) {
            best = Some(best.map_or(s, |b: u16| b.max(s)));
        }
    }
    best
}

/// Build a smart-case fuzzy atom for the given query.
pub fn fuzzy_atom(query: &str) -> Atom {
    Atom::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    )
}

/// Default matcher (non-path).
pub fn new_matcher() -> Matcher {
    Matcher::new(Config::DEFAULT)
}
