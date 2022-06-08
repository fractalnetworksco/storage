mod api;
mod snapshot;
#[cfg(test)]
mod tests;
mod volume;

use anyhow::Result;
use fractal_auth_client::{key_store, AuthConfig, KeyStore};
use rocket::fs::TempFile;
use rocket::*;
use sqlx::AnyPool;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use url::Url;
use std::net::SocketAddr;

#[derive(StructOpt)]
struct Options {
    #[structopt(long, short, env = "STORAGE_DATABASE")]
    database: String,

    #[structopt(long, env = "STORAGE_JWKS")]
    jwks: Option<Url>,

    #[structopt(long, env = "STORAGE_IPFS")]
    ipfs: Option<Url>,

    #[structopt(long, env = "STORAGE_LISTEN", default_value = "0.0.0.0:8000")]
    listen: SocketAddr,

    #[cfg(feature = "insecure-auth")]
    #[structopt(long, global = true)]
    insecure_auth: bool,
}

impl Options {
    pub async fn run(&self) -> Result<()> {
        // connect to database
        let pool = AnyPool::connect(&self.database).await?;
        sqlx::migrate!().run(&pool).await?;

        // auth configuration
        let mut auth_config = AuthConfig::new();

        if let Some(jwks) = &self.jwks {
            info!("Fetching JWKS from {}", &jwks);
            let key_store = key_store(&jwks.to_string()).await?;
            auth_config = auth_config.with_keystore(key_store);
        }

        #[cfg(feature = "insecure-auth")]
        if self.insecure_auth {
            error!("Enabling insecure auth, do not enable this in production");
            auth_config = auth_config.with_insecure_stub(self.insecure_auth);
        }

        let config = Config::figment()
            .merge(("port", self.listen.port()))
            .merge(("address", self.listen.ip()));
        rocket::custom(config)
            .mount("/api/v1/", api::routes())
            .mount("/", api::health())
            .manage(pool)
            .manage(auth_config)
            .launch()
            .await?;

        Ok(())
    }
}

#[rocket::main]
async fn main() {
    env_logger::init();
    let options = Options::from_args();
    match options.run().await {
        Ok(()) => {}
        Err(error) => {
            error!("Fatal error: {error:?}");
        }
    }
}
