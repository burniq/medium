use anyhow::{Context, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub version: u32,
    pub control_url: String,
    pub bootstrap_token: String,
}

pub fn parse_invite(raw: &str) -> anyhow::Result<Invite> {
    let (scheme, remainder) = raw
        .split_once("://")
        .context("invite must include a scheme")?;
    if scheme != "medium" {
        bail!("unsupported invite scheme {scheme}");
    }

    let (path, query) = remainder
        .split_once('?')
        .context("invite must include query parameters")?;
    if path != "join" {
        bail!("unsupported invite path {path}");
    }

    let mut version = None;
    let mut control_url = None;
    let mut bootstrap_token = None;

    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .with_context(|| format!("invalid invite parameter {pair}"))?;

        match key {
            "v" => {
                version = Some(
                    value
                        .parse()
                        .with_context(|| format!("invalid invite version {value}"))?,
                );
            }
            "control" => {
                if value.is_empty() {
                    bail!("invite control URL cannot be empty");
                }
                control_url = Some(value.to_string());
            }
            "token" => {
                if value.is_empty() {
                    bail!("invite bootstrap token cannot be empty");
                }
                bootstrap_token = Some(value.to_string());
            }
            _ => {}
        }
    }

    Ok(Invite {
        version: version.context("invite is missing version")?,
        control_url: control_url.context("invite is missing control URL")?,
        bootstrap_token: bootstrap_token.context("invite is missing bootstrap token")?,
    })
}
