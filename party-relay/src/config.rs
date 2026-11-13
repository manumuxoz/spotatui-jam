use std::{
    error::Error,
    fmt::{self, Display},
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};

/// Runtime configuration read from the environment.
#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub max_participants: usize,
    pub max_message_bytes: usize,
    pub room_ttl: Duration,
    pub queue_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 8080)),
            max_participants: 8,
            max_message_bytes: 4096,
            room_ttl: Duration::from_secs(300),
            queue_enabled: true,
        }
    }
}

impl Config {
    /// Reads `BIND_ADDR`, `MAX_PARTICIPANTS`, `MAX_MESSAGE_BYTES`,
    /// `ROOM_TTL_SECS` and `QUEUE_ENABLED`, falling back to the documented
    /// defaults. Invalid values are an error instead of being silently ignored.
    pub fn from_env() -> Result<Self, ConfigError> {
        let default = Self::default();

        let bind_addr = match env_value("BIND_ADDR")? {
            Some(raw) => raw.trim().parse().map_err(|_| {
                ConfigError::new("BIND_ADDR must be a socket address like 0.0.0.0:8080")
            })?,
            None => default.bind_addr,
        };

        let max_participants: usize = parse_env("MAX_PARTICIPANTS", default.max_participants)?;
        if max_participants == 0 {
            return Err(ConfigError::new("MAX_PARTICIPANTS must be at least 1"));
        }

        let max_message_bytes: usize = parse_env("MAX_MESSAGE_BYTES", default.max_message_bytes)?;
        if max_message_bytes == 0 {
            return Err(ConfigError::new("MAX_MESSAGE_BYTES must be at least 1"));
        }

        let room_ttl_secs: u64 = parse_env("ROOM_TTL_SECS", default.room_ttl.as_secs())?;

        let queue_enabled = match env_value("QUEUE_ENABLED")? {
            Some(raw) => parse_bool(&raw).ok_or_else(|| {
                ConfigError::new("QUEUE_ENABLED must be one of 1, 0, true, false, yes, no, on, off")
            })?,
            None => default.queue_enabled,
        };

        Ok(Self {
            bind_addr,
            max_participants,
            max_message_bytes,
            room_ttl: Duration::from_secs(room_ttl_secs),
            queue_enabled,
        })
    }
}

fn env_value(name: &str) -> Result<Option<String>, ConfigError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(ConfigError::new(format!("{name} is not valid UTF-8")))
        }
    }
}

fn parse_env<T>(name: &str, default: T) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    match env_value(name)? {
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|error| ConfigError::new(format!("{name} has an invalid value: {error}"))),
        None => Ok(default),
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disabled" => Some(false),
        _ => None,
    }
}

#[derive(Debug)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::parse_bool;

    #[test]
    fn boolean_env_values_accept_common_spellings() {
        for value in ["1", "true", "TRUE", "yes", "on", " true "] {
            assert_eq!(parse_bool(value), Some(true));
        }
        for value in ["0", "false", "no", "off", "disabled"] {
            assert_eq!(parse_bool(value), Some(false));
        }
        assert_eq!(parse_bool("maybe"), None);
    }
}
