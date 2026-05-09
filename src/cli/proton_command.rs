use std::path::PathBuf;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ProtonCommand {
    /// Fetch non-secret SRP auth bootstrap metadata
    AuthInfo {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Log in and store a reusable direct Proton API session
    Login {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// TOTP code for accounts that require one
        #[arg(long)]
        totp_code: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Fetch non-secret authenticated user and address metadata
    Identity {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Verify username/password login without storing returned tokens
    LoginCheck {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// TOTP code for accounts that require one
        #[arg(long)]
        totp_code: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Refresh and validate a stored direct Proton API session
    SessionCheck {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Capture one encrypted message and key material for local offline debugging
    #[command(hide = true)]
    CaptureFixture {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Proton message ID to capture; defaults to the newest listed message
        #[arg(long)]
        message_id: Option<String>,

        /// Sensitive local fixture path
        #[arg(long, default_value = "target/private/proton-fixtures/fixture.json")]
        output: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Decrypt a captured Proton fixture without contacting Proton
    #[command(hide = true)]
    DecryptFixture {
        /// Account whose password command unlocks the fixture
        #[arg(long)]
        account: Option<String>,

        /// Sensitive local fixture path
        #[arg(long, default_value = "target/private/proton-fixtures/fixture.json")]
        fixture: PathBuf,

        /// Optional path for decrypted body bytes
        #[arg(long)]
        output: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
