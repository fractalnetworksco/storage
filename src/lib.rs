mod api;
mod snapshot;
#[cfg(test)]
mod tests;
mod volume;

use anyhow::Result;
use fractal_auth_client::{key_store, AuthConfig, StaticToken};
use rocket::*;
use sqlx::AnyPool;
use std::net::SocketAddr;
use structopt::StructOpt;
use url::Url;

/// Backend for the Fractal Storage product. This is a service that holds metadata
/// on volumes and snapshots. Whenever a snapshot is made, it is uploaded (in encrypted
/// form) to IPFS, and a manifest is uploaded to this storage service. This service allows
/// for querying snapshots.
#[derive(StructOpt)]
pub struct Options {
    /// Which database to use. Specify a string like `sqlite://:memory:` to use an in-memory
    /// database, or `sqlite://storage.db` for a local file.
    #[structopt(long, short, env = "STORAGE_DATABASE")]
    database: String,

    /// JWKS URL, used to fetch manifest that is needed to validate JWTs. If not supplied, JWT
    /// authentication is disabled.
    #[structopt(long, env = "STORAGE_JWKS")]
    jwks: Option<Url>,

    /// IPFS node. Not required.
    #[structopt(long, env = "STORAGE_IPFS")]
    ipfs: Option<Url>,

    /// What IP address and port to listen on.
    #[structopt(long, env = "STORAGE_LISTEN", default_value = "0.0.0.0:8000")]
    listen: SocketAddr,

    /// Disable authentication altogether, parses authentication tokens as UUIDs. This flag is
    /// deprecated, it is recommended to use `--static-system` and `--static-user` instead.
    #[cfg(feature = "insecure-auth")]
    #[structopt(long, global = true)]
    insecure_auth_stub: bool,

    /// Adds a static user token. Supply it in the format `token:uuid`.
    #[structopt(long, env = "MANAGER_STATIC_USER", use_delimiter = true)]
    pub static_user: Vec<StaticToken>,

    /// Adds a static system token. Supply it in the format `token:uuid`.
    #[structopt(long, env = "MANAGER_STATIC_SYSTEM", use_delimiter = true)]
    pub static_system: Vec<StaticToken>,
}

impl Options {
    pub async fn run(&self) -> Result<()> {
        // connect to database
        let pool = AnyPool::connect(&self.database).await?;
        sqlx::migrate!().run(&pool).await?;

        // auth configuration
        let mut auth_config = AuthConfig::new();

        // add JWKS if set
        if let Some(jwks) = &self.jwks {
            info!("Fetching JWKS from {}", &jwks);
            let key_store = key_store(&jwks.to_string()).await?;
            auth_config = auth_config.with_keystore(key_store);
        }

        // add static user tokens, if supplied
        for user in &self.static_user {
            info!("Adding static user token for {}", user.account);
            auth_config.add_static_user(&user.token, &user.account);
        }

        // add static system tokens, if supplied
        for system in &self.static_system {
            info!("Adding static system token for {}", system.account);
            auth_config.add_static_system(&system.token, &system.account);
        }

        #[cfg(feature = "insecure-auth")]
        if self.insecure_auth_stub {
            error!("Enabling insecure auth, do not enable this in production");
            auth_config = auth_config.with_insecure_stub(self.insecure_auth_stub);
        }

        let config = Config::figment()
            .merge(("port", self.listen.port()))
            .merge(("address", self.listen.ip()));
        let _rocket = rocket::custom(config)
            .mount("/api/v1/", api::routes())
            .mount("/", api::health())
            .manage(pool)
            .manage(auth_config)
            .launch()
            .await?;

        Ok(())
    }
}
