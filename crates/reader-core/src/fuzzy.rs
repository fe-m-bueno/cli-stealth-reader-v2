//! fzf-style subsequence matching for pickers and command completion.
//!
//! Scoring mirrors v1: contiguous runs and word-start matches are rewarded, and
//! a spread-out match is lightly penalized. A query that is not a subsequence
//! has no score at all.

/// Score `text` against `query`, or `None` when `query` is not a subsequence.
///
/// An empty query scores zero so callers can treat "no filter" uniformly.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<i64> {
    Matcher::new(query).score(text)
}

/// A normalized query reusable across every row of one picker update.
struct Matcher {
    needle: Vec<char>,
}

impl Matcher {
    fn new(query: &str) -> Self {
        Self {
            needle: query.to_lowercase().chars().collect(),
        }
    }

    fn score(&self, text: &str) -> Option<i64> {
        if self.needle.is_empty() {
            return Some(0);
        }
        let lowered = text.to_lowercase();
        let mut haystack = lowered.chars().enumerate();

        let mut total: i64 = 0;
        let mut text_index = 0usize;
        let mut previous_char: Option<char> = None;
        // v1 seeded `previousMatch` at -2 so the first character can never look
        // contiguous with a preceding match.
        let mut previous_match: i64 = -2;
        for char in self.needle.iter().copied() {
            let (found, word_start) = loop {
                let (index, candidate) = haystack.next()?;
                let word_start = index == 0 || matches!(previous_char, Some(' ' | '-' | '_'));
                previous_char = Some(candidate);
                if candidate == char {
                    break (index, word_start);
                }
            };
            let found_index = found as i64;
            if found_index == previous_match + 1 {
                total += 5;
            }
            if word_start {
                total += 3;
            }
            total += 1;
            previous_match = found_index;
            text_index = found + 1;
        }

        let span = previous_match - (text_index as i64 - self.needle.len() as i64);
        Some(total - span.div_euclid(10))
    }
}

/// Filter and rank `items`, keeping the original order among equal scores.
///
/// An empty query returns every item untouched.
pub fn filter<T, F>(query: &str, items: Vec<T>, text_of: F) -> Vec<T>
where
    F: for<'a> Fn(&'a T) -> &'a str,
{
    if query.is_empty() {
        return items;
    }
    let matcher = Matcher::new(query);
    let mut scored: Vec<(usize, i64, T)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| {
            matcher
                .score(text_of(&item))
                .map(|value| (index, value, item))
        })
        .collect();
    scored.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::{filter, score};

    #[test]
    fn empty_query_matches_everything_with_a_zero_score() {
        assert_eq!(score("", "anything"), Some(0));
        let items = vec!["a", "b"];
        assert_eq!(filter("", items.clone(), |item| *item), items);
    }

    #[test]
    fn non_subsequence_has_no_score() {
        assert_eq!(score("xyz", "the quiet harbour"), None);
        assert_eq!(score("qq", "quiet"), None);
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(score("QUIET", "the Quiet harbour").is_some());
    }

    #[test]
    fn contiguous_and_word_start_matches_score_higher() {
        let contiguous = score("qui", "quiet").expect("subsequence");
        let scattered = score("qui", "q u i").expect("subsequence");
        assert!(
            contiguous > scattered,
            "{contiguous} should beat {scattered}"
        );

        // A word start is either index 0 or a character after a space, dash, or
        // underscore — being inside the first word is not enough.
        let word_start = score("h", "harbour bay").expect("subsequence");
        let mid_word = score("r", "harbour").expect("subsequence");
        assert!(word_start > mid_word, "{word_start} should beat {mid_word}");
        assert_eq!(score("h", "the harbour"), score("r", "harbour"));
    }

    #[test]
    fn ranking_prefers_better_matches_and_breaks_ties_by_input_order() {
        // A contiguous run scores the same wherever it sits, so `rep` inside a
        // longer title ties with the shorter one and input order decides. These
        // are the v1 scores: 16, 16, and 6.
        let items = vec!["rope.epub", "the quiet report.epub", "report.epub"];
        let ranked = filter("rep", items, |item| *item);
        assert_eq!(
            ranked,
            vec!["the quiet report.epub", "report.epub", "rope.epub"]
        );
        assert_eq!(score("rep", "report.epub"), Some(16));
        assert_eq!(score("rep", "rope.epub"), Some(6));
    }

    #[test]
    fn filtering_drops_items_that_do_not_match() {
        let items = vec!["dune.epub", "quiet.epub"];
        let ranked = filter("dune", items, |item| *item);
        assert_eq!(ranked, vec!["dune.epub"]);
    }

    #[test]
    fn ties_keep_the_input_order() {
        let items = vec!["ab-1", "ab-2", "ab-3"];
        let ranked = filter("ab", items.clone(), |item| *item);
        assert_eq!(ranked, items);
    }
}
