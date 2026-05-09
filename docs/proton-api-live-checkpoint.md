# Proton API Live Checkpoint

This checkpoint is for the ignored agent Proton account environment and should avoid repeated fresh logins. Prefer a persistent temporary home so `proton-session.json` can be reused by `sync`.

## Environment

The agent account credentials are expected in local ignored environment files:

- `PROTON_USERNAME`
- `PROTON_PASSWORD`

Do not print either value. Do not commit session files, tokens, private keys, decrypted key material, or exported message bodies.

## Persistent Test Home

Use a repo-external home such as:

```sh
export VIVI_PROTON_CHECKPOINT_HOME=/tmp/vivi-proton-agent-checkpoint
mkdir -p "$VIVI_PROTON_CHECKPOINT_HOME/.config/vivarium"
```

Write config once:

```sh
cat > "$VIVI_PROTON_CHECKPOINT_HOME/.config/vivarium/config.toml" <<EOF
[defaults]
mail_root = "$VIVI_PROTON_CHECKPOINT_HOME/mail"
EOF
```

Write account config once:

```sh
cat > "$VIVI_PROTON_CHECKPOINT_HOME/.config/vivarium/accounts.toml" <<EOF
[[accounts]]
name = "agent-direct"
email = "$PROTON_USERNAME"
username = "$PROTON_USERNAME"
password_cmd = "printenv PROTON_PASSWORD"
provider = "proton-api"
storage_mode = "bodies"
imap_host = ""
smtp_host = ""
EOF
chmod 600 "$VIVI_PROTON_CHECKPOINT_HOME/.config/vivarium/accounts.toml"
```

## Login Policy

First try to reuse an existing session:

```sh
HOME="$VIVI_PROTON_CHECKPOINT_HOME" target/debug/vivi proton session-check --account agent-direct
```

Only run login when the session file is missing or refresh fails:

```sh
HOME="$VIVI_PROTON_CHECKPOINT_HOME" PROTON_PASSWORD="$PROTON_PASSWORD" \
  target/debug/vivi proton login --account agent-direct
```

If Proton returns `429 Too Many Requests`, stop and wait. Do not retry in a loop.

## Body Checkpoint

After a valid session exists:

```sh
HOME="$VIVI_PROTON_CHECKPOINT_HOME" PROTON_PASSWORD="$PROTON_PASSWORD" \
  target/debug/vivi sync --account agent-direct --limit 1
```

Then verify structurally without printing message contents:

```sh
HANDLE=$(HOME="$VIVI_PROTON_CHECKPOINT_HOME" \
  target/debug/vivi list inbox --account agent-direct --limit 1 | awk 'NR==2 {print $1}')

HOME="$VIVI_PROTON_CHECKPOINT_HOME" \
  target/debug/vivi export --account agent-direct --text "$HANDLE" > /tmp/vivi-phase5-export.txt

python3 - <<'PY'
from pathlib import Path
body = Path('/tmp/vivi-phase5-export.txt').read_bytes()
print(f"checkpoint_handle_present={bool(body)}")
print(f"checkpoint_export_bytes={len(body)}")
print(f"checkpoint_contains_pgp_marker={b'-----BEGIN PGP MESSAGE-----' in body}")
print(f"checkpoint_contains_decryption_error={b'X-Vivarium-Proton-Decryption-Error' in body}")
PY
```

The Phase 5 live checkpoint passes when exported text is non-empty, does not contain the armored PGP marker, and does not contain the local decryption-error marker.
