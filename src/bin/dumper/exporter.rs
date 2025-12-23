/*
 * Copyright (C) 2025 by Github User @Jachimo 
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Library;
use super::LibraryCache;

//use exempi2::Xmp;
use exempi2::SerialFlags;
use aplib::xmp::{ToXmp, XmpProperty, ns};

#[derive(Debug, Serialize)]
pub struct ExportJob {
    pub master_uuid: String,
    pub master_path: PathBuf,
    pub master_filename: String,
    pub version_uuids: Vec<String>,
    pub version_paths: Vec<PathBuf>,
    pub version_filenames: Vec<String>,
    pub sidecar_filename: String,
}

fn export_job_files(job: &ExportJob, out_dir: &Path, cache: &LibraryCache) -> std::io::Result<()> {
    // Copy master
    let master_out = out_dir.join(&job.master_filename);
    if !job.master_path.exists() {
        eprintln!("Warning: master file {} does not exist, skipping.", job.master_path.display());
        return Ok(());
    }
    fs::copy(&job.master_path, &master_out)?;

    // Write master XMP sidecar
    let master_sidecar = master_out.with_extension("xmp");
    let mut xmp = exempi2::Xmp::new();
    if let Some(master) = cache.master_map.get(&job.master_uuid) {
        master.to_xmp(&mut xmp);
    }
    let mut file = fs::File::create(&master_sidecar)?;
    let xmp_string = xmp.serialize(SerialFlags::default(), 0)
        .unwrap_or_else(|_| exempi2::XmpString::new());
    file.write_all(xmp_string.to_string().as_bytes())?;

    // Copy versions and write their sidecars
    for ((src, dest_name), version_uuid) in job.version_paths.iter().zip(&job.version_filenames).zip(&job.version_uuids) {
        if !src.exists() {
            eprintln!("Warning: version file {} does not exist, skipping.", src.display());
            continue;
        }
        let dest = out_dir.join(dest_name);
        fs::copy(src, &dest)?;

        // Write version XMP sidecar
        let version_sidecar = dest.with_extension("xmp");
        let mut xmp = exempi2::Xmp::new();
        if let Some(version) = cache.version_map.get(version_uuid) {
            version.to_xmp(&mut xmp);
            // Add master reference
            XmpProperty::new(ns::APLIB, "MasterUUID").put_into_xmp(&job.master_uuid, &mut xmp);
            XmpProperty::new(ns::APLIB, "MasterFilename").put_into_xmp(&job.master_filename, &mut xmp);
        }
        let mut file = fs::File::create(&version_sidecar)?;
        let xmp_string = xmp.serialize(SerialFlags::default(), 0)
            .unwrap_or_else(|_| exempi2::XmpString::new());
        file.write_all(xmp_string.to_string().as_bytes())?;
    }

    Ok(())
}

pub fn get_master_root(library_abs: &Path) -> PathBuf {
    let masters_dir = library_abs.join("Masters");
    if masters_dir.is_dir() {
        masters_dir
    } else {
        library_abs.to_path_buf()
    }
}

pub fn get_version_image_path(library_path: &str, version_uuid: &str, file_name: &str) -> PathBuf {
    let subdir = &version_uuid[0..2];
    Path::new(library_path)
        .join("Database")
        .join("Versions")
        .join(subdir)
        .join(format!("{version_uuid}.apversion"))
        .join(file_name)
}

/// Build a list of export jobs for all masters and their versions
pub fn build_export_jobs(
    library: &Library,
    cache: &LibraryCache,
    library_abs: &Path,
    out_dir: &Path,
) -> Vec<ExportJob> {
    let mut jobs = Vec::new();
    let master_root = get_master_root(library_abs);

    for (master_uuid, master) in &cache.master_map {
        // Master file info
        let master_path = match master.image_path.as_ref() {
            Some(p) => master_root.join(p),
            None => {
                eprintln!("Warning: master {} has no image_path, skipping.", master_uuid);
                continue;
            }
        };        
        let master_filename = format!("{}_master{}", master_uuid, master_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());

        // Find all versions for this master
        let mut version_uuids = Vec::new();
        let mut version_paths = Vec::new();
        let mut version_filenames = Vec::new();

        for (version_uuid, version) in &cache.version_map {
            if version.master_uuid.as_ref() == Some(master_uuid) {
                version_uuids.push(version_uuid.clone());
                let version_file = version.file_name.clone().unwrap_or_default();
                let version_path = get_version_image_path(
                    library_abs.to_str().unwrap(),
                    version_uuid,
                    &version_file,
                );
                let version_filename = format!(
                    "{}_version_{}{}",
                    master_uuid,
                    version_uuid,
                    Path::new(&version_file)
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default()
                );
                version_paths.push(version_path);
                version_filenames.push(version_filename);
            }
        }

        // XMP sidecar filename
        let sidecar_filename = format!("{}.xmp", master_uuid);

        jobs.push(ExportJob {
            master_uuid: master_uuid.clone(),
            master_path,
            master_filename,
            version_uuids,
            version_paths,
            version_filenames,
            sidecar_filename,
        });
    }
    jobs
}

/// The main export entry point, moved from process_export in main.rs
pub fn process_export(args: &super::ExportArgs) {
    let library_abs = fs::canonicalize(&args.path)
        .expect("Failed to resolve absolute path to library");

    let mut library = Library::new(&args.path);

    let cache_path = {
        use std::hash::{Hasher, Hash};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        args.path.hash(&mut hasher);
        let hash = hasher.finish();
        PathBuf::from(format!("/tmp/aplib_cache_{hash:x}.bin"))
    };
    let cache = LibraryCache::new_or_load(&mut library, &cache_path);

    let out_dir = args.out_dir.as_deref().unwrap_or(".");
    let out_dir = Path::new(out_dir);
    if args.dryrun {
        println!("mkdir -p '{}'", out_dir.display());
    } else {
        fs::create_dir_all(out_dir).expect("Failed to create output directory");
    }

    if args.albums {
        println!("Exporting albums is not yet implemented.");
    } else if args.folders {
        println!("Exporting folders is not yet implemented.");
    } else if args.masters {
        println!("Exporting masters is not yet implemented.");
    } else if args.versions {
        println!("Exporting versions is not yet implemented.");
    } else if args.all {
        let jobs = build_export_jobs(&library, &cache, &library_abs, out_dir);
        println!("Prepared {} export jobs (one per master).", jobs.len());
        for job in &jobs {
            println!(
                "Exporting master {} and {} versions...",
                job.master_uuid,
                job.version_uuids.len()
            );
            if !args.dryrun {
                if let Err(e) = export_job_files(job, out_dir, &cache) {
                    eprintln!("Failed to export {}: {}", job.master_uuid, e);
                }
            }
        }
    } else {
        eprintln!("Specify --albums, --folders, --masters, --versions, or --all for export.");
    }
}