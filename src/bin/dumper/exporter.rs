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

use exempi2::SerialFlags;
//use crate::exporter::ns; // for custom XMP namespace addition
use aplib::xmp::{ToXmp, XmpProperty, ns};

#[derive(Debug, Serialize)]
pub struct ExportJob {
    pub master_uuid: String,
    pub master_path: PathBuf,
    pub master_filename: String,
    pub master_rel_dir: PathBuf, // relative directory for output
    pub version_uuids: Vec<String>,
    pub version_paths: Vec<PathBuf>,
    pub version_filenames: Vec<String>,
    // Removed: pub sidecar_filename: String,
}

fn export_job_files(
    job: &ExportJob,
    out_dir: &Path,
    cache: &LibraryCache,
    library_root_path: &Path,
    keyword_map: &std::collections::HashMap<String, String>,
) -> std::io::Result<()> {

    // Create the output directory
    let master_out_dir = out_dir.join(&job.master_rel_dir);
    fs::create_dir_all(&master_out_dir)?;

    // Copy master
    let master_out = master_out_dir.join(&job.master_filename);
    if !job.master_path.exists() {
        eprintln!("Warning: master file {} does not exist, skipping.", job.master_path.display());
        return Ok(());
    }
    fs::copy(&job.master_path, &master_out)?;

    // Write master XMP sidecar (same basename, .xmp extension)
    let master_sidecar = master_out.with_extension("xmp");
    let mut xmp = exempi2::Xmp::new();

    if let Some(master) = cache.master_map.get(&job.master_uuid) {
        let mut master = master.clone();

        // Resolve keywords from UUIDs to names
        if let Some(ref raw_keywords) = master.keywords {
            let resolved: Vec<String> = raw_keywords
                .iter()
                .filter_map(|kw| keyword_map.get(kw).cloned())
                .collect();
            master.keywords = if resolved.is_empty() { None } else { Some(resolved) };
        }

        // Populate "ApertureLibraryPath" custom metadata field from actual dir structure
        let mut custom = master.custom_aplib_fields.unwrap_or_default();
        if let Ok(rel_path) = job.master_path.strip_prefix(library_root_path) {
            custom.insert("ApertureLibraryPath".to_string(), rel_path.to_string_lossy().to_string());
        } else {
            custom.insert("ApertureLibraryPath".to_string(), job.master_path.to_string_lossy().to_string());
        }

        // Create XMP elements from metadata fields
        master.custom_aplib_fields = Some(custom);
        if !master.to_xmp(&mut xmp) {
            eprintln!("Warning: XMP metadata incomplete for master {}", job.master_uuid);
        }
    }
    // Create and write the sidecar file 
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
        // Place version in the same subdirectory as the master (unified output tree)
        let version_out_dir = master_out_dir.clone();
        fs::create_dir_all(&version_out_dir)?;
        let dest = version_out_dir.join(dest_name);
        fs::copy(src, &dest)?;

        // Write version XMP sidecar
        let version_sidecar = dest.with_extension("xmp");
        let mut xmp = exempi2::Xmp::new();
        if let Some(version) = cache.version_map.get(version_uuid) {
            let mut version = version.clone();
            // Resolve keywords from UUIDs to names
            if let Some(ref raw_keywords) = version.keywords {
                let resolved: Vec<String> = raw_keywords
                    .iter()
                    .filter_map(|kw| keyword_map.get(kw).cloned())
                    .collect();
                version.keywords = if resolved.is_empty() { None } else { Some(resolved) };
            }
            let mut custom = version.custom_aplib_fields.unwrap_or_default();
            if let Ok(rel_path) = src.strip_prefix(library_root_path) {
                custom.insert("ApertureLibraryPath".to_string(), rel_path.to_string_lossy().to_string());
            } else {
                custom.insert("ApertureLibraryPath".to_string(), src.to_string_lossy().to_string());
            }
            version.custom_aplib_fields = Some(custom);
            if !version.to_xmp(&mut xmp) {
                eprintln!("Warning: XMP metadata incomplete for version {}", version_uuid);
            }

            // Add master reference to version's metadata
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
        let db_masters_dir = library_abs.join("Database").join("Masters");
        if db_masters_dir.is_dir() {
            db_masters_dir
        } else {
            library_abs.to_path_buf()
        }
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
        let image_path = match master.image_path.as_ref() {
            Some(p) => p,
            None => {
                eprintln!("Warning: master {} has no image_path, skipping.", master_uuid);
                continue;
            }
        };
        let master_path = master_root.join(image_path);

        // Compute the relative path (e.g., 2006/11/02/20061102-161812/PICT0019.JPG)
        let rel_path = Path::new(image_path);

        // The output path for the master will be out_dir/rel_path
        let master_filename = rel_path.file_name().unwrap().to_string_lossy().to_string();
        let master_rel_dir = rel_path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();

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

        jobs.push(ExportJob {
            master_uuid: master_uuid.clone(),
            master_path,
            master_filename,
            master_rel_dir,
            version_uuids,
            version_paths,
            version_filenames,
        });
    }
    jobs
}

/// The main export entry point, moved from process_export in main.rs
pub fn process_export(args: &super::ExportArgs) {
    let library_abs = fs::canonicalize(&args.path)
        .expect("Failed to resolve absolute path to library");

    let mut library = Library::new(&args.path);

    let keyword_map: std::collections::HashMap<String, String> = library
        .list_keywords()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|kw| {
            if let (Some(uuid), name) = (kw.uuid.as_ref(), &kw.name) {
                if !uuid.is_empty() && !name.is_empty() {
                    Some((uuid.clone(), name.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

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

    // Create a custom namespace for non-standard XMP fields
    let _ = exempi2::register_namespace(ns::APLIB, "aplib");

    // Currently we export all masters and versions *that exist in the Versions tree*
    // This means: "orphaned" masters (without versions) will not be exported!
    // TODO: Create an option to either include or at least report a list of "orphans"
    
    let jobs = build_export_jobs(&library, &cache, &library_abs, out_dir);
    println!("Prepared {} export jobs (one per master).", jobs.len());

    for job in &jobs {
        println!(
            "Exporting master {} and {} versions...",
            job.master_uuid,
            job.version_uuids.len()
        );
        if !args.dryrun {
            if let Err(e) = export_job_files(job, out_dir, &cache, &library_abs, &keyword_map) {
                eprintln!("Failed to export {}: {}", job.master_uuid, e);
            }
        }
    }
}
