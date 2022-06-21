use fractal_storage::Options;
use log::*;
use structopt::StructOpt;

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
