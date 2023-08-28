// Copyright 2023 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use super::wallet::{chunk_and_pay_for_storage, ChunkedFile};

use bytes::Bytes;
use clap::Parser;
use color_eyre::Result;
use libp2p::futures::future::join_all;
use sn_client::{Client, Files, WalletClient};
use sn_protocol::storage::{Chunk, ChunkAddress};
use tokio::{fs, io::AsyncReadExt, sync::Semaphore};

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::WalkDir;
use xor_name::XorName;

#[derive(Parser, Debug)]
pub enum FilesCmds {
    Upload {
        /// The location of the files to upload.
        #[clap(name = "path", value_name = "DIRECTORY")]
        path: PathBuf,
    },
    Download {
        /// Name of the file to download.
        #[clap(name = "file_name")]
        file_name: Option<String>,
        /// Address of the file to download, in hex string.
        #[clap(name = "file_addr")]
        file_addr: Option<String>,
    },
}

pub(crate) async fn files_cmds(
    cmds: FilesCmds,
    client: Client,
    root_dir: &Path,
    verify_store: bool,
) -> Result<()> {
    match cmds {
        FilesCmds::Upload { path } => upload_files(path, client, root_dir, verify_store).await?,
        FilesCmds::Download {
            file_name,
            file_addr,
        } => {
            let file_api: Files = Files::new(client);

            match (file_name, file_addr) {
                (Some(name), Some(address)) => {
                    let bytes = hex::decode(address).expect("Input address is not a hex string");
                    download_file(
                        &file_api,
                        &XorName(
                            bytes
                                .try_into()
                                .expect("Failed to parse XorName from hex string"),
                        ),
                        &name,
                        root_dir,
                    )
                    .await
                }
                _ => {
                    println!("Trying to download files recorded in uploaded_files folder");
                    download_files(&file_api, root_dir).await?
                }
            }
        }
    };
    Ok(())
}

/// Given a directory, upload all files contained
/// Optionally verifies data was stored successfully
async fn upload_files(
    files_path: PathBuf,
    client: Client,
    // wallet_client: &WalletClient,
    root_dir: &Path,
    verify_store: bool,
) -> Result<()> {
    debug!(
        "Uploading files from {:?}, will verify?: {verify_store}",
        files_path
    );
    // The input files_path has to be a dir
    let file_names_path = root_dir.join("uploaded_files");

    // Payment shall always be verified.
    let chunks_to_upload = chunk_and_pay_for_storage(&client, root_dir, &files_path, true).await?;

    let mut uploads = vec![];
    for (
        file_addr,
        ChunkedFile {
            file_name,
            size,
            chunks,
        },
    ) in chunks_to_upload.into_iter()
    {
        println!(
            "Storing file '{file_name}' of {size} bytes ({} chunk/s)..",
            chunks.len()
        );

        // Clone necessary variables for each file upload
        let file_api: Files = Files::new(client.clone());
        let wallet_dir = root_dir.to_path_buf();
        // Spawn a new task for each file upload
        let upload = match upload_chunks(&file_api, &file_name, &wallet_dir, chunks, verify_store)
            .await
        {
            Err(error) => {
                println!("Failed to store all chunks of file '{file_name}' to all nodes in the close group: {error}");
                None
            }
            _ => {
                println!("Successfully stored '{file_name}' to {file_addr:64x}");
                Some((file_addr, file_name))
            }
        };

        // Add the upload task to the list of uploads
        uploads.push(upload);
    }

    let chunks_to_fetch: Vec<_> = uploads.into_iter().filter(|x| x.is_some()).collect();

    let content = bincode::serialize(&chunks_to_fetch)?;
    tokio::fs::create_dir_all(file_names_path.as_path()).await?;
    let date_time = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let file_names_path = file_names_path.join(format!("file_names_{date_time}"));
    println!("Writing {} bytes to {file_names_path:?}", content.len());
    fs::write(file_names_path, content).await?;

    Ok(())
}

/// Upload chunks of an individual file to the network.
async fn upload_chunks(
    file_api: &Files,
    file_name: &str,
    wallet_dir: &Path,
    chunks_paths: Vec<(XorName, PathBuf)>,
    verify_store: bool,
) -> Result<()> {
    println!("Awaiting to start uploading chunks of {file_name:?}");
    for (name, path) in chunks_paths {
        // limit pulling of chunks into mem if we cannot handle more!
        let start_time = std::time::Instant::now();
        let wallet_client = file_api.wallet(wallet_dir).await?;

        // This is pre chunked, so we don't need to worry about the size of the file here being overly large.
        let chunk = Chunk::new(Bytes::from(fs::read(path).await?));

        file_api
            .get_payment_and_upload_chunk(chunk, &wallet_client, verify_store)
            .await?;
    }
    Ok(())
}

async fn download_files(file_api: &Files, root_dir: &Path) -> Result<()> {
    let docs_of_uploaded_files_path = root_dir.join("uploaded_files");
    let download_path = root_dir.join("downloaded_files");
    tokio::fs::create_dir_all(download_path.as_path()).await?;

    for entry in WalkDir::new(docs_of_uploaded_files_path)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            let index_doc_bytes = Bytes::from(fs::read(entry.path()).await?);
            let index_doc_name = entry.file_name();

            println!("Loading file names from index doc {index_doc_name:?}");
            let files_to_fetch: Vec<(XorName, String)> = bincode::deserialize(&index_doc_bytes)?;

            if files_to_fetch.is_empty() {
                println!("No files to download!");
            }
            for (xorname, file_name) in files_to_fetch.iter() {
                download_file(file_api, xorname, file_name, &download_path).await;
            }
        }
    }

    Ok(())
}

async fn download_file(
    file_api: &Files,
    xorname: &XorName,
    file_name: &String,
    download_path: &Path,
) {
    println!(
        "Downloading file {file_name:?} with address {:64x}",
        xorname
    );
    debug!("Downloading file {file_name:?}");
    match file_api.read_bytes(ChunkAddress::new(*xorname)).await {
        Ok(bytes) => {
            debug!("Successfully got file {file_name}!");
            println!("Successfully got file {file_name}!");
            let file_name_path = download_path.join(file_name);
            println!("Writing {} bytes to {file_name_path:?}", bytes.len());
            if let Err(err) = fs::write(file_name_path, bytes).await {
                println!("Failed to create file {file_name:?} with error {err:?}");
            }
        }
        Err(error) => {
            error!("Did not get file {file_name:?} from the network! {error}");
            println!("Did not get file {file_name:?} from the network! {error}")
        }
    }
}
