use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use vivarium::config::Auth;
use vivarium::proton_decrypt::ProtonBodyDecryptor;
use vivarium::{VivariumError, proton_api};

use super::Runtime;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Deserialize, Serialize)]
struct ProtonOfflineFixture {
    sensitive: bool,
    account: String,
    username: String,
    captured_at: DateTime<Utc>,
    message: proton_api::ProtonFullMessage,
    key_material: Option<proton_api::ProtonKeyMaterial>,
    key_material_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct CaptureFixtureReport {
    account: String,
    username: String,
    output: String,
    message_id: String,
    subject: String,
    key_material: Option<KeyMaterialCountReport>,
    key_material_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct KeyMaterialCountReport {
    user_keys: usize,
    address_keys: usize,
    key_salts: usize,
}

#[derive(Debug, Serialize)]
struct DecryptFixtureReport {
    account: String,
    username: String,
    fixture: String,
    message_id: String,
    subject: String,
    bytes: usize,
    output: Option<String>,
}

impl Runtime {
    pub(crate) async fn proton_capture_fixture(
        &self,
        account: Option<String>,
        message_id: Option<&str>,
        output: PathBuf,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        let mail_root = acct.mail_path(&self.config);
        let store = proton_api::ProtonSessionStore::new(&mail_root);
        let mut session = store.load()?;
        let client = proton_api::ProtonApiClient::default();

        let selected_id = select_fixture_message_id(&client, &mut session, message_id).await?;
        let (refreshed, message) = client.fetch_message(&session, &selected_id).await?;
        session = refreshed;
        let mut key_material_error = None;
        let key_material = match client.key_material(&session).await {
            Ok((refreshed, material)) => {
                session = refreshed;
                Some(material)
            }
            Err(err) => {
                key_material_error = Some(err.to_string());
                None
            }
        };
        store.save(&session)?;

        let report_counts = key_material.as_ref().map(key_material_counts);
        let fixture = ProtonOfflineFixture {
            sensitive: true,
            account: acct.name.clone(),
            username: acct.username.clone(),
            captured_at: Utc::now(),
            message,
            key_material,
            key_material_error: key_material_error.clone(),
        };
        write_sensitive_json(&output, &fixture)?;

        let report = CaptureFixtureReport {
            account: acct.name,
            username: acct.username,
            output: output.display().to_string(),
            message_id: fixture.message.metadata.id,
            subject: fixture.message.metadata.subject,
            key_material: report_counts,
            key_material_error,
        };
        super::proton_api_command::print_report(&report, as_json, |report| {
            println!("Vivi Proton API fixture captured: {}", report.account);
            println!("username  {}", report.username);
            println!("path      {}", report.output);
            println!("message   {}", report.message_id);
            println!("subject   {}", report.subject);
            if let Some(keys) = &report.key_material {
                println!(
                    "keys      user={} address={} salts={}",
                    keys.user_keys, keys.address_keys, keys.key_salts
                );
            } else if let Some(error) = &report.key_material_error {
                println!("keys      unavailable ({error})");
            }
        });
        Ok(())
    }

    pub(crate) async fn proton_decrypt_fixture(
        &self,
        account: Option<String>,
        fixture: PathBuf,
        output: Option<PathBuf>,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_proton_api_account(account)?;
        if acct.auth != Auth::Password {
            return Err(VivariumError::Config(format!(
                "account '{}' uses auth = \"{}\"; Proton fixture decryption requires auth = \"password\"",
                acct.name, acct.auth
            )));
        }
        let fixture_data = fs::read_to_string(&fixture)?;
        let fixture_data: ProtonOfflineFixture = serde_json::from_str(&fixture_data)
            .map_err(|e| VivariumError::Other(format!("Proton fixture parse failed: {e}")))?;
        let password = acct.resolve_secret().await?;
        let key_material = fixture_data.key_material.as_ref().ok_or_else(|| {
            VivariumError::Other(
                "Proton fixture does not include key material; capture a fixture with locked scope before offline decryption".into(),
            )
        })?;
        let decryptor = ProtonBodyDecryptor::new(&password, key_material)?;
        let body = decryptor.decrypt_body(&fixture_data.message.body)?;

        if let Some(path) = output.as_ref() {
            write_sensitive_bytes(path, &body)?;
        }

        let report = DecryptFixtureReport {
            account: acct.name,
            username: acct.username,
            fixture: fixture.display().to_string(),
            message_id: fixture_data.message.metadata.id,
            subject: fixture_data.message.metadata.subject,
            bytes: body.len(),
            output: output.map(|path| path.display().to_string()),
        };
        super::proton_api_command::print_report(&report, as_json, |report| {
            println!("Vivi Proton API fixture decrypted: {}", report.account);
            println!("username  {}", report.username);
            println!("fixture   {}", report.fixture);
            println!("message   {}", report.message_id);
            println!("subject   {}", report.subject);
            println!("bytes     {}", report.bytes);
            if let Some(output) = &report.output {
                println!("output    {output}");
            }
        });
        Ok(())
    }
}

async fn select_fixture_message_id(
    client: &proton_api::ProtonApiClient,
    session: &mut proton_api::ProtonSession,
    message_id: Option<&str>,
) -> Result<String, VivariumError> {
    if let Some(id) = message_id {
        return Ok(id.to_string());
    }
    let (refreshed, messages, _) = client.list_messages(session, 0, 1).await?;
    *session = refreshed;
    messages
        .first()
        .map(|message| message.id.clone())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| VivariumError::Other("Proton account has no messages to capture".into()))
}

fn key_material_counts(key_material: &proton_api::ProtonKeyMaterial) -> KeyMaterialCountReport {
    KeyMaterialCountReport {
        user_keys: key_material.user_keys.len(),
        address_keys: key_material.address_keys.len(),
        key_salts: key_material.key_salts.len(),
    }
}

fn write_sensitive_json<T: Serialize>(path: &Path, value: &T) -> Result<(), VivariumError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| VivariumError::Other(format!("Proton fixture serialization failed: {e}")))?;
    write_sensitive_bytes(path, &bytes)
}

fn write_sensitive_bytes(path: &Path, bytes: &[u8]) -> Result<(), VivariumError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    Ok(())
}
