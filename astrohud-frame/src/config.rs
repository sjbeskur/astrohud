use crate::BoxError;
use std::env;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_SERVER: &str = "http://127.0.0.1:8080";
const DEFAULT_FRAME_ID: &str = "demo-frame";
const DEFAULT_CACHE_MIB: u64 = 1024;
const DEFAULT_SYNC_SECONDS: u64 = 5;
const DEFAULT_SLIDE_SECONDS: u64 = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub server_url: String,
    pub frame_id: String,
    pub cache_dir: PathBuf,
    pub cache_limit_bytes: u64,
    pub sync_interval: Duration,
    pub slide_interval: Duration,
    pub windowed: bool,
}

impl Config {
    pub fn from_env_and_args() -> Result<Self, BoxError> {
        let defaults = Self {
            server_url: env::var("ASTROHUD_SERVER_URL")
                .unwrap_or_else(|_| DEFAULT_SERVER.to_owned()),
            frame_id: env::var("ASTROHUD_FRAME_ID").unwrap_or_else(|_| DEFAULT_FRAME_ID.to_owned()),
            cache_dir: env::var_os("ASTROHUD_FRAME_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("astrohud-frame-data")),
            cache_limit_bytes: parse_env_u64("ASTROHUD_CACHE_MIB", DEFAULT_CACHE_MIB)?
                .checked_mul(1024 * 1024)
                .ok_or("ASTROHUD_CACHE_MIB is too large")?,
            sync_interval: Duration::from_secs(parse_env_u64(
                "ASTROHUD_SYNC_SECONDS",
                DEFAULT_SYNC_SECONDS,
            )?),
            slide_interval: Duration::from_secs(parse_env_u64(
                "ASTROHUD_SLIDE_SECONDS",
                DEFAULT_SLIDE_SECONDS,
            )?),
            windowed: false,
        };

        Self::parse_args(defaults, env::args().skip(1))
    }

    pub fn parse_args<I, S>(mut config: Self, args: I) -> Result<Self, BoxError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--server" => config.server_url = required_value(&mut args, "--server")?,
                "--frame" => config.frame_id = required_value(&mut args, "--frame")?,
                "--cache-dir" => {
                    config.cache_dir = PathBuf::from(required_value(&mut args, "--cache-dir")?)
                }
                "--cache-mib" => {
                    config.cache_limit_bytes =
                        parse_positive(&required_value(&mut args, "--cache-mib")?, "--cache-mib")?
                            .checked_mul(1024 * 1024)
                            .ok_or("--cache-mib is too large")?;
                }
                "--sync-seconds" => {
                    config.sync_interval = Duration::from_secs(parse_positive(
                        &required_value(&mut args, "--sync-seconds")?,
                        "--sync-seconds",
                    )?);
                }
                "--slide-seconds" => {
                    config.slide_interval = Duration::from_secs(parse_positive(
                        &required_value(&mut args, "--slide-seconds")?,
                        "--slide-seconds",
                    )?);
                }
                "--windowed" => config.windowed = true,
                "--help" | "-h" => return Err(usage().into()),
                other => return Err(format!("unknown argument: {other}\n\n{}", usage()).into()),
            }
        }

        config.server_url = config.server_url.trim_end_matches('/').to_owned();
        if config.server_url.is_empty() {
            return Err("server URL cannot be empty".into());
        }
        if config.frame_id.is_empty() {
            return Err("frame ID cannot be empty".into());
        }
        Ok(config)
    }
}

fn required_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, BoxError> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn parse_positive(value: &str, name: &str) -> Result<u64, BoxError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(parsed)
}

fn parse_env_u64(name: &str, default: u64) -> Result<u64, BoxError> {
    match env::var(name) {
        Ok(value) => parse_positive(&value, name),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

pub fn usage() -> &'static str {
    "Usage: astrohud-frame [options]\n\
     \n\
     Options:\n\
       --server URL          AstroHUD service URL\n\
       --frame ID            Frame identity (default: demo-frame)\n\
       --cache-dir PATH      Persistent cache directory\n\
       --cache-mib N         Cache size in MiB (default: 1024)\n\
       --sync-seconds N      Manifest refresh interval (default: 5)\n\
       --slide-seconds N     Time per photo (default: 12)\n\
       --windowed            Open a resizable development window\n\
       -h, --help            Print this help\n\
     \n\
     The same settings can be supplied with ASTROHUD_SERVER_URL,\n\
     ASTROHUD_FRAME_ID, ASTROHUD_FRAME_CACHE_DIR, ASTROHUD_CACHE_MIB,\n\
     ASTROHUD_SYNC_SECONDS, and ASTROHUD_SLIDE_SECONDS."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Config {
        Config {
            server_url: DEFAULT_SERVER.to_owned(),
            frame_id: DEFAULT_FRAME_ID.to_owned(),
            cache_dir: PathBuf::from("cache"),
            cache_limit_bytes: DEFAULT_CACHE_MIB * 1024 * 1024,
            sync_interval: Duration::from_secs(DEFAULT_SYNC_SECONDS),
            slide_interval: Duration::from_secs(DEFAULT_SLIDE_SECONDS),
            windowed: false,
        }
    }

    #[test]
    fn arguments_override_defaults() {
        let config = Config::parse_args(
            defaults(),
            [
                "--server",
                "http://frame.test:8080/",
                "--frame",
                "kitchen",
                "--cache-mib",
                "64",
                "--windowed",
            ],
        )
        .expect("parse arguments");

        assert_eq!(config.server_url, "http://frame.test:8080");
        assert_eq!(config.frame_id, "kitchen");
        assert_eq!(config.cache_limit_bytes, 64 * 1024 * 1024);
        assert!(config.windowed);
    }

    #[test]
    fn zero_intervals_are_rejected() {
        let result = Config::parse_args(defaults(), ["--sync-seconds", "0"]);
        assert!(result.is_err());
    }
}
