mod api;
mod db;
mod info;
mod keys;

use rocket::fs::TempFile;
use rocket::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Options {
    #[structopt(long, short)]
    database: PathBuf,
    #[structopt(long, short)]
    storage: PathBuf,
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
    if !options.storage.is_dir() {
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
