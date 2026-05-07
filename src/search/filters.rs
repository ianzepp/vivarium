use super::SearchResult;

#[derive(Debug, Clone, Copy, Default)]
pub struct SearchFilters<'a> {
    pub folder: Option<&'a str>,
    pub from_addr: Option<&'a str>,
    pub from_domain: Option<&'a str>,
}

impl<'a> SearchFilters<'a> {
    pub fn new(
        folder: Option<&'a str>,
        from_addr: Option<&'a str>,
        from_domain: Option<&'a str>,
    ) -> Option<Self> {
        let filters = Self {
            folder,
            from_addr: clean_filter(from_addr),
            from_domain: clean_filter(from_domain),
        };
        if filters.folder.is_none() && filters.from_addr.is_none() && filters.from_domain.is_none()
        {
            None
        } else {
            Some(filters)
        }
    }
}

pub(crate) fn filter_results(
    results: Vec<SearchResult>,
    filters: Option<SearchFilters<'_>>,
) -> Vec<SearchResult> {
    let Some(filters) = filters else {
        return results;
    };
    results
        .into_iter()
        .filter(|result| matches_filters(result, filters))
        .collect()
}

fn clean_filter(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn matches_filters(result: &SearchResult, filters: SearchFilters<'_>) -> bool {
    if let Some(folder) = filters.folder
        && !result.local_role.eq_ignore_ascii_case(folder)
    {
        return false;
    }
    if let Some(from_addr) = filters.from_addr
        && !contains_case_insensitive(&result.from, from_addr)
    {
        return false;
    }
    if let Some(domain) = filters.from_domain
        && !matches_sender_domain(&result.from, domain)
    {
        return false;
    }
    true
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn matches_sender_domain(from: &str, domain: &str) -> bool {
    let domain = domain.trim().trim_start_matches('@').to_ascii_lowercase();
    if domain.is_empty() {
        return true;
    }
    from.to_ascii_lowercase()
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '<' | '>' | ',' | ';'))
        .any(|part| {
            part.rsplit_once('@')
                .is_some_and(|(_, sender_domain)| sender_domain.trim_matches('"') == domain)
        })
}
