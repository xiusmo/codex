use codex_protocol::protocol::RateLimitSnapshot;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use thiserror::Error;
use uuid::Uuid;

const STORE_RELATIVE_PATH: &[&str] = &["multi_accounts", "accounts.json"];
const AUTH_DIR_RELATIVE_PATH: &[&str] = &["multi_accounts", "auth"];

#[derive(Debug, Error)]
pub enum MultiAccountError {
    #[error("account name cannot be empty")]
    EmptyName,
    #[error("account `{0}` does not exist")]
    MissingAccount(String),
    #[error("account `{0}` has no stored auth payload")]
    MissingAuth(String),
    #[error("no stored accounts are available")]
    NoAccounts,
    #[error("all stored accounts are exhausted")]
    AllAccountsExhausted,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MultiAccountError>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AccountRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_selected_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhausted_until: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rate_limits: Option<RateLimitSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rate_limits_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct MultiAccountState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub accounts: Vec<AccountRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwitchOutcome {
    pub previous: String,
    pub selected: String,
    pub auth: Value,
}

#[derive(Clone, Debug)]
pub struct MultiAccountStore {
    path: PathBuf,
    auth_dir: PathBuf,
}

impl MultiAccountStore {
    pub fn new(codex_home: impl AsRef<Path>) -> Self {
        let mut path = codex_home.as_ref().to_path_buf();
        for segment in STORE_RELATIVE_PATH {
            path.push(segment);
        }
        let mut auth_dir = codex_home.as_ref().to_path_buf();
        for segment in AUTH_DIR_RELATIVE_PATH {
            auth_dir.push(segment);
        }
        Self { path, auth_dir }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<MultiAccountState> {
        let mut file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MultiAccountState::default());
            }
            Err(err) => return Err(err.into()),
        };
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(serde_json::from_str(&contents)?)
    }

    pub fn save(&self, state: &MultiAccountState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(state)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(&self.path)?;
        file.write_all(serialized.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    pub fn upsert_account(&self, name: &str, auth: Value) -> Result<()> {
        let name = normalize_name(name)?;
        let now = now_unix();
        let mut state = self.load()?;
        match state
            .accounts
            .iter_mut()
            .find(|account| account.name == name)
        {
            Some(account) => {
                let auth_file = self.write_account_auth(account.id.as_deref(), &auth)?;
                account.id = auth_id_from_file_name(&auth_file);
                account.auth_file = Some(auth_file);
                account.auth = None;
                account.exhausted_until = None;
                account.last_selected_at = Some(now);
            }
            None => {
                let auth_file = self.write_account_auth(None, &auth)?;
                state.accounts.push(AccountRecord {
                    id: auth_id_from_file_name(&auth_file),
                    name: name.clone(),
                    auth_file: Some(auth_file),
                    auth: None,
                    added_at: Some(now),
                    last_selected_at: Some(now),
                    exhausted_until: None,
                    last_rate_limits: None,
                    last_rate_limits_at: None,
                });
            }
        }
        state.active = Some(name);
        self.save(&state)
    }

    pub fn remove_account(&self, name: &str) -> Result<bool> {
        let name = normalize_name(name)?;
        let mut state = self.load()?;
        let before = state.accounts.len();
        let mut removed_auth_files = Vec::new();
        state.accounts.retain(|account| {
            if account.name == name {
                if let Some(auth_file) = account.auth_file.clone() {
                    removed_auth_files.push(auth_file);
                }
                false
            } else {
                true
            }
        });
        if state.active.as_deref() == Some(&name) {
            state.active = state.accounts.first().map(|account| account.name.clone());
        }
        let removed = state.accounts.len() != before;
        if removed {
            self.save(&state)?;
            for auth_file in removed_auth_files {
                let _ = std::fs::remove_file(self.auth_dir.join(auth_file));
            }
        }
        Ok(removed)
    }

    pub fn select_account(&self, name: &str) -> Result<AccountRecord> {
        let name = normalize_name(name)?;
        let mut state = self.load()?;
        let now = now_unix();
        let selected = state
            .accounts
            .iter_mut()
            .find(|account| account.name == name)
            .ok_or_else(|| MultiAccountError::MissingAccount(name.clone()))?;
        selected.last_selected_at = Some(now);
        selected.exhausted_until = None;
        let mut record = selected.clone();
        record.auth = Some(self.read_account_auth(&record)?);
        state.active = Some(name);
        self.save(&state)?;
        Ok(record)
    }

    pub fn record_active_rate_limits(&self, snapshot: RateLimitSnapshot) -> Result<()> {
        let mut state = self.load()?;
        let Some(active) = state.active.clone() else {
            return Ok(());
        };
        let Some(account) = state
            .accounts
            .iter_mut()
            .find(|account| account.name == active)
        else {
            return Ok(());
        };
        account.last_rate_limits = Some(snapshot);
        account.last_rate_limits_at = Some(now_unix());
        self.save(&state)
    }

    pub fn mark_active_exhausted_and_switch(
        &self,
        exhausted_until: Option<i64>,
        last_rate_limits: Option<RateLimitSnapshot>,
    ) -> Result<Option<SwitchOutcome>> {
        let mut state = self.load()?;
        if state.accounts.is_empty() {
            return Ok(None);
        }

        let active = state
            .active
            .clone()
            .or_else(|| state.accounts.first().map(|account| account.name.clone()))
            .ok_or(MultiAccountError::NoAccounts)?;
        let now = now_unix();
        let fallback_exhausted_until = now + 60 * 60;
        let exhausted_until = exhausted_until.unwrap_or(fallback_exhausted_until);

        if let Some(account) = state
            .accounts
            .iter_mut()
            .find(|account| account.name == active)
        {
            account.exhausted_until = Some(exhausted_until);
            if let Some(snapshot) = last_rate_limits {
                account.last_rate_limits = Some(snapshot);
                account.last_rate_limits_at = Some(now);
            }
        }

        let Some(next) = choose_next_available(&state.accounts, &active, now) else {
            self.save(&state)?;
            return Err(MultiAccountError::AllAccountsExhausted);
        };
        let selected_name = next.name.clone();
        let selected_auth = self.read_account_auth(next)?;
        if let Some(account) = state
            .accounts
            .iter_mut()
            .find(|account| account.name == selected_name)
        {
            account.last_selected_at = Some(now);
        }
        state.active = Some(selected_name.clone());
        self.save(&state)?;
        Ok(Some(SwitchOutcome {
            previous: active,
            selected: selected_name,
            auth: selected_auth,
        }))
    }

    fn write_account_auth(&self, id: Option<&str>, auth: &Value) -> Result<String> {
        std::fs::create_dir_all(&self.auth_dir)?;
        let id = id
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let file_name = format!("auth-{id}.json");
        let path = self.auth_dir.join(&file_name);
        let serialized = serde_json::to_string_pretty(auth)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(serialized.as_bytes())?;
        file.flush()?;
        Ok(file_name)
    }

    fn read_account_auth(&self, account: &AccountRecord) -> Result<Value> {
        if let Some(auth_file) = &account.auth_file {
            let mut file = std::fs::File::open(self.auth_dir.join(auth_file))?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            return Ok(serde_json::from_str(&contents)?);
        }
        account
            .auth
            .clone()
            .ok_or_else(|| MultiAccountError::MissingAuth(account.name.clone()))
    }
}

pub fn account_is_available(account: &AccountRecord, now: i64) -> bool {
    account.exhausted_until.is_none_or(|reset| reset <= now)
}

fn choose_next_available<'a>(
    accounts: &'a [AccountRecord],
    active: &str,
    now: i64,
) -> Option<&'a AccountRecord> {
    accounts
        .iter()
        .filter(|account| account.name != active)
        .find(|account| account_is_available(account, now))
}

