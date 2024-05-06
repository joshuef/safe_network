// Copyright 2024 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

use clap::Parser;
use color_eyre::eyre::{bail, Result};
use std::env;
use std::process::Command;
use tokio::task::LocalSet;

use sn_node_launchpad::{
    app::App,
    utils::{initialize_logging, initialize_panic_handler, version},
};

#[derive(Parser, Debug)]
#[command(author, version = version(), about)]
pub struct Cli {
    #[arg(
        short,
        long,
        value_name = "FLOAT",
        help = "Tick rate, i.e. number of ticks per second",
        default_value_t = 1.0
    )]
    pub tick_rate: f64,

    #[arg(
        short,
        long,
        value_name = "FLOAT",
        help = "Frame rate, i.e. number of frames per second",
        default_value_t = 60.0
    )]
    pub frame_rate: f64,
}

async fn tokio_main() -> Result<()> {
    initialize_logging()?;

    initialize_panic_handler()?;

    let args = Cli::parse();
    let mut app = App::new(args.tick_rate, args.frame_rate)?;
    app.run().await?;

    Ok(())
}

#[cfg(not(windows))]
fn is_running_root() -> bool {
    use nix::unistd::geteuid;
    geteuid().is_root()
}

#[cfg(windows)]
fn is_running_root() -> bool {
    // Example: Attempt to read from a typically restricted system directory
    std::fs::read_dir("C:\\Windows\\System32\\config").is_ok()
}

#[tokio::main]
async fn main() -> Result<()> {
    if !is_running_root() {
        #[cfg(windows)]
        {
            bail!("Admin privilges required to run");
        }

        // Attempt to elevate privileges using sudo
        let exe = env::current_exe()?;
        let status = Command::new("sudo").arg(exe).status()?;

        if !status.success() {
            bail!("Failed to gain root privileges.");
        }
        return Ok(());
    }

    // Construct a local task set that can run `!Send` futures.
    let local = LocalSet::new();
    local
        .run_until(async {
            if let Err(e) = tokio_main().await {
                eprintln!("{} error: Something went wrong", env!("CARGO_PKG_NAME"));
                Err(e)
            } else {
                Ok(())
            }
        })
        .await
}
