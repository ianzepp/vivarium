use super::{Runtime, VivariumError, print_sync_result};

impl Runtime {
    pub(crate) async fn sync(
        &self,
        account: Option<String>,
        limit: Option<usize>,
        since: Option<String>,
        before: Option<String>,
        reset: bool,
    ) -> Result<(), VivariumError> {
        validate_reset(reset, limit, since.as_deref(), before.as_deref())?;
        let window = vivarium::sync::SyncWindow::parse(since.as_deref(), before.as_deref())?;
        match self.selected_account_name(account) {
            Some(name) => {
                let acct = self.accounts.find_account(&name)?;
                if reset {
                    vivarium::sync::reset_account_cache(acct, &self.config)?;
                }
                let result =
                    vivarium::sync::sync_account(acct, &self.config, self.insecure, limit, window)
                        .await?;
                print_sync_result(&name, &result);
            }
            None => {
                for acct in &self.accounts.accounts {
                    if reset {
                        vivarium::sync::reset_account_cache(acct, &self.config)?;
                    }
                    let result = vivarium::sync::sync_account(
                        acct,
                        &self.config,
                        self.insecure,
                        limit,
                        window,
                    )
                    .await?;
                    print_sync_result(&acct.name, &result);
                }
            }
        }
        Ok(())
    }
}

fn validate_reset(
    reset: bool,
    limit: Option<usize>,
    since: Option<&str>,
    before: Option<&str>,
) -> Result<(), VivariumError> {
    if !reset {
        return Ok(());
    }
    if limit.is_some() || since.is_some() || before.is_some() {
        return Err(VivariumError::Config(
            "--reset cannot be combined with --limit, --since, or --before".into(),
        ));
    }
    Ok(())
}
