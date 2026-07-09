// This file is part of Compact.
// Copyright (C) 2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Reference end-to-end CLI for the Midnight DID Rust API.
//!
//! See `README.md` for the user-facing walkthrough; this entry point only
//! wires `clap` arguments to the [`flow`] driver and chooses an output
//! layout for the [`printer`].

#![warn(missing_docs, rust_2018_idioms, clippy::all)]
#![forbid(unsafe_code)]

mod fixtures;
mod flow;
mod printer;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use flow::{FlowDriver, Step, StepOutput};
use printer::{JsonLayout, print_document, print_header};

/// Reference CLI demonstrating the Midnight DID Rust API.
#[derive(Debug, Parser)]
#[command(name = "midnight-did-cli", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Run the CRUD flow and print each step's DID Document.
    Run {
        /// Force human-readable (multi-line) JSON output.
        #[arg(long, conflicts_with = "compact_json")]
        pretty_json: bool,
        /// Force one-line-per-document JSON output.
        #[arg(long)]
        compact_json: bool,
        /// Run a single step instead of the full sequence.
        #[arg(long, value_name = "NAME")]
        step: Option<String>,
    },
    /// Write each step's DID Document JSON to `<dir>/<step>.json`.
    CaptureFixtures {
        /// Output directory. Created if it does not exist.
        dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            pretty_json,
            compact_json,
            step,
        } => run(pretty_json, compact_json, step).await,
        Command::CaptureFixtures { dir } => capture(dir).await,
    }
}

/// Determine the output layout from explicit flags + tty heuristic.
fn layout_for(pretty: bool, compact: bool) -> JsonLayout {
    if compact {
        JsonLayout::Compact
    } else if pretty {
        JsonLayout::Pretty
    } else {
        // Default: pretty if stdout looks like a terminal, else compact.
        // We avoid pulling in `is-terminal` — environment-driven default is
        // sufficient for a demo binary. `NO_COLOR`-style heuristic: any value
        // for MIDNIGHT_DID_CLI_COMPACT switches to compact.
        if std::env::var_os("MIDNIGHT_DID_CLI_COMPACT").is_some() {
            JsonLayout::Compact
        } else {
            JsonLayout::Pretty
        }
    }
}

/// Execute the `run` subcommand.
async fn run(pretty: bool, compact: bool, single_step: Option<String>) -> Result<()> {
    let layout = layout_for(pretty, compact);
    let mut driver = FlowDriver::new();

    let steps: Vec<Step> = match single_step.as_deref() {
        Some(name) => vec![Step::from_cli(name).with_context(|| {
            format!(
                "unknown step `{name}` — try one of: create, set-vm, set-service, set-aka, rotate, resolve, deactivate"
            )
        })?],
        None => Step::ALL.to_vec(),
    };

    for (idx, step) in steps.into_iter().enumerate() {
        let output = driver.run_step(step).await?;
        print_header(idx + 1, output.display_name);
        print_document(&output.document, layout);
    }
    Ok(())
}

/// Execute the `capture-fixtures` subcommand.
async fn capture(dir: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&dir).with_context(|| format!("create dir {dir:?}"))?;
    let mut driver = FlowDriver::new();
    for step in Step::ALL {
        let StepOutput { name, document, .. } = driver.run_step(step).await?;
        let path = dir.join(format!("{name}.json"));
        let payload = serde_json::to_string_pretty(&document).context("serialize step output")?;
        std::fs::write(&path, payload).with_context(|| format!("write {path:?}"))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
