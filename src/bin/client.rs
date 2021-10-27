use structopt::StructOpt;
use url::Url;
use anyhow::Result;
use storage_api::{ed25519::*, Storage};
use reqwest::Client;

#[derive(StructOpt, Debug, Clone)]
pub struct Options {
    #[structopt(long, short)]
    server: Url,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt, Debug, Clone)]
pub enum Command {
    Create(CreateCommand),
    Latest(LatestCommand),
    List(ListCommand),
}

#[derive(StructOpt, Debug, Clone)]
pub struct CreateCommand {
    #[structopt(long, short)]
    privkey: Option<Privkey>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ListCommand {
    #[structopt(long, short)]
    privkey: Privkey,
    #[structopt(long, short)]
    parent: Option<u64>,
    #[structopt(long)]
    genmin: Option<u64>,
    #[structopt(long)]
    genmax: Option<u64>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct LatestCommand {
    #[structopt(long, short)]
    privkey: Privkey,
    #[structopt(long, short)]
    parent: Option<u64>,
}

impl Options {
    pub async fn run(&self) -> Result<()> {
        let client = Client::new();
        match &self.command {
            Command::Create(create) => {
                let privkey = create.privkey.unwrap_or_else(|| {
                    let privkey = Privkey::generate();
                    println!("privkey {}", privkey);
                    privkey
                });
                let result = self.server.create(&client, &privkey).await?;
                if result {
                    println!("pubkey {}", privkey.pubkey());
                }
                Ok(())
            }
            Command::List(opts) => {
                let result = self.server.list(
                    &client,
                    &opts.privkey.pubkey(),
                    opts.parent,
                    opts.genmin,
                    opts.genmax,
                ).await?;
                println!("{:#?}", result);
                Ok(())
            }
            Command::Latest(opts) => {
                let result = self.server.latest(
                    &client,
                    &opts.privkey.pubkey(),
                    opts.parent,
                ).await?;
                println!("{:#?}", result);
                Ok(())
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let options = Options::from_args();
    match options.run().await {
        Ok(_) => {},
        Err(e) => println!("{}", e.to_string())
    }
}
