mod api;
mod keys;
mod info;

use rocket::fs::TempFile;
use rocket::*;
use structopt::StructOpt;
use std::path::PathBuf;

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

    rocket::build()
        .mount("/", api::routes())
        .launch()
        .await;
}
