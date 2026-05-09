use crate::config::Config;

use super::EmbeddingOptions;

#[test]
fn embedding_options_require_config_without_cli_values() {
    let err = EmbeddingOptions::from_config(&Config::default()).unwrap_err();

    assert!(err.to_string().contains("defaults.embedding_provider"));
}

#[test]
fn embedding_options_allow_explicit_cli_values_without_config_defaults() {
    let options = EmbeddingOptions::from_values(
        &Config::default(),
        Some("ollama".into()),
        Some("model".into()),
        Some("http://example.test/api/embed".into()),
    )
    .unwrap();

    assert_eq!(options.provider, "ollama");
    assert_eq!(options.model, "model");
    assert_eq!(options.endpoint, "http://example.test/api/embed");
}
