//! Sorted artist paging and A–Z jumps for `QueryArtistsPage`.

use api_models::common::ArtistPageMode;

/// First-letter bucket: A–Z (Unicode uppercase) or `#` for the rest.
pub fn letter_bucket(name: &str) -> char {
    match name.trim().chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().next().unwrap_or('#'),
        _ => '#',
    }
}

/// Index of the first artist on the requested page.
pub fn start_index(names: &[String], offset: usize, mode: ArtistPageMode) -> usize {
    if names.is_empty() {
        return 0;
    }
    match mode {
        ArtistPageMode::Offset => offset.min(names.len()),
        ArtistPageMode::NextLetter => next_letter_start(names, offset.min(names.len() - 1)),
        ArtistPageMode::PrevLetter => prev_letter_start(names, offset.min(names.len() - 1)),
    }
}

fn group_start(names: &[String], idx: usize) -> usize {
    let bucket = letter_bucket(&names[idx]);
    let mut i = idx;
    while i > 0 && letter_bucket(&names[i - 1]) == bucket {
        i -= 1;
    }
    i
}

fn next_letter_start(names: &[String], idx: usize) -> usize {
    let bucket = letter_bucket(&names[idx]);
    names
        .iter()
        .enumerate()
        .skip(idx + 1)
        .find(|(_, n)| letter_bucket(n) != bucket)
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn prev_letter_start(names: &[String], idx: usize) -> usize {
    let start = group_start(names, idx);
    if start == 0 {
        group_start(names, names.len() - 1)
    } else {
        group_start(names, start - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn letter_bucket_ascii_and_hash() {
        assert_eq!(letter_bucket("abba"), 'A');
        assert_eq!(letter_bucket("Beatles"), 'B');
        assert_eq!(letter_bucket("  zappa"), 'Z');
        assert_eq!(letter_bucket("12 Stones"), '#');
        assert_eq!(letter_bucket(""), '#');
    }

    #[test]
    fn offset_clamps_to_len() {
        let n = names(&["A", "B"]);
        assert_eq!(start_index(&n, 0, ArtistPageMode::Offset), 0);
        assert_eq!(start_index(&n, 2, ArtistPageMode::Offset), 2);
        assert_eq!(start_index(&[], 3, ArtistPageMode::Offset), 0);
    }

    #[test]
    fn next_and_prev_letter_wrap() {
        let n = names(&["Abba", "AC/DC", "Beatles", "Björk", "Zappa"]);
        assert_eq!(start_index(&n, 0, ArtistPageMode::NextLetter), 2);
        assert_eq!(start_index(&n, 2, ArtistPageMode::NextLetter), 4);
        assert_eq!(start_index(&n, 4, ArtistPageMode::NextLetter), 0);
        assert_eq!(start_index(&n, 2, ArtistPageMode::PrevLetter), 0);
        assert_eq!(start_index(&n, 0, ArtistPageMode::PrevLetter), 4);
    }
}
