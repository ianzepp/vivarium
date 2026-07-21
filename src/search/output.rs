use super::SearchResult;

pub struct SearchOutput<'a> {
    pub query: &'a str,
    pub folder: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    pub results: Vec<SearchResult>,
    pub total: usize,
    pub as_json: bool,
    pub count_only: bool,
}

/// Search result in JSON-friendly format.
#[must_use]
pub fn to_json_result(result: &SearchResult) -> serde_json::Value {
    serde_json::json!({
        "handle": result.handle,
        "message_id": result.message_id,
        "account": result.account,
        "local_role": result.local_role,
        "content_id": result.content_id,
        "date": result.date,
        "from": result.from,
        "subject": result.subject,
        "score": result.score,
        "lexical_score": result.lexical_score,
        "semantic_score": result.semantic_score,
        "chunk_id": result.chunk_id,
        "snippet": result.snippet,
        "citation": {
            "handle": result.handle,
            "message_id": result.message_id,
            "account": result.account,
            "local_role": result.local_role,
            "content_id": result.content_id,
            "source_type": "rfc5322",
        },
    })
}

pub fn print_search_output(output: SearchOutput<'_>) {
    if output.count_only {
        print_count(&output);
        return;
    }
    if output.as_json {
        print_json(output);
        return;
    }

    print_text_header(&output);
    for result in &output.results {
        println!(
            "  {}  {:<16}  {}  {}",
            result.handle, result.date, result.from, result.subject
        );
        if !result.snippet.is_empty() {
            println!("    snippet: {}", result.snippet);
        }
    }
}

fn print_json(output: SearchOutput<'_>) {
    let output = serde_json::json!({
        "query": output.query,
        "folder": output.folder,
        "total": output.total,
        "limit": output.limit,
        "offset": output.offset,
        "results": output.results.into_iter()
            .map(|r| to_json_result(&r))
            .collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
    );
}

fn print_text_header(output: &SearchOutput<'_>) {
    if let Some(folder) = output.folder {
        println!(
            "search: {} results for '{}' in {}",
            output.total, output.query, folder
        );
    } else {
        println!("search: {} results for '{}'", output.total, output.query);
    }
}

fn print_count(output: &SearchOutput<'_>) {
    if output.as_json {
        let output = serde_json::json!({
            "query": output.query,
            "folder": output.folder,
            "total": output.total,
        });
        println!("{output}");
    } else {
        println!("{}", output.total);
    }
}
