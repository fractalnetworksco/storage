use anyhow::Result;
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use storage_api::{ed25519::*, SnapshotHeader, Storage};
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::stdin;
use tokio::io::AsyncRead;
use url::Url;

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
    Upload(UploadCommand),
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

#[derive(StructOpt, Debug, Clone)]
pub struct UploadCommand {
    #[structopt(long, short)]
    privkey: Privkey,
    #[structopt(long, short)]
    generation: u64,
    #[structopt(long, short)]
    parent: Option<u64>,
    #[structopt(long, short)]
    creation: u64,

    /// File to upload, if none specified, read from standard input.
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct FetchCommand {
    #[structopt(long, short)]
    privkey: Privkey,
    #[structopt(long, short)]
    generation: u64,
    #[structopt(long, short)]
    parent: Option<u64>,

    /// File to save to, if none specified, piped to standard output.
    file: Option<PathBuf>,
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
                let result = self
                    .server
                    .list(
                        &client,
                        &opts.privkey.pubkey(),
                        opts.parent,
                        opts.genmin,
                        opts.genmax,
                    )
                    .await?;
                println!("{:#?}", result);
                Ok(())
            }
            Command::Latest(opts) => {
                let result = self
                    .server
                    .latest(&client, &opts.privkey.pubkey(), opts.parent)
                    .await?;
                println!("{:#?}", result);
                Ok(())
            }
            Command::Upload(opts) => {
                let header = SnapshotHeader {
                    parent: opts.parent,
                    generation: opts.generation,
                    creation: opts.creation,
                };

                let input: Pin<Box<dyn AsyncRead + Send>> = match &opts.file {
                    Some(file) => Box::pin(File::open(file).await?),
                    None => Box::pin(stdin()),
                };

                let result = self
                    .server
                    .upload(&client, &opts.privkey, &header, input)
                    .await?;

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
        Ok(_) => {}
        Err(e) => println!("{}", e.to_string()),
    }
}
