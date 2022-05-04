use anyhow::Result;
use cid::Cid;
use futures::StreamExt;
use ipfs_api::{IpfsClient, TryFromUri};
use reqwest::ClientBuilder;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use storage_api::{
    keys::{Privkey, Secret},
    SnapshotHeader, Storage,
};
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::stdin;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use url::Url;

const STORAGE_API: &str = "https://storage.fractalnetworks.co";

#[derive(StructOpt, Debug, Clone)]
pub struct Options {
    /// Url to the server running the storage API.
    #[structopt(long, short, global = true, env = "STORAGE_API")]
    server: Option<Url>,
    /// Url of IPFS server.
    #[structopt(long, global = true, env = "IPFS_API")]
    ipfs: Option<Url>,
    /// Allow invalid TLS certificates.
    #[structopt(long, global = true)]
    insecure: bool,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt, Debug, Clone)]
pub enum Command {
    /// Create a new volume (and private key).
    Create(CreateCommand),
    /// Return the latest snapshot available from a given parent.
    Latest(LatestCommand),
    /// List all snapshots that exist.
    List(ListCommand),
    /// Upload a new snapshot.
    Upload(UploadCommand),
    /// Upload a new snapshot using IPFS
    IpfsUpload(IpfsUploadCommand),
    /// Fetch data from IPFS.
    IpfsFetch(IpfsFetchCommand),
    /// Fetch a snapshot.
    Fetch(FetchCommand),
}

#[derive(StructOpt, Debug, Clone)]
pub struct CreateCommand {
    #[structopt(long, short)]
    privkey: Option<Privkey>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ListCommand {
    #[structopt(long, short = "k")]
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
    #[structopt(long, short = "k")]
    privkey: Privkey,
    #[structopt(long, short)]
    parent: Option<u64>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct UploadCommand {
    #[structopt(long, short = "k")]
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
pub struct IpfsUploadCommand {
    /// Decryption key (can also be derived from private key).
    #[structopt(long, required_unless("privkey"))]
    secret: Option<Secret>,
    /// Private key (used to derive decryption key).
    #[structopt(long, required_unless("secret"))]
    privkey: Option<Privkey>,
    /// File to upload, if none specified, read from standard input.
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct IpfsFetchCommand {
    /// Decryption key (can also be derived from private key).
    #[structopt(long, required_unless("privkey"))]
    secret: Option<Secret>,
    /// Private key (used to derive decryption key).
    #[structopt(long, required_unless("secret"))]
    privkey: Option<Privkey>,
    cid: Cid,
    /// File to upload, if none specified, read from standard input.
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct FetchCommand {
    #[structopt(long, short = "k")]
    privkey: Privkey,
    #[structopt(long, short)]
    generation: u64,
    #[structopt(long, short)]
    parent: Option<u64>,

    /// File to save to, if none specified, piped to standard output.
    file: Option<PathBuf>,
}

impl Options {
    pub fn ipfs(&self) -> Result<IpfsClient> {
        match &self.ipfs {
            Some(url) => Ok(IpfsClient::from_str(&url.to_string())?),
            None => Ok(IpfsClient::default()),
        }
    }

    pub fn server(&self) -> Url {
        self.server
            .clone()
            .unwrap_or_else(|| Url::from_str(STORAGE_API).unwrap())
    }

    pub async fn run(&self) -> Result<()> {
        let client = ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .build()?;
        match &self.command {
            Command::Create(create) => {
                let privkey = create.privkey.unwrap_or_else(|| {
                    let privkey = Privkey::generate();
                    println!("privkey {}", privkey);
                    privkey
                });
                let result = self.server().create(&client, &privkey).await?;
                if result {
                    println!("pubkey {}", privkey.pubkey());
                }
                Ok(())
            }
            Command::List(opts) => {
                let result = self
                    .server()
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
                    .server()
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

                let input: Pin<Box<dyn AsyncRead + Send + Sync>> = match &opts.file {
                    Some(file) => Box::pin(File::open(file).await?),
                    None => Box::pin(stdin()),
                };

                let result = self
                    .server()
                    .upload(&client, &opts.privkey, &header, input)
                    .await?;

                println!("{:#?}", result);
                Ok(())
            }
            Command::IpfsUpload(opts) => {
                let input: Pin<Box<dyn AsyncRead + Send + Sync>> = match &opts.file {
                    Some(file) => Box::pin(File::open(file).await?),
                    None => Box::pin(stdin()),
                };

                let input = ReaderStream::new(input);
                let input = Box::pin(input);

                let ipfs = self.ipfs()?;

                let secret = opts
                    .secret
                    .or_else(|| opts.privkey.map(|k| k.derive_secret()))
                    .unwrap();
                let cid = storage_api::upload_encrypt(&ipfs, &secret, input).await?;
                println!("{cid}");
                Ok(())
            }
            Command::IpfsFetch(opts) => {
                let ipfs = self.ipfs()?;
                let secret = opts
                    .secret
                    .or_else(|| opts.privkey.map(|k| k.derive_secret()))
                    .unwrap();
                let mut data = storage_api::fetch_decrypt(&ipfs, &secret, &opts.cid).await?;
                let mut stdout = tokio::io::stdout();

                loop {
                    match data.next().await {
                        Some(data) => stdout.write_all(&data?).await?,
                        None => break,
                    }
                }

                Ok(())
            }
            Command::Fetch(opts) => {
                let (header, mut stream) = self
                    .server()
                    .fetch(&client, &opts.privkey, opts.generation, opts.parent)
                    .await?;
                let mut stdout = tokio::io::stdout();
                eprintln!("{:#?}", header);
                while let Some(data) = stream.next().await {
                    let data = data?;
                    stdout.write_all(&data).await?;
                }
                Ok(())
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let options = Options::from_args();
    match options.run().await {
        Ok(_) => {}
        Err(e) => println!("{}", e.to_string()),
    }
}
