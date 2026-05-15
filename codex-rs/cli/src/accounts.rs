use clap::Parser;
use codex_core::config::Config;
use codex_login::load_auth_dot_json;
use codex_login::save_auth;
use codex_multi_account::AccountRecord;
use codex_multi_account::MultiAccountStore;
use codex_utils_cli::CliConfigOverrides;

#[derive(Debug, Parser)]
pub struct AccountsCommand {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub subcommand: AccountsSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum AccountsSubcommand {
    /// Save the current login as a named account.
    Add {
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List saved accounts and their last known usage.
    List,

    /// Make a saved account active immediately.
    Use {
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// Remove a saved account from the multi-account store.
    Remove {
        #[arg(value_name = "NAME")]
        name: String,
    },
}

pub async fn run_accounts(command: AccountsCommand) -> ! {
    let AccountsCommand {
        config_overrides,
        subcommand,
    } = command;
    let config = load_config_or_exit(config_overrides).await;
    let store = MultiAccountStore::new(&config.codex_home);

    match subcommand {
        AccountsSubcommand::Add { name } => {
            let auth = match load_auth_dot_json(
                &config.codex_home,
                config.cli_auth_credentials_store_mode,
            ) {
                Ok(Some(auth)) => auth,
                Ok(None) => {
                    eprintln!("No stored login found. Run `codexx login` first.");
                    std::process::exit(1);
                }
                Err(err) => {
                    eprintln!("Error reading current login: {err}");
                    std::process::exit(1);
                }
            };
            let auth = match serde_json::to_value(auth) {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("Error encoding current login: {err}");
                    std::process::exit(1);
                }
            };
            if let Err(err) = store.upsert_account(&name, auth) {
                eprintln!("Error saving account `{name}`: {err}");
                std::process::exit(1);
            }
            eprintln!("Saved account `{name}` and made it active");
            std::process::exit(0);
        }
        AccountsSubcommand::List => {
            let state = match store.load() {
                Ok(state) => state,
                Err(err) => {
                    eprintln!("Error reading accounts: {err}");
                    std::process::exit(1);
                }
            };
            if state.accounts.is_empty() {
                eprintln!("No saved accounts");
                std::process::exit(1);
            }
            for account in &state.accounts {
                print_account(account, state.active.as_deref());
            }
            std::process::exit(0);
        }
        AccountsSubcommand::Use { name } => {
            let account = match store.select_account(&name) {
                Ok(account) => account,
                Err(err) => {
                    eprintln!("Error selecting account `{name}`: {err}");
                    std::process::exit(1);
                }
            };
            let Some(account_auth) = account.auth else {
                eprintln!("Account `{name}` has no stored auth payload");
                std::process::exit(1);
            };
            let auth = match serde_json::from_value(account_auth) {
                Ok(auth) => auth,
                Err(err) => {
                    eprintln!("Error decoding account `{name}`: {err}");
                    std::process::exit(1);
                }
            };
            if let Err(err) = save_auth(
                &config.codex_home,
                &auth,
                config.cli_auth_credentials_store_mode,
            ) {
                eprintln!("Error activating account `{name}`: {err}");
                std::process::exit(1);
            }
            eprintln!("Activated account `{name}`");
            std::process::exit(0);
        }
        AccountsSubcommand::Remove { name } => match store.remove_account(&name) {
            Ok(true) => {
                eprintln!("Removed account `{name}`");
                std::process::exit(0);
            }
            Ok(false) => {
                eprintln!("Account `{name}` was not saved");
                std::process::exit(1);
            }
            Err(err) => {
                eprintln!("Error removing account `{name}`: {err}");
                std::process::exit(1);
            }
        },
    }
}

async fn load_config_or_exit(cli_config_overrides: CliConfigOverrides) -> Config {
    let cli_overrides = match cli_config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    match Config::load_with_cli_overrides(cli_overrides).await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error loading configuration: {e}");
            std::process::exit(1);
        }
    }
}

fn print_account(account: &AccountRecord, active: Option<&str>) {
    let marker = if active == Some(account.name.as_str()) {
        "*"
    } else {
        " "
    };
    let usage = account
        .last_rate_limits
        .as_ref()
        .and_then(|snapshot| snapshot.primary.as_ref())
        .map(|window| format!("{:.0}% used", window.used_percent))
        .unwrap_or_else(|| "usage unknown".to_string());
    let exhausted = account
        .exhausted_until
        .map(|timestamp| format!(", exhausted until {timestamp}"))
        .unwrap_or_default();
    eprintln!("{marker} {} - {usage}{exhausted}", account.name);
}
