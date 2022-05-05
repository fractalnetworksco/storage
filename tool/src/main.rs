use anyhow::anyhow;
use anyhow::Result;
use cid::Cid;
use futures::StreamExt;
use ipfs_api::{IpfsClient, TryFromUri};
use reqwest::ClientBuilder;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use storage_api::{keys::*, *};
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::stdin;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
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
    /// Generate a new key.
    Privkey,
    /// Generate the corresponding pubkey.
    Pubkey(PubkeyCommand),
    /// Generate the corresponding secret.
    Secret(SecretCommand),
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
    /// Generate a manifest from JSON
    ManifestSign(ManifestSignCommand),
    ManifestVerify(ManifestVerifyCommand),
}

#[derive(StructOpt, Debug, Clone)]
pub struct PubkeyCommand {
    privkey: Option<Privkey>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct SecretCommand {
    privkey: Option<Privkey>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ManifestSignCommand {
    #[structopt(long, short)]
    privkey: Option<Privkey>,
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ManifestVerifyCommand {
    #[structopt(long, short)]
    pubkey: Option<Pubkey>,
    file: Option<PathBuf>,
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

async fn read_privkey() -> Result<Privkey> {
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let line = lines.next_line().await?.ok_or(anyhow!("Error: no input"))?;
    Ok(Privkey::from_str(&line)?)
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
                let result = storage_api::create(&self.server(), &client, &privkey).await?;
                if result {
                    println!("pubkey {}", privkey.pubkey());
                }
                Ok(())
            }
            Command::List(opts) => {
                let result = storage_api::list(
                    &self.server(),
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
                let result = storage_api::latest(
                    &self.server(),
                    &client,
                    &opts.privkey.pubkey(),
                    opts.parent,
                )
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

                let result =
                    storage_api::upload(&self.server(), &client, &opts.privkey, &header, input)
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
                let (header, mut stream) = storage_api::fetch(
                    &self.server(),
                    &client,
                    &opts.privkey,
                    opts.generation,
                    opts.parent,
                )
                .await?;
                let mut stdout = tokio::io::stdout();
                eprintln!("{:#?}", header);
                while let Some(data) = stream.next().await {
                    let data = data?;
                    stdout.write_all(&data).await?;
                }
                Ok(())
            }
            Command::ManifestSign(opts) => Ok(()),
            Command::ManifestVerify(opts) => Ok(()),
            Command::Privkey => {
                let privkey = Privkey::generate();
                println!("{privkey}");
                Ok(())
            }
            Command::Pubkey(opts) => {
                let privkey = match opts.privkey {
                    Some(privkey) => privkey,
                    None => read_privkey().await?,
                };
                let pubkey = privkey.pubkey();
                println!("{pubkey}");
                Ok(())
            }
            Command::Secret(opts) => {
                let privkey = match opts.privkey {
                    Some(privkey) => privkey,
                    None => read_privkey().await?,
                };
                let secret = privkey.derive_secret();
                println!("{secret}");
                Ok(())
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let options = Options::from_args();
    match options.run().await {
        Ok(_) => {}
        Err(e) => println!("{}", e.to_string()),
    }
}
