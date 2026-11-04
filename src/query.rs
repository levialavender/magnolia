use crate::db::FileEntry;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct QueryOptions {
    pub case_sensitive: bool,
    pub match_whole_word: bool,
    pub match_path: bool,
    pub regex: bool,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            match_whole_word: false,
            match_path: false,
            regex: false,
        }
    }
}

pub struct QueryEngine {
    raw_query: String,
    options: QueryOptions,
    terms: Vec<String>,
    regex_matcher: Option<Regex>,
}

impl QueryEngine {
    pub fn new(query_str: &str, options: QueryOptions) -> Self {
        let trimmed = query_str.trim();
        let mut regex_matcher = None;

        if options.regex && !trimmed.is_empty() {
            let re_str = if options.case_sensitive {
                trimmed.to_string()
            } else {
                format!("(?i){}", trimmed)
            };
            regex_matcher = Regex::new(&re_str).ok();
        }

        let terms: Vec<String> = if !options.regex {
            trimmed
                .split_whitespace()
                .map(|s| {
                    if options.case_sensitive {
                        s.to_string()
                    } else {
                        s.to_lowercase()
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        Self {
            raw_query: query_str.to_string(),
            options,
            terms,
            regex_matcher,
        }
    }

    pub fn matches(&self, entry: &FileEntry, folder_path: &str) -> bool {
        if self.raw_query.trim().is_empty() {
            return true;
        }

        if self.options.match_path {
            let target = if folder_path.is_empty() {
                entry.name.clone()
            } else if folder_path.ends_with('\\') || folder_path.ends_with('/') {
                format!("{}{}", folder_path, entry.name)
            } else {
                format!("{}\\{}", folder_path, entry.name)
            };
            self.test_target(&target)
        } else {
            self.test_target(&entry.name)
        }
    }

    fn test_target(&self, target: &str) -> bool {
        if self.options.regex {
            if let Some(ref re) = self.regex_matcher {
                return re.is_match(target);
            }
            return false;
        }

        let haystack = if self.options.case_sensitive {
            target.to_string()
        } else {
            target.to_lowercase()
        };

        for term in &self.terms {
            if term.starts_with('!') {
                let neg_term = &term[1..];
                if !neg_term.is_empty() && haystack.contains(neg_term) {
                    return false;
                }
            } else if term.contains('*') || term.contains('?') {
                if !wildcard_match(term, &haystack) {
                    return false;
                }
            } else if !haystack.contains(term) {
                return false;
            }
        }

        true
    }
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p_chars: Vec<char> = pattern.chars().collect();
    let t_chars: Vec<char> = text.chars().collect();
    let mut p = 0;
    let mut t = 0;
    let mut star_p = None;
    let mut match_t = 0;

    while t < t_chars.len() {
        if p < p_chars.len() && (p_chars[p] == '?' || p_chars[p] == t_chars[t]) {
            p += 1;
            t += 1;
        } else if p < p_chars.len() && p_chars[p] == '*' {
            star_p = Some(p);
            p += 1;
            match_t = t;
        } else if let Some(sp) = star_p {
            p = sp + 1;
            match_t += 1;
            t = match_t;
        } else {
            return false;
        }
    }

    while p < p_chars.len() && p_chars[p] == '*' {
        p += 1;
    }

    p == p_chars.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_search() {
        let q = QueryEngine::new("test", QueryOptions::default());
        let entry = FileEntry {
            id: 1,
            name: "My_Test_File.txt".to_string(),
            parent_id: 0,
            size: 10,
            is_dir: false,
            modified: 0,
        };
        assert!(q.matches(&entry, "C:\\"));
    }

    #[test]
    fn test_negation_search() {
        let q = QueryEngine::new("test !doc", QueryOptions::default());
        let entry1 = FileEntry {
            id: 1,
            name: "test.txt".to_string(),
            parent_id: 0,
            size: 10,
            is_dir: false,
            modified: 0,
        };
        let entry2 = FileEntry {
            id: 2,
            name: "test.doc".to_string(),
            parent_id: 0,
            size: 10,
            is_dir: false,
            modified: 0,
        };
        assert!(q.matches(&entry1, "C:\\"));
        assert!(!q.matches(&entry2, "C:\\"));
    }

    #[test]
    fn test_wildcard_search() {
        let q = QueryEngine::new("*.rs", QueryOptions::default());
        let entry1 = FileEntry {
            id: 1,
            name: "main.rs".to_string(),
            parent_id: 0,
            size: 10,
            is_dir: false,
            modified: 0,
        };
        let entry2 = FileEntry {
            id: 2,
            name: "main.c".to_string(),
            parent_id: 0,
            size: 10,
            is_dir: false,
            modified: 0,
        };
        assert!(q.matches(&entry1, "C:\\"));
        assert!(!q.matches(&entry2, "C:\\"));
    }
}
