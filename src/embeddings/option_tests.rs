use crate::config::Config;

use super::EmbeddingOptions;

#[test]
fn embedding_options_require_config_without_cli_values() {
    let err = EmbeddingOptions::from_config(&Config::default()).unwrap_err();

    assert!(err.to_string().contains("defaults.embedding_provider"));
}

#[test]
fn embedding_options_allow_explicit_cli_values_without_config_defaults() {
    let provider = "ollama".to_string();
    let model = "model".to_string();
    let endpoint = "http://example.test/api/embed".to_string();
    let options = EmbeddingOptions::from_values(
        &Config::default(),
        Some(&provider),
        Some(&model),
        Some(&endpoint),
    )
    .unwrap();

    assert_eq!(options.provider, "ollama");
    assert_eq!(options.model, "model");
    assert_eq!(options.endpoint, "http://example.test/api/embed");
}
