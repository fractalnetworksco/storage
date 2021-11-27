mod api;
mod db;
mod info;
mod keys;

use rocket::fs::TempFile;
use rocket::*;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Clone, Debug)]
pub enum Storage {
    #[cfg(feature = "backend-local")]
    Local(StorageLocal),
    #[cfg(feature = "backend-s3")]
    S3(StorageS3)
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
    path: PathBuf
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
    #[structopt(long, short)]
    database: PathBuf,
    #[structopt(subcommand)]
    storage: Storage,
}

#[rocket::main]
async fn main() {
    env_logger::init();

    let options = Options::from_args();

    // create database if not exists
    if !options.database.exists() {
        info!("Creating database file");
        tokio::fs::File::create(&options.database).await.unwrap();
    }

    // connect to database
    let database_path = options
        .database
        .clone()
        .into_os_string()
        .into_string()
        .unwrap();
    let pool = sqlx::SqlitePool::connect(&database_path).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();

    // make sure storage folder exists
    if !options.storage.path().is_dir() {
        error!("Storage folder does not exists");
        return;
    }

    rocket::build()
        .mount("/", api::routes())
        .manage(pool)
        .manage(options)
        .launch()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_migrations() {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
}
