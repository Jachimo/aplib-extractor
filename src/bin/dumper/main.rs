/*
 * Copyright (C) 2015-2025 Hubert Figuière
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::collections::HashMap;
use std::io::stderr;
use std::path::Path;

use clap::Parser;
use pbr::ProgressBar;
use serde::{Deserialize, Serialize};

use aplib::{Library, StoreWrapper, PROGRESS_NONE};

mod exporter;

#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    #[command(flatten)]
    export: ExportArgs,
}

#[derive(Clone, Debug, Parser)]
struct ExportArgs {
    #[arg(long)]
    out_dir: Option<String>,
    #[arg(long)]
    dryrun: bool,
    #[arg(long)]
    nas_safe: bool,
    #[arg(long, value_name = "MIB_PER_SEC")]
    max_write_mib_per_sec: Option<f64>,
    #[arg(long, value_name = "MIB_PER_SEC")]
    max_read_mib_per_sec: Option<f64>,
    #[arg(long, value_name = "MILLISECONDS")]
    io_delay_ms: Option<u64>,
    #[arg(long, value_name = "KIB")]
    io_chunk_kib: Option<usize>,
    path: String,
}

#[derive(Serialize, Deserialize)]
struct LibraryCache {
    version_map: HashMap<String, aplib::Version>,
    master_map: HashMap<String, aplib::Master>,
}

impl LibraryCache {
    fn new_or_load(library: &mut Library, cache_path: &Path) -> Self {
        if let Ok(file) = std::fs::File::open(cache_path) {
            println!(
                "Found existing cache file at {}. Reading / deserializing...",
                cache_path.display()
            );
            match serde_json::from_reader(file) {
                Ok(cache) => {
                    println!("Loaded cache from {}", cache_path.display());
                    return cache;
                }
                Err(e) => {
                    println!("Failed to deserialize cache, rebuilding... Error: {e:?}");
                }
            }
        } else {
            println!(
                "No cache found at {}, building cache...",
                cache_path.display()
            );
        }

        println!("Building version cache...");
        library.load_versions(PROGRESS_NONE);
        let versions = library.versions();
        let mut pb = ProgressBar::on(stderr(), versions.len() as u64);
        pb.message("Materializing versions map: ");
        pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(100)));
        let mut version_map = HashMap::new();
        for version_uuid in versions {
            if let Some(StoreWrapper::Version(version)) = library.get(version_uuid) {
                version_map.insert(version_uuid.to_owned(), (**version).clone());
            }
            pb.inc();
        }
        pb.finish();
        println!("Version cache ready ({} entries)", version_map.len());

        println!("Building master cache...");
        library.load_masters(PROGRESS_NONE);
        let masters = library.masters();
        let mut pb = ProgressBar::on(stderr(), masters.len() as u64);
        pb.message("Materializing masters map: ");
        pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(100)));
        let mut master_map = HashMap::new();
        for master_uuid in masters {
            if let Some(StoreWrapper::Master(master)) = library.get(master_uuid) {
                master_map.insert(master_uuid.to_owned(), (**master).clone());
            }
            pb.inc();
        }
        pb.finish();
        println!("Master cache ready ({} entries)", master_map.len());

        let cache = Self {
            version_map,
            master_map,
        };
        if let Ok(file) = std::fs::File::create(cache_path) {
            if let Err(e) = serde_json::to_writer(file, &cache) {
                eprintln!("Failed to write cache: {e}");
            } else {
                println!("Wrote cache to {}", cache_path.display());
            }
        } else {
            eprintln!("Failed to create cache file {}", cache_path.display());
        }
        cache
    }
}

fn main() {
    let args = Args::parse();
    exporter::process_export(&args.export);
}
