use config::{Config, ConfigError, Environment, File};
use serde_derive::Deserialize;
use std::env;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub rabbit: RabbitMQSettings,
    pub elasticsearch: ElasticsearchSettings,
    pub application: ApplicationSettings,
}
impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = env::var("APP_PROFILE").unwrap_or_else(|_| "dev".into());

        let s = Config::builder()
            .add_source(File::with_name("application"))
            .add_source(File::with_name(&format!("application-{}", run_mode)).required(false))
            .add_source(File::with_name("application-local").required(false))
            .add_source(
                Environment::with_prefix("APP")
                    .prefix_separator("_")
                    .separator("_")
                    .try_parsing(true),
            )
            .build()?;

        s.try_deserialize()
    }
}

#[derive(Debug, Deserialize)]
pub struct RabbitMQSettings {
    pub host: String,
    pub user: String,
    pub password: String,
    pub port: u16,
    pub vhost: String,
    pub prefetch: u16,
    pub queue: String,
    pub tag: String,
}

#[derive(Debug, Deserialize)]
pub struct ElasticsearchSettings {
    pub url: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ApplicationSettings {
    pub idle: u64,
    pub batch: usize,
}
