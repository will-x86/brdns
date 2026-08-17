//! `brdns-admin` CLI for managing accounts, rules, and upstreams
//! through the API.

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(
    name = "brdns-admin",
    version,
    about = "Manage brdns accounts, rules and upstreams"
)]
struct Cli {
    /// Management API base URL.
    #[arg(long, env = "BRDNS_ADMIN_URL", default_value = "http://127.0.0.1:8080")]
    url: String,

    /// Bearer token for the management API.
    #[arg(long, env = "BRDNS_ADMIN_TOKEN")]
    token: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Ping the API.
    Health,
    /// Create an account (prints the account number).
    Account {
        /// Optional account number; auto-generated when omitted.
        account_number: Option<String>,
    },
    /// Manage an account's rules.
    Rules {
        account: String,
        #[command(subcommand)]
        cmd: RulesCmd,
    },
    /// Manage an account's upstreams.
    Upstreams {
        account: String,
        #[command(subcommand)]
        cmd: UpstreamsCmd,
    },
    /// List preset upstreams.
    Presets,
}

#[derive(Subcommand, Debug)]
enum RulesCmd {
    /// List rules in order.
    List,
    /// Append a rule to the end of the list.
    Add {
        #[arg(long, value_enum)]
        action: ActionArg,
        #[arg(long, value_enum)]
        target: TargetArg,
        /// Domain, wildcard (`*.example.com`), or category name.
        value: String,
        /// Quota budget (for `--action limit`).
        #[arg(long)]
        limit: Option<i64>,
        /// Quota window (for `--action limit`).
        #[arg(long, value_enum)]
        window: Option<WindowArg>,
    },
    /// Replace the rule list from a JSON array (path or `-` for stdin).
    Set { file: String },
}

#[derive(Subcommand, Debug)]
enum UpstreamsCmd {
    List,
    /// Replace custom upstreams from a JSON array (path or `-` for stdin).
    Set {
        file: String,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ActionArg {
    Allow,
    Deny,
    Limit,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum TargetArg {
    Domain,
    Wildcard,
    Category,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum WindowArg {
    Hour,
    Day,
    Week,
    Month,
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn err(status: reqwest::StatusCode, body: &str) -> Box<dyn std::error::Error + Send + Sync> {
    format!("API error {status}: {body}").into()
}

async fn request(
    c: &reqwest::Client,
    url: &str,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut req = c
        .request(method.clone(), format!("{url}{path}"))
        .bearer_auth(token);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(err(status, &text));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("bad JSON from API: {e}").into())
}

fn print(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    if cli.token.is_empty() {
        return Err("no token: pass --token or set BRDNS_ADMIN_TOKEN".into());
    }
    let c = client();

    match cli.command {
        Command::Health => {
            let v = request(
                &c,
                &cli.url,
                &cli.token,
                reqwest::Method::GET,
                "/healthz",
                None,
            )
            .await?;
            print(&v);
        }
        Command::Account { account_number } => {
            let body = json!({ "account_number": account_number.unwrap_or_default() });
            let v = request(
                &c,
                &cli.url,
                &cli.token,
                reqwest::Method::POST,
                "/api/accounts",
                Some(body),
            )
            .await?;
            print(&v);
        }
        Command::Presets => {
            let v = request(
                &c,
                &cli.url,
                &cli.token,
                reqwest::Method::GET,
                "/api/upstreams/presets",
                None,
            )
            .await?;
            print(&v);
        }
        Command::Rules { account, cmd } => match cmd {
            RulesCmd::List => {
                let path = format!("/api/accounts/{account}/rules");
                let v =
                    request(&c, &cli.url, &cli.token, reqwest::Method::GET, &path, None).await?;
                print(&v);
            }
            RulesCmd::Add {
                action,
                target,
                value,
                limit,
                window,
            } => {
                let path = format!("/api/accounts/{account}/rules");
                // Read-modify-write: fetch current rules, append, replace.
                let current: Vec<Value> = serde_json::from_value(
                    request(&c, &cli.url, &cli.token, reqwest::Method::GET, &path, None).await?,
                )?;
                let mut rules: Vec<Value> = current
                    .into_iter()
                    .map(|r| {
                        json!({
                            "action": r["action"],
                            "target_type": r["target_type"],
                            "target_value": r["target_value"],
                            "limit_count": r["limit_count"],
                            "limit_window": r["limit_window"],
                            "enabled": r["enabled"],
                        })
                    })
                    .collect();
                rules.push(json!({
                    "action": action.to_string(),
                    "target_type": target.to_string(),
                    "target_value": value,
                    "limit_count": limit,
                    "limit_window": window.map(|w| w.to_string()),
                    "enabled": true,
                }));
                let v = request(
                    &c,
                    &cli.url,
                    &cli.token,
                    reqwest::Method::PUT,
                    &path,
                    Some(Value::Array(rules)),
                )
                .await?;
                print(&v);
            }
            RulesCmd::Set { file } => {
                let path = format!("/api/accounts/{account}/rules");
                let body = read_json(&file)?;
                let v = request(
                    &c,
                    &cli.url,
                    &cli.token,
                    reqwest::Method::PUT,
                    &path,
                    Some(body),
                )
                .await?;
                print(&v);
            }
        },
        Command::Upstreams { account, cmd } => match cmd {
            UpstreamsCmd::List => {
                let path = format!("/api/accounts/{account}/upstreams");
                let v =
                    request(&c, &cli.url, &cli.token, reqwest::Method::GET, &path, None).await?;
                print(&v);
            }
            UpstreamsCmd::Set { file } => {
                let path = format!("/api/accounts/{account}/upstreams");
                let body = read_json(&file)?;
                let v = request(
                    &c,
                    &cli.url,
                    &cli.token,
                    reqwest::Method::PUT,
                    &path,
                    Some(body),
                )
                .await?;
                print(&v);
            }
        },
    }

    Ok(())
}

fn read_json(file: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let text = if file == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(file)?
    };
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON in {file}: {e}").into())
}

impl std::fmt::Display for ActionArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Limit => "limit",
        })
    }
}

impl std::fmt::Display for TargetArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Domain => "domain",
            Self::Wildcard => "wildcard",
            Self::Category => "category",
        })
    }
}

impl std::fmt::Display for WindowArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        })
    }
}
