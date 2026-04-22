use super::types::{SourceStatus, SourceVerdict};
use crate::agent::types::{SourceReference, ToolEvidence};

/// Verify all sources from a structured answer against the actual policy documents.
/// Uses tool evidence (from Read/Grep calls) to determine precise line ranges.
#[tracing::instrument(
    name = "judge.verify_sources",
    skip(sources, tool_evidence),
    fields(source_count = sources.len())
)]
pub async fn verify_sources(
    resources_dir: &str,
    sources: &[SourceReference],
    tool_evidence: &ToolEvidence,
) -> Vec<SourceVerdict> {
    let mut verdicts = Vec::with_capacity(sources.len());
    for source in sources {
        let dir = resources_dir.to_string();
        let src = source.clone();
        // Look up tool evidence for this document
        let evidence_ranges = tool_evidence
            .accessed_ranges
            .get(&source.document)
            .cloned()
            .unwrap_or_default();
        let verdict = tokio::task::spawn_blocking(move || check_source(&dir, &src, &evidence_ranges))
            .await
            .unwrap_or_else(|_| SourceVerdict {
                document: source.document.clone(),
                section: source.section.clone(),
                line_range: source.line_range.clone(),
                status: SourceStatus::DocumentNotFound,
                detail: Some("Internal error during verification".to_string()),
            });
        verdicts.push(verdict);
    }
    verdicts
}