fn normalize_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(MultiAccountError::EmptyName);
    }
    Ok(name.to_string())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn auth_id_from_file_name(file_name: &str) -> Option<String> {
    file_name
        .strip_prefix("auth-")
        .and_then(|value| value.strip_suffix(".json"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_to_next_available_account() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = MultiAccountStore::new(dir.path());
        store
            .upsert_account("one", serde_json::json!({"OPENAI_API_KEY": "one"}))
            .expect("add one");
        store
            .upsert_account("two", serde_json::json!({"OPENAI_API_KEY": "two"}))
            .expect("add two");
        store.select_account("one").expect("select one");

        let outcome = store
            .mark_active_exhausted_and_switch(Some(now_unix() + 10), None)
            .expect("switch")
            .expect("outcome");

        assert_eq!(outcome.previous, "one");
        assert_eq!(outcome.selected, "two");
        assert_eq!(store.load().expect("load").active.as_deref(), Some("two"));
    }

    #[test]
    fn reports_all_accounts_exhausted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = MultiAccountStore::new(dir.path());
        store
            .upsert_account("one", serde_json::json!({"OPENAI_API_KEY": "one"}))
            .expect("add one");

        let err = store
            .mark_active_exhausted_and_switch(Some(now_unix() + 10), None)
            .expect_err("expected exhaustion");

        assert!(matches!(err, MultiAccountError::AllAccountsExhausted));
    }
}
