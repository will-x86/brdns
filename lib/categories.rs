//! In-memory domain to category.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// Normalize a domain: lowercase, strip a trailing dot.
pub fn normalize_domain(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

#[derive(Default)]
pub struct CategoryIndex {
    /// domain to set of categories it belongs to.
    map: RwLock<HashMap<String, HashSet<String>>>,
}

impl CategoryIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Is `qname` (or any ancestor) a member of `category`?
    pub fn contains(&self, qname: &str, category: &str) -> bool {
        let q = normalize_domain(qname);
        let map = self.map.read().expect("category index poisoned");
        let mut labels: Vec<&str> = q.split('.').filter(|l| !l.is_empty()).collect();
        while !labels.is_empty() {
            let domain = labels.join(".");
            if map.get(&domain).is_some_and(|cats| cats.contains(category)) {
                return true;
            }
            labels.remove(0);
        }
        false
    }

    /// Replace the whole index (after a blocklist refresh).
    pub fn replace(&self, map: HashMap<String, HashSet<String>>) {
        let mut guard = self.map.write().expect("category index poisoned");
        *guard = map;
    }

    pub fn len(&self) -> usize {
        self.map.read().expect("poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> CategoryIndex {
        let idx = CategoryIndex::new();
        idx.replace(HashMap::from([(
            "youtube.com".to_string(),
            HashSet::from(["youtube".to_string(), "video".to_string()]),
        )]));
        idx
    }

    #[test]
    fn exact_match() {
        assert!(index().contains("youtube.com", "youtube"));
    }

    #[test]
    fn subdomain_matches_category() {
        assert!(index().contains("www.youtube.com", "youtube"));
        assert!(!index().contains("i.ytimg.com", "video")); // not in index
    }

    #[test]
    fn unrelated_domain_does_not_match() {
        assert!(!index().contains("example.com", "youtube"));
    }

    #[test]
    fn case_and_trailing_dot_normalized() {
        assert!(index().contains("WWW.YouTube.com.", "video"));
    }
}
