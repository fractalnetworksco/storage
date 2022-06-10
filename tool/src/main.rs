use anyhow::anyhow;
use anyhow::Result;
use cid::Cid;
use futures::StreamExt;
use ipfs_api::{IpfsClient, TryFromUri};
use reqwest::ClientBuilder;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use storage_api::{keys::*, *};
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::stdin;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
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
    /// JWT or ApiKey of user
    #[structopt(long, global = true, env = "STORAGE_TOKEN")]
    token: Option<String>,
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
    VolumeCreate(VolumeCreateCommand),
    /// List all snapshots that exist.
    SnapshotList(SnapshotListCommand),
    /// Upload a new snapshot using IPFS
    IpfsUpload(IpfsUploadCommand),
    /// Fetch data from IPFS.
    IpfsFetch(IpfsFetchCommand),
    /// Generate a manifest from JSON
    ManifestGenerate(ManifestGenerateCommand),
    ManifestParse(ManifestParseCommand),
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
pub struct ManifestGenerateCommand {
    /// Key to sign manifest with. Don't generate signature if missing.
    #[structopt(long, short)]
    privkey: Option<Privkey>,
    /// File to read JSON data from (otherwise read from standard input).
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct ManifestParseCommand {
    /// If given, validate signature.
    #[structopt(long, short)]
    pubkey: Option<Pubkey>,
    /// Ignore signature.
    #[structopt(long)]
    split_signature: bool,
    /// Ignore invalid signature.
    #[structopt(long)]
    ignore_invalid: bool,
    /// File to read manifest from (or read from standard input).
    file: Option<PathBuf>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct VolumeCreateCommand {
    #[structopt(long, short)]
    privkey: Option<Privkey>,
}

#[derive(StructOpt, Debug, Clone)]
pub struct SnapshotListCommand {
    #[structopt(long, short = "k")]
    privkey: Privkey,
    #[structopt(long, short)]
    parent: Option<Hash>,
    #[structopt(long, short)]
    root: bool,
}

#[derive(StructOpt, Debug, Clone)]
pub struct SnapshotFetchCommand {
    #[structopt(long, short = "k")]
    privkey: Privkey,
    #[structopt(long, short)]
    hash: Hash,
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

async fn read_privkey() -> Result<Privkey> {
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let line = lines.next_line().await?.ok_or(anyhow!("Error: no input"))?;
    Ok(Privkey::from_str(&line)?)
}

async fn read_data(file: Option<&Path>) -> Result<Vec<u8>> {
    let mut reader: Box<dyn AsyncRead + Unpin> = match file {
        Some(path) => Box::new(File::open(path).await?),
        None => Box::new(tokio::io::stdin()),
    };
    let mut data = vec![];
    reader.read_to_end(&mut data).await?;
    Ok(data)
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

    pub fn token(&self) -> String {
        self.token.clone().unwrap_or_else(|| String::new())
    }

    pub async fn run(&self) -> Result<()> {
        let client = ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .build()?;
        match &self.command {
            Command::VolumeCreate(create) => {
                let privkey = create.privkey.unwrap_or_else(|| {
                    let privkey = Privkey::generate();
                    println!("privkey {}", privkey);
                    privkey
                });
                let token = self.token();
                let result =
                    storage_api::volume_create(&self.server(), &client, &token, &privkey).await?;
                println!("pubkey {}", privkey.pubkey());
                Ok(())
            }
            Command::SnapshotList(opts) => {
                let result = storage_api::snapshot_list(
                    &self.server(),
                    &client,
                    &self.token(),
                    &opts.privkey.pubkey(),
                    opts.parent.as_ref(),
                    opts.root,
                )
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
            Command::ManifestGenerate(opts) => {
                let data = read_data(opts.file.as_deref()).await?;
                let manifest: Manifest = serde_json::from_slice(&data)?;
                match &opts.privkey {
                    Some(key) => {
                        tokio::io::stdout()
                            .write_all(&manifest.signed(&key))
                            .await?
                    }
                    None => tokio::io::stdout().write_all(&manifest.encode()).await?,
                }
                Ok(())
            }
            Command::ManifestParse(opts) => {
                let data = read_data(opts.file.as_deref()).await?;
                let manifest = match &opts.pubkey {
                    Some(key) => {
                        let (manifest, signature) =
                            Manifest::split(&data).ok_or(anyhow!("Manifest too short"))?;
                        match Manifest::validate(manifest, signature, key) {
                            Ok(()) => {}
                            Err(e) if opts.ignore_invalid => {
                                eprintln!("Warning: Invalid signature: {e}")
                            }
                            Err(e) => return Err(e),
                        }
                        manifest
                    }
                    None => match opts.split_signature {
                        true => {
                            Manifest::split(&data)
                                .ok_or(anyhow!("Manifest too short"))?
                                .0
                        }
                        false => &data,
                    },
                };
                let manifest = Manifest::decode(&manifest)?;
                let manifest = serde_json::to_string_pretty(&manifest)?;
                println!("{manifest}");
                Ok(())
            }
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
        Err(e) => eprintln!("{}", e.to_string()),
    }
}
