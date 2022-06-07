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

#[derive(StructOpt, Clone, Debug)]
pub enum Storage {
    #[cfg(feature = "backend-local")]
    Local(StorageLocal),
    #[cfg(feature = "backend-s3")]
    S3(StorageS3),
}

impl Storage {
    pub fn path(&self) -> &Path {
        match self {
            Storage::Local(storage) => &storage.path,
            _ => unimplemented!(),
        }
    }
}

#[derive(StructOpt, Clone, Debug)]
pub struct StorageLocal {
    #[structopt(long, short)]
    path: PathBuf,
}

#[derive(StructOpt, Clone, Debug)]
pub struct StorageS3 {
    #[structopt(long)]
    access_key: String,
    #[structopt(long)]
    secret_key: String,
    #[structopt(long)]
    security_token: String,
}

#[derive(StructOpt)]
struct Options {
    #[structopt(long, short, env = "STORAGE_DATABASE", global = true)]
    database: Url,

    #[structopt(long, env = "STORAGE_JWKS", global = true)]
    jwks: Option<Url>,

    #[structopt(long, env = "STORAGE_IPFS", global = true)]
    ipfs: Option<Url>,

    #[cfg(feature = "insecure-auth")]
    #[structopt(long, global = true)]
    insecure_auth: bool,

    #[structopt(subcommand)]
    storage: Storage,
}

impl Options {
    pub async fn run(&self) -> Result<()> {
        // connect to database
        let pool = AnyPool::connect(&self.database.to_string()).await?;
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

        rocket::build()
            .mount("/api/v1/", api::routes())
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