/// Check a single source against the actual document.
/// `evidence_ranges` are the (start, end) line ranges the agent actually Read/Grep'd.
fn check_source(
    resources_dir: &str,
    source: &SourceReference,
    evidence_ranges: &[(usize, usize)],
) -> SourceVerdict {
    let base = SourceVerdict {
        document: source.document.clone(),
        section: source.section.clone(),
        line_range: source.line_range.clone(),
        status: SourceStatus::Ok,
        detail: None,
    };

    // Path traversal guard: reject document names with path separators or parent refs
    if source.document.contains('/')
        || source.document.contains('\\')
        || source.document.contains("..")
    {
        return SourceVerdict {
            status: SourceStatus::DocumentNotFound,
            detail: Some("Invalid document name".to_string()),
            ..base
        };
    }

    let doc_path = format!("{}/{}", resources_dir, source.document);

    let content = match std::fs::read_to_string(&doc_path) {
        Ok(c) => c,
        Err(_) => {
            return SourceVerdict {
                status: SourceStatus::DocumentNotFound,
                detail: Some(format!("Document not found: {}", source.document)),
                ..base
            };
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // If there's a quote, deterministically find it in the document.
    // Strategy:
    //   1. If we have tool evidence (Read ranges), search ONLY within those ranges first.
    //      This is the most precise: the agent read these exact lines and cited from them.
    //   2. Fall back to full-document search if not found in evidence ranges.
    if let Some(quote) = &source.quote {
        // First: try to find the quote within tool-evidence ranges (narrowed search)
        if !evidence_ranges.is_empty() {
            let clamped_ranges: Vec<(usize, usize)> = evidence_ranges
                .iter()
                .map(|&(s, e)| (s.max(1), e.min(total_lines)))
                .filter(|(s, e)| s <= e)
                .collect();

            for &(range_start, range_end) in &clamped_ranges {
                // Extract the sub-slice of lines for this range (convert 1-indexed to 0-indexed)
                let sub_lines = &lines[range_start - 1..range_end];
                if let Some((rel_start, rel_end)) = find_quote_lines(sub_lines, quote) {
                    // Convert back to absolute 1-indexed line numbers
                    let abs_start = range_start + rel_start - 1;
                    let abs_end = range_start + rel_end - 1;
                    let computed_range = if abs_start == abs_end {
                        format!("{abs_start}")
                    } else {
                        format!("{abs_start}-{abs_end}")
                    };
                    tracing::debug!(
                        document = %source.document,
                        evidence_range = %format!("{range_start}-{range_end}"),
                        computed_range = %computed_range,
                        "Quote found within tool-evidence range"
                    );
                    return SourceVerdict {
                        line_range: Some(computed_range),
                        status: SourceStatus::Ok,
                        detail: Some("Line range from tool evidence".to_string()),
                        ..base
                    };
                }
            }
            // Not found in evidence ranges — fall through to full-document search
            tracing::debug!(
                document = %source.document,
                "Quote not found in tool-evidence ranges, trying full document"
            );
        }

        // Fallback: search the entire document
        match find_quote_lines(&lines, quote) {
            Some((start, end)) => {
                let computed_range = if start == end {
                    format!("{start}")
                } else {
                    format!("{start}-{end}")
                };
                return SourceVerdict {
                    line_range: Some(computed_range),
                    status: SourceStatus::Ok,
                    detail: None,
                    ..base
                };
            }
            None => {
                return SourceVerdict {
                    status: SourceStatus::QuoteMismatch,
                    detail: Some("Quote not found in document".to_string()),
                    ..base
                };
            }
        }
    }

    // No quote provided — if we have tool evidence, use the narrowest range
    if !evidence_ranges.is_empty() {
        let min_start = evidence_ranges.iter().map(|r| r.0.max(1)).min().unwrap_or(1);
        let max_end = evidence_ranges.iter().map(|r| r.1.min(total_lines)).max().unwrap_or(total_lines);
        let computed_range = if min_start == max_end {
            format!("{min_start}")
        } else {
            format!("{min_start}-{max_end}")
        };
        return SourceVerdict {
            line_range: Some(computed_range),
            status: SourceStatus::Ok,
            detail: Some("Line range from tool evidence (no quote)".to_string()),
            ..base
        };
    }

    // No quote, no tool evidence — just check that the document exists
    base
}

/// Find the line range (1-indexed, inclusive) where a quote best matches in the document.
/// Uses a sliding window approach with fuzzy matching.
fn find_quote_lines(lines: &[&str], quote: &str) -> Option<(usize, usize)> {
    let norm_quote = normalize(quote);
    if norm_quote.len() < 10 {
        // Very short quotes — can't reliably locate, just accept
        return Some((1, 1));
    }
    let quote_words: Vec<&str> = norm_quote.split_whitespace().collect();
    if quote_words.is_empty() {
        return Some((1, 1));
    }

    let total = lines.len();
    let mut best_ratio: f64 = 0.0;
    let mut best_range: Option<(usize, usize)> = None;

    // Try window sizes from 1 line up to min(quote_word_count * 2 / avg_words_per_line, 20)
    // to avoid O(n^2) blowup on large docs
    let max_window = 20.min(total);

    for window_size in 1..=max_window {
        for start_idx in 0..=(total.saturating_sub(window_size)) {
            let window_text = lines[start_idx..start_idx + window_size].join("\n");
            let norm_window = normalize(&window_text);

            // Quick check: if normalized window is much shorter than quote, skip
            if norm_window.len() < norm_quote.len() / 3 {
                continue;
            }

            let w_words: Vec<&str> = norm_window.split_whitespace().collect();
            let lcs = word_lcs_len(&w_words, &quote_words);
            let ratio = lcs as f64 / quote_words.len() as f64;

            if ratio > best_ratio {
                best_ratio = ratio;
                best_range = Some((start_idx + 1, start_idx + window_size)); // 1-indexed
            }

            // Perfect match — no need to keep searching
            if ratio >= 1.0 {
                return best_range;
            }
        }
    }

    if best_ratio >= 0.7 {
        best_range
    } else {
        None
    }
}

/// Parse "120-135" or "120" into (start, end), both inclusive, 1-indexed.
#[allow(dead_code)]
fn parse_line_range(s: &str) -> Option<(usize, usize)> {
    let s = s.trim();
    if let Some((a, b)) = s.split_once('-') {
        let start: usize = a.trim().parse().ok()?;
        let end: usize = b.trim().parse().ok()?;
        Some((start, end))
    } else {
        let n: usize = s.parse().ok()?;
        Some((n, n))
    }
}

/// Fuzzy match: normalize both strings, then check if they overlap sufficiently.
/// Uses word-level longest common subsequence to tolerate minor differences
/// from markdown artifacts, slight paraphrasing, or formatting issues.
#[allow(dead_code)]
fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let n = normalize(needle);
    // Very short quotes always pass — they'd match trivially and give false negatives
    if n.len() < 10 {
        return true;
    }
    let h = normalize(haystack);

    // First try exact substring (fast path)
    if h.contains(&n) {
        return true;
    }

    // Fall back to word-level LCS ratio
    let h_words: Vec<&str> = h.split_whitespace().collect();
    let n_words: Vec<&str> = n.split_whitespace().collect();
    if n_words.is_empty() {
        return true;
    }
    let lcs_len = word_lcs_len(&h_words, &n_words);
    let ratio = lcs_len as f64 / n_words.len() as f64;
    ratio >= 0.7
}

/// Length of the longest common subsequence of two word slices.
fn word_lcs_len(a: &[&str], b: &[&str]) -> usize {
    let m = a.len();
    let n = b.len();
    // Use single-row DP to save memory
    let mut prev = vec![0u16; n + 1];
    let mut curr = vec![0u16; n + 1];
    for i in 1..=m {
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    prev[n] as usize
}

fn normalize(s: &str) -> String {
    // Strip inline line-number prefixes (e.g. "1293: " or "42: ") that the
    // inline agent may include when copying quotes from numbered document lines.
    let stripped: String = s
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            // Match "digits: " at the start of a line
            if let Some(rest) = trimmed.split_once(": ") {
                if rest.0.chars().all(|c| c.is_ascii_digit()) && !rest.0.is_empty() {
                    return rest.1;
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");

    stripped
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '[' | ']' | '(' | ')'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_range_range() {
        assert_eq!(parse_line_range("120-135"), Some((120, 135)));
    }

    #[test]
    fn test_parse_line_range_single() {
        assert_eq!(parse_line_range("42"), Some((42, 42)));
    }

    #[test]
    fn test_parse_line_range_spaces() {
        assert_eq!(parse_line_range(" 120 - 135 "), Some((120, 135)));
    }

    #[test]
    fn test_parse_line_range_invalid() {
        assert_eq!(parse_line_range("abc"), None);
        assert_eq!(parse_line_range(""), None);
    }

    #[test]
    fn test_fuzzy_contains_exact() {
        assert!(fuzzy_contains(
            "De maximale hypotheek is 100% van de marktwaarde.",
            "De maximale hypotheek is 100% van de marktwaarde."
        ));
    }

    #[test]
    fn test_fuzzy_contains_markdown_stripped() {
        assert!(fuzzy_contains(
            "De **maximale** hypotheek is _100%_ van de marktwaarde.",
            "De maximale hypotheek is 100% van de marktwaarde."
        ));
    }

    #[test]
    fn test_fuzzy_contains_whitespace_normalized() {
        assert!(fuzzy_contains(
            "De maximale  hypotheek\n is 100% van\n  de marktwaarde.",
            "De maximale hypotheek is 100% van de marktwaarde."
        ));
    }

    #[test]
    fn test_fuzzy_contains_short_quote_always_passes() {
        assert!(fuzzy_contains("anything here", "ja"));
    }

    #[test]
    fn test_fuzzy_contains_no_match() {
        assert!(!fuzzy_contains(
            "De maximale hypotheek is 100% van de marktwaarde.",
            "Het minimale inkomen bedraagt vijftigduizend euro."
        ));
    }

    #[test]
    fn test_fuzzy_contains_partial_match() {
        // Claude might slightly rephrase or truncate — 70%+ word overlap should pass
        assert!(fuzzy_contains(
            "Ja, hierop zijn uitzonderingen mogelijk en kun je per mail aan ons voorleggen.",
            "Ja, hierop zijn uitzonderingen mogelijk en kun je per mail voorleggen."
        ));
    }

    #[test]
    fn test_fuzzy_contains_markdown_links_stripped() {
        // Document has markdown links like [text](url)
        assert!(fuzzy_contains(
            "kun je per mail aan ons voorleggen ([voorleggen@munthypotheken.nl](mailto:voorleggen@munthypotheken.nl)).",
            "kun je per mail aan ons voorleggen voorleggen@munthypotheken.nl."
        ));
    }

    #[test]
    fn test_fuzzy_contains_bold_merged() {
        // Document sometimes has bold text merged with next sentence without space
        assert!(fuzzy_contains(
            "**Mag de klant een verbouwing in eigen beheer doen?**Ja, dat is toegestaan.",
            "Mag de klant een verbouwing in eigen beheer doen? Ja, dat is toegestaan."
        ));
    }

    #[test]
    fn test_path_traversal_rejected() {
        let source = SourceReference {
            document: "../../etc/passwd".to_string(),
            section: "test".to_string(),
            quote: None,
            line_range: None,
        };
        let verdict = check_source("/tmp", &source, &[]);
        assert!(matches!(verdict.status, SourceStatus::DocumentNotFound));
    }

    #[test]
    fn test_backslash_rejected() {
        let source = SourceReference {
            document: "..\\etc\\passwd".to_string(),
            section: "test".to_string(),
            quote: None,
            line_range: None,
        };
        let verdict = check_source("/tmp", &source, &[]);
        assert!(matches!(verdict.status, SourceStatus::DocumentNotFound));
    }
}
