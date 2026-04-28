use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::config::Account;
use crate::error::VivariumError;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_SCOPE: &str = "https://mail.google.com/";
const KEYCHAIN_SERVICE: &str = "vivarium-oauth";

#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn authorize(account: &Account, client: OAuthClient) -> Result<(), VivariumError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let auth_url = authorization_url(&client.client_id, &redirect_uri);

    open_browser(&auth_url)?;
    println!("opened browser for Google OAuth");
    println!("waiting for callback on {redirect_uri}");

    let code = wait_for_code(listener).await?;
    let response = exchange_code(&client, &redirect_uri, &code).await?;
    let refresh_token = response.refresh_token.ok_or_else(|| {
        VivariumError::Config(
            "Google did not return a refresh token; remove prior consent or rerun auth with a fresh consent prompt".into(),
        )
    })?;

    store_refresh_token(&account.name, &refresh_token)?;
    println!("stored OAuth refresh token for account '{}'", account.name);
    Ok(())
}

pub async fn print_access_token(
    account: &Account,
    client: OAuthClient,
) -> Result<(), VivariumError> {
    let refresh_token = load_refresh_token(&account.name)?;
    let response = refresh_access_token(&client, &refresh_token).await?;
    let access_token = response
        .access_token
        .ok_or_else(|| VivariumError::Config("Google token response had no access_token".into()))?;
    println!("{access_token}");
    Ok(())
}

pub fn oauth_client(
    account: &Account,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<OAuthClient, VivariumError> {
    let client_id = client_id
        .or_else(|| account.oauth_client_id.clone())
        .ok_or_else(|| missing_client_field(account, "oauth_client_id", "--client-id"))?;
    let client_secret = client_secret
        .or_else(|| account.oauth_client_secret.clone())
        .ok_or_else(|| missing_client_field(account, "oauth_client_secret", "--client-secret"))?;
    Ok(OAuthClient {
        client_id,
        client_secret,
    })
}

fn missing_client_field(account: &Account, field: &str, flag: &str) -> VivariumError {
    VivariumError::Config(format!(
        "account '{}' needs {field} in accounts.toml or {flag}",
        account.name
    ))
}

fn authorization_url(client_id: &str, redirect_uri: &str) -> String {
    format!(
        "{GOOGLE_AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        percent_encode(client_id),
        percent_encode(redirect_uri),
        percent_encode(GMAIL_SCOPE)
    )
}

async fn wait_for_code(listener: TcpListener) -> Result<String, VivariumError> {
    let (mut stream, _) = listener.accept().await?;
    let mut buffer = vec![0; 8192];
    let mut code = None::<String>;

    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Err(VivariumError::Config("OAuth callback connection closed unexpectedly".into()));
        }
        let request = String::from_utf8_lossy(&buffer[..n]);
        match parse_callback_code(&request) {
            Ok(c) => {
                code = Some(c);
                break;
            }
            Err(_) => {
                // Ignore non-callback requests (e.g., favicon.ico)
                continue;
            }
        }
    }

    let body = if code.is_some() {
        "Vivarium OAuth complete. You can close this tab."
    } else {
        "Vivarium OAuth failed. Return to the terminal for details."
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;

    code.ok_or_else(|| {
        VivariumError::Config("OAuth callback was closed before returning a code".into())
    })
}

fn parse_callback_code(request: &str) -> Result<String, VivariumError> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| VivariumError::Config("OAuth callback was empty".into()))?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| VivariumError::Config("OAuth callback request had no path".into()))?;
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .ok_or_else(|| VivariumError::Config("OAuth callback had no query string".into()))?;
    let params = parse_query(query);
    if let Some(error) = params.get("error") {
        return Err(VivariumError::Config(format!("OAuth failed: {error}")));
    }
    params
        .get("code")
        .cloned()
        .ok_or_else(|| VivariumError::Config("OAuth callback had no code".into()))
}

async fn exchange_code(
    client: &OAuthClient,
    redirect_uri: &str,
    code: &str,
) -> Result<TokenResponse, VivariumError> {
    token_request(&[
        ("client_id", client.client_id.as_str()),
        ("client_secret", client.client_secret.as_str()),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ])
    .await
}

async fn refresh_access_token(
    client: &OAuthClient,
    refresh_token: &str,
) -> Result<TokenResponse, VivariumError> {
    token_request(&[
        ("client_id", client.client_id.as_str()),
        ("client_secret", client.client_secret.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ])
    .await
}

async fn token_request(form: &[(&str, &str)]) -> Result<TokenResponse, VivariumError> {
    let response = reqwest::Client::new()
        .post(GOOGLE_TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|e| VivariumError::Config(format!("OAuth token request failed: {e}")))?;
    let status = response.status();
    let token = response
        .json::<TokenResponse>()
        .await
        .map_err(|e| VivariumError::Config(format!("OAuth token response was invalid: {e}")))?;
    if !status.is_success() {
        let reason = token
            .error_description
            .or(token.error)
            .unwrap_or_else(|| status.to_string());
        return Err(VivariumError::Config(format!(
            "OAuth token request failed: {reason}"
        )));
    }
    Ok(token)
}

fn store_refresh_token(account: &str, refresh_token: &str) -> Result<(), VivariumError> {
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            account,
            "-w",
            refresh_token,
            "-U",
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(VivariumError::Config(format!(
            "failed to store OAuth token in Keychain for account '{account}'"
        )))
    }
}

fn load_refresh_token(account: &str) -> Result<String, VivariumError> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            account,
            "-w",
        ])
        .output()?;
    if !output.status.success() {
        return Err(VivariumError::Config(format!(
            "no OAuth refresh token found in Keychain for account '{account}'; run `vivarium auth {account}` first"
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn open_browser(url: &str) -> Result<(), VivariumError> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener).arg(url).status()?;
    Ok(())
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((percent_decode(key), percent_decode(value)))
        })
        .collect()
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let mut output = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16)
        {
            output.push(hex);
            i += 3;
            continue;
        }
        output.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_callback_code() {
        let request = "GET /callback?code=abc%20123&scope=x HTTP/1.1\r\n\r\n";
        assert_eq!(parse_callback_code(request).unwrap(), "abc 123");
    }

    #[test]
    fn parses_callback_error() {
        let request = "GET /callback?error=access_denied HTTP/1.1\r\n\r\n";
        assert!(
            parse_callback_code(request)
                .unwrap_err()
                .to_string()
                .contains("access_denied")
        );
    }

    #[test]
    fn encodes_google_scope() {
        assert_eq!(
            percent_encode("https://mail.google.com/"),
            "https%3A%2F%2Fmail.google.com%2F"
        );
    }
}
