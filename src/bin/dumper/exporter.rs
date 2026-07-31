/*
 * Copyright (C) 2016-2023 Hubert Figuière
 * Copyright (C) 2025-2026 the "aplib-extractor" Contributors
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use super::LibraryCache;
use crate::Library;

use exempi2::SerialFlags;
//use crate::exporter::ns; // for custom XMP namespace addition
use aplib::xmp::{ns, ToXmp, XmpProperty};

const MAX_EXPORT_FILENAME_LEN: usize = 200;

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

struct ExportContext<'a> {
    cache: &'a LibraryCache,
    library_root_path: &'a Path,
    flat_keyword_map: &'a std::collections::HashMap<String, String>,
    hierarchical_keyword_map: &'a std::collections::HashMap<String, String>,
    io_throttle: &'a IoThrottle,
    checkpoint_log_path: Option<&'a Path>,
}

#[derive(Clone, Debug)]
struct VersionExportEntry {
    version_uuid: String,
    version_number: Option<i64>,
    version_name: Option<String>,
    version_file_name: String,
    version_source_directory: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct IoThrottle {
    max_read_bytes_per_sec: Option<u64>,
    max_write_bytes_per_sec: Option<u64>,
    operation_delay: Duration,
    chunk_size: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct IoStats {
    read_bytes: u64,
    written_bytes: u64,
    elapsed: Duration,
}

impl IoStats {
    fn add(&mut self, other: IoStats) {
        self.read_bytes += other.read_bytes;
        self.written_bytes += other.written_bytes;
        self.elapsed += other.elapsed;
    }

    fn mibps(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            (self.written_bytes as f64 / (1024.0 * 1024.0)) / secs
        }
    }
}

impl IoThrottle {
    fn from_args(args: &super::ExportArgs) -> Self {
        let max_write_mib_per_sec = args
            .max_write_mib_per_sec
            .or_else(|| args.nas_safe.then_some(4.0));
        let max_read_mib_per_sec = args
            .max_read_mib_per_sec
            .or_else(|| args.nas_safe.then_some(4.0));

        let max_write_bytes_per_sec = max_write_mib_per_sec.and_then(|mib| {
            if mib <= 0.0 {
                None
            } else {
                Some((mib * 1024.0 * 1024.0) as u64)
            }
        });
        let max_read_bytes_per_sec = max_read_mib_per_sec.and_then(|mib| {
            if mib <= 0.0 {
                None
            } else {
                Some((mib * 1024.0 * 1024.0) as u64)
            }
        });

        let delay_ms = args
            .io_delay_ms
            .or_else(|| args.nas_safe.then_some(20))
            .unwrap_or(0);

        let chunk_kib = args
            .io_chunk_kib
            .or_else(|| args.nas_safe.then_some(64))
            .unwrap_or(256)
            .max(1);

        Self {
            max_read_bytes_per_sec,
            max_write_bytes_per_sec,
            operation_delay: Duration::from_millis(delay_ms),
            chunk_size: chunk_kib * 1024,
        }
    }

    fn describe(&self) -> String {
        let read_rate = self
            .max_read_bytes_per_sec
            .map(|v| format!("{:.2} MiB/s", v as f64 / (1024.0 * 1024.0)))
            .unwrap_or_else(|| "unlimited".to_string());
        let write_rate = self
            .max_write_bytes_per_sec
            .map(|v| format!("{:.2} MiB/s", v as f64 / (1024.0 * 1024.0)))
            .unwrap_or_else(|| "unlimited".to_string());

        format!(
            "read={}, write={}, delay={}ms, chunk={} KiB",
            read_rate,
            write_rate,
            self.operation_delay.as_millis(),
            self.chunk_size / 1024
        )
    }

    fn sleep_for_read_rate(&self, bytes_read: u64, started_at: Instant) {
        if let Some(max_bps) = self.max_read_bytes_per_sec {
            if max_bps == 0 {
                return;
            }
            let target_secs = bytes_read as f64 / max_bps as f64;
            let target_elapsed = Duration::from_secs_f64(target_secs);
            let actual_elapsed = started_at.elapsed();
            if target_elapsed > actual_elapsed {
                thread::sleep(target_elapsed - actual_elapsed);
            }
        }
    }

    fn sleep_for_write_rate(&self, bytes_written: u64, started_at: Instant) {
        if let Some(max_bps) = self.max_write_bytes_per_sec {
            if max_bps == 0 {
                return;
            }
            let target_secs = bytes_written as f64 / max_bps as f64;
            let target_elapsed = Duration::from_secs_f64(target_secs);
            let actual_elapsed = started_at.elapsed();
            if target_elapsed > actual_elapsed {
                thread::sleep(target_elapsed - actual_elapsed);
            }
        }
    }

    fn sleep_between_operations(&self) {
        if !self.operation_delay.is_zero() {
            thread::sleep(self.operation_delay);
        }
    }
}

fn compare_optional_version_numbers(left: &Option<i64>, right: &Option<i64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn sanitize_filename_component(input: &str, max_len: usize) -> String {
    let mut sanitized = String::new();
    let mut previous_was_dash = false;

    for ch in input.trim().chars() {
        let mapped = match ch {
            '/' | '\\' | ':' | '"' | '<' | '>' | '|' | '?' | '*' => Some('-'),
            c if c.is_control() => None,
            c if c.is_whitespace() => Some('-'),
            c => Some(c),
        };

        if let Some(mapped_ch) = mapped {
            if mapped_ch == '-' {
                if sanitized.is_empty() || previous_was_dash {
                    continue;
                }
                previous_was_dash = true;
            } else {
                previous_was_dash = false;
            }
            sanitized.push(mapped_ch);
        }
    }

    let trimmed = sanitized
        .trim_matches(|ch: char| ch == '.' || ch == '-' || ch.is_whitespace())
        .to_string();

    let mut result: String = trimmed.chars().take(max_len).collect();
    while result.ends_with('-') || result.ends_with('.') || result.ends_with(' ') {
        result.pop();
    }
    result
}

fn short_uuid_token(version_uuid: &str) -> String {
    let token: String = version_uuid
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .take(8)
        .collect();

    if token.is_empty() {
        String::from("unknown")
    } else {
        token
    }
}

fn master_stem(master_filename: &str, master_uuid: &str) -> String {
    Path::new(master_filename)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| master_uuid.to_string())
}

fn version_extension(version_file_name: &str) -> String {
    Path::new(version_file_name)
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default()
}

fn compose_version_filename(master_stem: &str, label: &str, version_file_name: &str) -> String {
    let extension = version_extension(version_file_name);
    let separator_len = 2;
    let max_master_len = MAX_EXPORT_FILENAME_LEN
        .saturating_sub(separator_len + label.len() + extension.len());
    let mut fitted_master: String = master_stem.chars().take(max_master_len).collect();

    if fitted_master.is_empty() {
        fitted_master = master_stem
            .chars()
            .take(MAX_EXPORT_FILENAME_LEN.saturating_sub(separator_len + label.len() + extension.len()))
            .collect();
    }

    if fitted_master.is_empty() {
        fitted_master = String::from("export");
    }

    format!("{}__{}{}", fitted_master, label, extension)
}

fn build_version_label(version_name: Option<&str>, version_uuid: &str) -> String {
    version_name
        .map(|name| sanitize_filename_component(name, 48))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("v-{}", short_uuid_token(version_uuid)))
}

#[cfg(test)]
fn build_version_filename(
    master_stem: &str,
    version_name: Option<&str>,
    version_uuid: &str,
    version_file_name: &str,
) -> String {
    let label = build_version_label(version_name, version_uuid);
    compose_version_filename(master_stem, &label, version_file_name)
}

fn dedupe_filename(
    master_stem: &str,
    version_name: Option<&str>,
    version_uuid: &str,
    version_file_name: &str,
    used: &mut HashSet<String>,
) -> String {
    let label = build_version_label(version_name, version_uuid);
    let candidate = compose_version_filename(master_stem, &label, version_file_name);
    if used.insert(candidate.clone()) {
        return candidate;
    }

    let token = short_uuid_token(version_uuid);

    let dedupe_label = format!("{}__u-{}", label, token);
    let mut attempt = compose_version_filename(master_stem, &dedupe_label, version_file_name);
    if used.insert(attempt.clone()) {
        return attempt;
    }

    let mut suffix = 2;
    loop {
        let dedupe_label = format!("{}__u-{}__n{}", label, token, suffix);
        attempt = compose_version_filename(master_stem, &dedupe_label, version_file_name);
        if used.insert(attempt.clone()) {
            return attempt;
        }
        suffix += 1;
    }
}

fn initialize_checkpoint_log(log_path: &Path, out_dir: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(file, "run\t{}\t{}", started_at, out_dir.display())?;
    file.flush()?;
    Ok(())
}

fn copy_file_with_throttle(
    src: &Path,
    dest: &Path,
    throttle: &IoThrottle,
) -> std::io::Result<IoStats> {
    let src_file = fs::File::open(src)?;
    let dest_file = fs::File::create(dest)?;

    let mut reader = BufReader::with_capacity(throttle.chunk_size, src_file);
    let mut writer = BufWriter::with_capacity(throttle.chunk_size, dest_file);
    let mut buf = vec![0_u8; throttle.chunk_size];
    let mut total_read: u64 = 0;
    let mut total_written: u64 = 0;
    let started_at = Instant::now();

    loop {
        let bytes_read = reader.read(&mut buf)?;
        if bytes_read == 0 {
            break;
        }
        total_read += bytes_read as u64;
        throttle.sleep_for_read_rate(total_read, started_at);
        writer.write_all(&buf[..bytes_read])?;
        total_written += bytes_read as u64;
        throttle.sleep_for_write_rate(total_written, started_at);
    }

    writer.flush()?;
    throttle.sleep_between_operations();
    Ok(IoStats {
        read_bytes: total_read,
        written_bytes: total_written,
        elapsed: started_at.elapsed(),
    })
}

fn write_sidecar_with_throttle(
    path: &Path,
    content: &[u8],
    throttle: &IoThrottle,
) -> std::io::Result<IoStats> {
    let file = fs::File::create(path)?;
    let mut writer = BufWriter::with_capacity(throttle.chunk_size, file);
    let mut offset = 0;
    let started_at = Instant::now();

    while offset < content.len() {
        let next = (offset + throttle.chunk_size).min(content.len());
        writer.write_all(&content[offset..next])?;
        offset = next;
        throttle.sleep_for_write_rate(offset as u64, started_at);
    }

    writer.flush()?;
    throttle.sleep_between_operations();
    Ok(IoStats {
        read_bytes: 0,
        written_bytes: content.len() as u64,
        elapsed: started_at.elapsed(),
    })
}

fn materialize_metadata_only_version_image(
    master_export_path: &Path,
    version_export_path: &Path,
    throttle: &IoThrottle,
) -> std::io::Result<IoStats> {
    if version_export_path == master_export_path {
        return Ok(IoStats::default());
    }

    if version_export_path.exists() {
        fs::remove_file(version_export_path)?;
    }

    match fs::hard_link(master_export_path, version_export_path) {
        Ok(()) => {
            throttle.sleep_between_operations();
            let file_size = fs::metadata(master_export_path)?.len();
            Ok(IoStats {
                read_bytes: 0,
                written_bytes: file_size,
                elapsed: Duration::default(),
            })
        }
        Err(_) => copy_file_with_throttle(master_export_path, version_export_path, throttle),
    }
}

fn append_checkpoint_log(log_path: &Path, entry: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{entry}");
        let _ = file.flush();
    } else {
        eprintln!(
            "Warning: could not append checkpoint entry to {}",
            log_path.display()
        );
    }
}

fn export_job_files(
    job: &ExportJob,
    out_dir: &Path,
    ctx: &ExportContext<'_>,
) -> std::io::Result<IoStats> {
    let mut io_stats = IoStats::default();

    // Create the output directory
    let master_out_dir = out_dir.join(&job.master_rel_dir);
    fs::create_dir_all(&master_out_dir).map_err(|e| {
        eprintln!(
            "Error creating output directory:\n  Path:  {}\n  Error: {} (os error: {:?})",
            master_out_dir.display(),
            e,
            e.raw_os_error()
        );
        e
    })?;

    // Copy master
    let master_out = master_out_dir.join(&job.master_filename);
    let copied_master = match copy_file_with_throttle(&job.master_path, &master_out, ctx.io_throttle) {
        Ok(stats) => stats,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "Warning: master file {} does not exist, skipping.",
                job.master_path.display()
            );
            return Ok(io_stats);
        }
        Err(e) => {
            eprintln!(
                "Error copying master file:\n  Source: {}\n  Dest:   {}\n  Error:  {} (os error: {:?})",
                job.master_path.display(),
                master_out.display(),
                e,
                e.raw_os_error()
            );
            return Err(e);
        }
    };
    io_stats.add(copied_master);

    // Write master XMP sidecar (same basename, .xmp extension)
    let master_sidecar = master_out.with_extension("xmp");
    let mut xmp = exempi2::Xmp::new();

    if let Some(master) = ctx.cache.master_map.get(&job.master_uuid) {
        let mut master = master.clone();

        // Resolve keywords from UUIDs to names
        if let Some(ref raw_keywords) = master.keywords {
            let flat_resolved = aplib::keyword::resolve_keyword_uuids(
                raw_keywords,
                ctx.flat_keyword_map,
                &format!("Master {}", job.master_uuid),
            );
            let hierarchical_resolved = aplib::keyword::resolve_keyword_uuids_for_digikam(
                raw_keywords,
                ctx.hierarchical_keyword_map,
                &format!("Master {}", job.master_uuid),
            );
            master.keywords = if flat_resolved.is_empty() {
                None
            } else {
                Some(flat_resolved)
            };

            // Store hierarchical keywords in custom fields for XMP writing
            if !hierarchical_resolved.is_empty() {
                let mut custom = master.custom_aplib_fields.unwrap_or_default();
                custom.insert(
                    "_resolved_hierarchical_keywords".to_string(),
                    serde_json::to_string(&hierarchical_resolved).unwrap_or_default(),
                );
                master.custom_aplib_fields = Some(custom);
            }
        }

        // Populate "ApertureLibraryPath" custom metadata field from actual dir structure
        let mut custom = master.custom_aplib_fields.unwrap_or_default();
        if let Ok(rel_path) = job.master_path.strip_prefix(ctx.library_root_path) {
            custom.insert(
                "ApertureLibraryPath".to_string(),
                rel_path.to_string_lossy().to_string(),
            );
        } else {
            custom.insert(
                "ApertureLibraryPath".to_string(),
                job.master_path.to_string_lossy().to_string(),
            );
        }

        // Create XMP elements from metadata fields
        master.custom_aplib_fields = Some(custom);
        if !master.to_xmp(&mut xmp) {
            eprintln!(
                "Warning: XMP metadata incomplete for master {}",
                job.master_uuid
            );
        }
    }
    let xmp_string = xmp
        .serialize(SerialFlags::default(), 0)
        .unwrap_or_else(|_| exempi2::XmpString::new());
    let xmp_bytes = xmp_string.to_string();
    let master_sidecar_stats =
        write_sidecar_with_throttle(&master_sidecar, xmp_bytes.as_bytes(), ctx.io_throttle).map_err(
            |e| {
                eprintln!(
                    "Error writing master XMP sidecar:\n  Path:  {}\n  Error: {} (os error: {:?})",
                    master_sidecar.display(),
                    e,
                    e.raw_os_error()
                );
                e
            },
        )?;
    io_stats.add(master_sidecar_stats);
    if let Some(log_path) = ctx.checkpoint_log_path {
        append_checkpoint_log(log_path, &format!("master\t{}\t{}", job.master_uuid, master_out.display()));
    }

    // Copy versions and write their sidecars
    let version_out_dir = master_out_dir.clone();
    fs::create_dir_all(&version_out_dir).map_err(|e| {
        eprintln!(
            "Error creating version output directory:\n  Path:  {}\n  Error: {} (os error: {:?})",
            version_out_dir.display(),
            e,
            e.raw_os_error()
        );
        e
    })?;
    let master_canonical_path = job.master_path.canonicalize().ok();

    for ((src, dest_name), version_uuid) in job
        .version_paths
        .iter()
        .zip(&job.version_filenames)
        .zip(&job.version_uuids)
    {
        // Check if version file is the same as master file (metadata-only version)
        let is_same_as_master = if src == &job.master_path {
            true
        } else if let Some(ref master_canonical) = master_canonical_path {
            src.canonicalize()
                .map(|version_canonical| version_canonical == *master_canonical)
                .unwrap_or(false)
        } else {
            false
        };

        let dest = version_out_dir.join(dest_name);

        // Ensure every version sidecar has a matching image basename for importers like DigiKam.
        // Metadata-only versions may point at the master image path, so materialize a version image
        // filename (hard-link when possible, copy fallback).
        if is_same_as_master {
            eprintln!(
                "  Version {} (metadata-only, materializing version image from master)",
                version_uuid
            );
            let materialized_version = match materialize_metadata_only_version_image(
                &master_out,
                &dest,
                ctx.io_throttle,
            ) {
                Ok(stats) => stats,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "Warning: could not materialize metadata-only version {} because master export file is missing at {}",
                        version_uuid,
                        master_out.display()
                    );
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "Error materializing metadata-only version file:\n  Version UUID: {}\n  Source: {}\n  Dest:   {}\n  Error:  {} (os error: {:?})",
                        version_uuid,
                        master_out.display(),
                        dest.display(),
                        e,
                        e.raw_os_error()
                    );
                    return Err(e);
                }
            };
            io_stats.add(materialized_version);
        } else {
            eprintln!("  Version {} (separate rendered image)", version_uuid);
            let copied_version = match copy_file_with_throttle(src, &dest, ctx.io_throttle) {
                Ok(stats) => stats,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "Warning: version file {} does not exist, skipping.",
                        src.display()
                    );
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "Error copying version file:\n  Version UUID: {}\n  Source: {}\n  Dest:   {}\n  Error:  {} (os error: {:?})",
                        version_uuid,
                        src.display(),
                        dest.display(),
                        e,
                        e.raw_os_error()
                    );
                    return Err(e);
                }
            };
            io_stats.add(copied_version);
        }

        // Write version XMP sidecar (always, regardless of whether image was copied)
        let version_sidecar = dest.with_extension("xmp");
        let mut xmp = exempi2::Xmp::new();
        if let Some(version) = ctx.cache.version_map.get(version_uuid) {
            let mut version = version.clone();
            // Resolve keywords from UUIDs to names
            if let Some(ref raw_keywords) = version.keywords {
                let flat_resolved = aplib::keyword::resolve_keyword_uuids(
                    raw_keywords,
                    ctx.flat_keyword_map,
                    &format!("Version {}", version_uuid),
                );
                let hierarchical_resolved = aplib::keyword::resolve_keyword_uuids_for_digikam(
                    raw_keywords,
                    ctx.hierarchical_keyword_map,
                    &format!("Version {}", version_uuid),
                );
                version.keywords = if flat_resolved.is_empty() {
                    None
                } else {
                    Some(flat_resolved)
                };

                // Store hierarchical keywords in custom fields for XMP writing
                if !hierarchical_resolved.is_empty() {
                    let mut custom = version.custom_aplib_fields.unwrap_or_default();
                    custom.insert(
                        "_resolved_hierarchical_keywords".to_string(),
                        serde_json::to_string(&hierarchical_resolved).unwrap_or_default(),
                    );
                    version.custom_aplib_fields = Some(custom);
                }
            }
            let mut custom = version.custom_aplib_fields.unwrap_or_default();
            if let Ok(rel_path) = src.strip_prefix(ctx.library_root_path) {
                custom.insert(
                    "ApertureLibraryPath".to_string(),
                    rel_path.to_string_lossy().to_string(),
                );
            } else {
                custom.insert(
                    "ApertureLibraryPath".to_string(),
                    src.to_string_lossy().to_string(),
                );
            }
            version.custom_aplib_fields = Some(custom);
            if !version.to_xmp(&mut xmp) {
                eprintln!(
                    "Warning: XMP metadata incomplete for version {}",
                    version_uuid
                );
            }

            // Add master reference to version's metadata
            XmpProperty::new(ns::APLIB, "MasterUUID").put_into_xmp(&job.master_uuid, &mut xmp);
            XmpProperty::new(ns::APLIB, "MasterFilename")
                .put_into_xmp(&job.master_filename, &mut xmp);
        }
        let xmp_string = xmp
            .serialize(SerialFlags::default(), 0)
            .unwrap_or_else(|_| exempi2::XmpString::new());
        let xmp_bytes = xmp_string.to_string();
        let version_sidecar_stats = write_sidecar_with_throttle(&version_sidecar, xmp_bytes.as_bytes(), ctx.io_throttle).map_err(|e| {
            eprintln!(
                "Error writing version XMP sidecar:\n  Version UUID: {}\n  Path:  {}\n  Error: {} (os error: {:?})",
                version_uuid,
                version_sidecar.display(),
                e,
                e.raw_os_error()
            );
            e
        })?;
        io_stats.add(version_sidecar_stats);
        if let Some(log_path) = ctx.checkpoint_log_path {
            append_checkpoint_log(
                log_path,
                &format!("version\t{}\t{}\t{}", job.master_uuid, version_uuid, version_sidecar.display()),
            );
        }
    }

    Ok(io_stats)
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

/// Transform a Versions directory path to the corresponding Masters directory path.
/// Version image files are stored in the Masters tree, not in the Versions tree.
///
/// Example transformation:
/// - Input: `/path/to/Library.aplibrary/Database/Versions/2006/11/03/20061103-133939/UUID`
/// - Output: `Masters/2006/11/03/20061103-133939`
fn transform_versions_to_masters_path(versions_path: &Path) -> Option<PathBuf> {
    // Convert to string for easier manipulation
    let path_str = versions_path.to_str()?;

    // Find the "Versions/" component in the path (could be absolute or relative)
    // Look for "Database/Versions/" first, then fall back to just "Versions/"
    let relative_path = if let Some(pos) = path_str.find("Database/Versions/") {
        // Skip past "Database/Versions/"
        &path_str[pos + "Database/Versions/".len()..]
    } else if let Some(pos) = path_str.find("/Versions/") {
        // Skip past "/Versions/"
        &path_str[pos + "/Versions/".len()..]
    } else if let Some(pos) = path_str.find("Versions/") {
        // Skip past "Versions/" (at start of path)
        &path_str[pos + "Versions/".len()..]
    } else {
        // Path doesn't contain "Versions/" - can't transform
        return None;
    };

    // The relative path is now like: "2006/11/03/20061103-133939/UUID"
    // We need to remove the UUID (last component) to get: "2006/11/03/20061103-133939"
    let path_without_uuid = Path::new(relative_path);
    let parent_path = path_without_uuid.parent()?;

    // Build the Masters path
    Some(Path::new("Masters").join(parent_path))
}

/// Search for a version image file in the Masters tree with flexible subdirectory handling.
///
/// Version images may be stored:
/// - Directly: `Masters/.../YYYYMMDD-HHMMSS/filename.jpg`
/// - In subdirectories: `Masters/.../YYYYMMDD-HHMMSS/{subdir}/filename.jpg`
/// - In deeply nested subdirectories: `Masters/.../YYYYMMDD-HHMMSS/Users/jtuttle/Dropbox/photos/folder/file.jpg`
///
/// This function searches the timestamp directory recursively for the file.
///
/// Note: Aperture metadata sometimes contains `:nopm:` (no PM/AM marker) in filenames,
/// but actual disk files don't have this string. We try both the original filename
/// and a version with `:nopm:` stripped.
fn find_version_image_file(masters_timestamp_dir: &Path, filename: &str) -> Option<PathBuf> {
    // Try to find the file with the original filename
    if let Some(path) = try_find_file_recursive(masters_timestamp_dir, filename) {
        return Some(path);
    }

    // If filename contains :nopm:, try again with it stripped
    // Example: "IMG_20150724_154440:nopm:.jpg" -> "IMG_20150724_154440.jpg"
    if filename.contains(":nopm:") {
        let clean_filename = filename.replace(":nopm:", "");
        if let Some(path) = try_find_file_recursive(masters_timestamp_dir, &clean_filename) {
            return Some(path);
        }
    }

    None
}

/// Recursively search for a file in a directory tree.
/// This handles cases where version files are stored in deeply nested subdirectories
/// like `Users/jtuttle/Dropbox/photos/folder/file.jpg`.
fn try_find_file_recursive(dir: &Path, target_filename: &str) -> Option<PathBuf> {
    // First try the direct path (most common case)
    let direct_path = dir.join(target_filename);
    if direct_path.exists() {
        return Some(direct_path);
    }

    // Recursively search subdirectories
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                let entry_path = entry.path();

                if file_type.is_dir() {
                    // Recurse into subdirectory
                    if let Some(found) = try_find_file_recursive(&entry_path, target_filename) {
                        return Some(found);
                    }
                } else if file_type.is_file() {
                    // Check if this file matches
                    if let Some(filename) = entry_path.file_name() {
                        if filename == target_filename {
                            return Some(entry_path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// DEPRECATED: This function is a last-resort fallback for when Version.source_directory is None.
/// This only happens with old cached data. The function cannot reliably locate version files
/// because it lacks the timestamp directory information (YYYYMMDD-HHMMSS) needed to construct
/// the correct Masters path.
///
/// The correct approach is to use Version.source_directory which is captured during loading.
/// If you see this warning, consider clearing the cache to reload fresh metadata.
pub fn get_version_image_path(library_path: &str, version_uuid: &str, file_name: &str) -> PathBuf {
    // We don't have enough information to construct the correct path.
    // Version images are in Masters/YYYY/MM/DD/YYYYMMDD-HHMMSS/ but we don't know the timestamp.
    // Return a placeholder path that will fail - this forces cache regeneration.
    eprintln!(
        "ERROR: Cannot construct version path for {} without source_directory metadata.",
        version_uuid
    );
    eprintln!("       Clear the cache (/tmp/aplib_cache_*.bin) and try again.");

    // Return an obviously wrong path to ensure it fails
    Path::new(library_path)
        .join("CACHE_OUT_OF_DATE")
        .join(version_uuid)
        .join(file_name)
}

/// Build a list of export jobs for all masters and their versions
pub fn build_export_jobs(cache: &LibraryCache, library_abs: &Path) -> Vec<ExportJob> {
    let mut jobs = Vec::new();
    let master_root = get_master_root(library_abs);

    for (master_uuid, master) in &cache.master_map {
        // Skip trashed files silently
        if master.is_in_trash == Some(true) {
            continue;
        }

        // Warn about missing files but continue processing
        if master.is_missing == Some(true) {
            eprintln!(
                "Warning: master {} is marked as missing in library metadata, file may not exist",
                master_uuid
            );
            // Continue anyway - the file existence check later will handle it
        }

        // Master file info
        let image_path = match master.image_path.as_ref() {
            Some(p) => p,
            None => {
                eprintln!(
                    "Warning: master {} has no image_path, skipping.",
                    master_uuid
                );
                continue;
            }
        };
        let master_path = master_root.join(image_path);

        // Compute the relative path (e.g., 2006/11/02/20061102-161812/PICT0019.JPG)
        let rel_path = Path::new(image_path);

        // The output path for the master will be out_dir/rel_path
        let master_filename = match rel_path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                eprintln!(
                    "Warning: master {} has image_path '{}' with no filename, skipping.",
                    master_uuid, image_path
                );
                continue;
            }
        };
        let master_rel_dir = rel_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();

        let master_stem_name = master_stem(&master_filename, master_uuid);

        // Find all versions for this master
        let mut version_entries: Vec<VersionExportEntry> = cache
            .version_map
            .iter()
            .filter_map(|(version_uuid, version)| {
                if version.master_uuid.as_ref() == Some(master_uuid)
                    && version.is_original != Some(true)
                {
                    Some(VersionExportEntry {
                        version_uuid: version_uuid.clone(),
                        version_number: version.version_number,
                        version_name: version.name.clone(),
                        version_file_name: version.file_name.clone().unwrap_or_default(),
                        version_source_directory: version.source_directory.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        version_entries.sort_by(|left, right| {
            compare_optional_version_numbers(&left.version_number, &right.version_number)
                .then_with(|| left.version_uuid.cmp(&right.version_uuid))
        });

        let mut used_filenames: HashSet<String> = HashSet::new();
        used_filenames.insert(master_filename.clone());

        let mut version_uuids = Vec::new();
        let mut version_paths = Vec::new();
        let mut version_filenames = Vec::new();

        for entry in version_entries {
            version_uuids.push(entry.version_uuid.clone());

            // Transform the source_directory path from Versions tree to Masters tree
            let version_path = if let Some(ref source_dir) = entry.version_source_directory {
                // Transform: Database/Versions/.../UUID -> Masters/.../
                if let Some(masters_dir) = transform_versions_to_masters_path(source_dir) {
                    let masters_timestamp_dir = library_abs.join(&masters_dir);
                    // Search flexibly for the file (may be in subdirectories)
                    if let Some(found_path) =
                        find_version_image_file(&masters_timestamp_dir, &entry.version_file_name)
                    {
                        found_path
                    } else {
                        // File not found even with flexible search
                        eprintln!(
                            "Warning: Could not find version file '{}' for version {} in {}",
                            entry.version_file_name,
                            entry.version_uuid,
                            masters_timestamp_dir.display()
                        );
                        // Return a non-existent path so the later check will skip it
                        masters_timestamp_dir.join(&entry.version_file_name)
                    }
                } else {
                    // Transformation failed - try using source_dir directly as fallback
                    eprintln!(
                        "Warning: Failed to transform version path for {}, trying source_directory directly",
                        entry.version_uuid
                    );
                    library_abs.join(source_dir).join(&entry.version_file_name)
                }
            } else {
                // Fallback to old logic (will likely fail, but preserves old behavior)
                eprintln!(
                    "Warning: Version {} has no source_directory, using fallback path logic (may not find file)",
                    entry.version_uuid
                );
                let library_abs_str = library_abs.to_string_lossy().to_string();
                get_version_image_path(&library_abs_str, &entry.version_uuid, &entry.version_file_name)
            };

            let version_filename = dedupe_filename(
                &master_stem_name,
                entry.version_name.as_deref(),
                &entry.version_uuid,
                &entry.version_file_name,
                &mut used_filenames,
            );
            version_paths.push(version_path);
            version_filenames.push(version_filename);
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
    let library_abs = match fs::canonicalize(&args.path) {
        Ok(path) => path,
        Err(e) => {
            eprintln!(
                "Failed to resolve absolute path to library '{}': {}",
                args.path, e
            );
            return;
        }
    };

    let mut library = Library::new(&args.path);

    let keywords = library.list_keywords().unwrap_or_default();
    let (flat_keyword_map, hierarchical_keyword_map) =
        aplib::keyword::build_keyword_maps(&keywords);
    eprintln!(
        "Loaded {} keywords ({} flat mappings, {} hierarchical mappings)",
        keywords.len(),
        flat_keyword_map.len(),
        hierarchical_keyword_map.len()
    );

    let cache_path = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        args.path.hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&args.path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH) {
                    elapsed.as_nanos().hash(&mut hasher);
                }
            }
        }
        let hash = hasher.finish();
        PathBuf::from(format!("/tmp/aplib_cache_{hash:x}.bin"))
    };
    let cache = LibraryCache::new_or_load(&mut library, &cache_path);
    let io_throttle = IoThrottle::from_args(args);

    let out_dir = args.out_dir.as_deref().unwrap_or(".");
    let out_dir = Path::new(out_dir);
    let checkpoint_log_path = out_dir.join("export-checkpoint.log");
    if args.dryrun {
        println!("mkdir -p '{}'", out_dir.display());
    } else {
        if let Err(e) = fs::create_dir_all(out_dir) {
            eprintln!(
                "Failed to create output directory '{}': {}",
                out_dir.display(),
                e
            );
            return;
        }
        if let Err(e) = initialize_checkpoint_log(&checkpoint_log_path, out_dir) {
            eprintln!(
                "Warning: could not initialize checkpoint log {}: {}",
                checkpoint_log_path.display(),
                e
            );
        }
    }

    eprintln!("Export I/O throttle settings: {}", io_throttle.describe());

    // Create custom namespaces for non-standard XMP fields
    let _ = exempi2::register_namespace(ns::APLIB, "aplib");
    let _ = exempi2::register_namespace(ns::NS_DIGIKAM, "digiKam");

    // Export all masters, and attach any matching rendered versions found in the Versions tree.
    // Masters without versions still export as standalone master images.

    let jobs = build_export_jobs(&cache, &library_abs);
    println!("Prepared {} export jobs (one per master).", jobs.len());
    let mut total_io = IoStats::default();
    let export_context = ExportContext {
        cache: &cache,
        library_root_path: &library_abs,
        flat_keyword_map: &flat_keyword_map,
        hierarchical_keyword_map: &hierarchical_keyword_map,
        io_throttle: &io_throttle,
        checkpoint_log_path: if args.dryrun {
            None
        } else {
            Some(checkpoint_log_path.as_path())
        },
    };

    for (idx, job) in jobs.iter().enumerate() {
        println!(
            "Exporting master {} and {} versions...",
            job.master_uuid,
            job.version_uuids.len()
        );
        if !args.dryrun {
            match export_job_files(
                job,
                out_dir,
                &export_context,
            ) {
                Ok(job_io) => {
                    total_io.add(job_io);
                    println!(
                        "I/O progress {}/{}: wrote {:.2} MiB in {:.2}s ({:.2} MiB/s effective)",
                        idx + 1,
                        jobs.len(),
                        job_io.written_bytes as f64 / (1024.0 * 1024.0),
                        job_io.elapsed.as_secs_f64(),
                        job_io.mibps(),
                    );
                }
                Err(e) => {
                    eprintln!("Failed to export {}: {}", job.master_uuid, e);
                }
            }
        }
    }

    if !args.dryrun {
        append_checkpoint_log(
            &checkpoint_log_path,
            &format!(
                "done\tread={:.2}MiB\twrote={:.2}MiB\telapsed={:.2}s",
                total_io.read_bytes as f64 / (1024.0 * 1024.0),
                total_io.written_bytes as f64 / (1024.0 * 1024.0),
                total_io.elapsed.as_secs_f64(),
            ),
        );
        println!(
            "Export I/O summary: read {:.2} MiB, wrote {:.2} MiB in {:.2}s ({:.2} MiB/s effective)",
            total_io.read_bytes as f64 / (1024.0 * 1024.0),
            total_io.written_bytes as f64 / (1024.0 * 1024.0),
            total_io.elapsed.as_secs_f64(),
            total_io.mibps(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use aplib::{AplibObject, Master, PlistLoadable, Version};
    use exempi2::{PropFlags, Xmp};

    fn fixture_path(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("TestLibrary.aplibrary")
            .join(rel)
    }

    fn fixture_library_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("TestLibrary.aplibrary")
    }

    fn fixture_master() -> Master {
        Master::from_path(
            fixture_path(
                "Database/Versions/2006/11/02/20061102-161812/V6jjzYNdSVu006MPsZkt5w/Master.apmaster",
            ),
            None,
        )
        .expect("fixture master should parse")
    }

    fn fixture_version() -> Version {
        Version::from_path(
            fixture_path(
                "Database/Versions/2006/11/02/20061102-161812/V6jjzYNdSVu006MPsZkt5w/Version-0.apversion",
            ),
            None,
        )
        .expect("fixture version should parse")
    }

    fn fixture_edited_version() -> Version {
        Version::from_path(
            fixture_path(
                "Database/Versions/2006/11/02/20061102-161812/V6jjzYNdSVu006MPsZkt5w/Version-1.apversion",
            ),
            None,
        )
        .expect("fixture edited version should parse")
    }

    fn read_xmp_from_file(path: &Path) -> Xmp {
        let content = fs::read(path).expect("xmp sidecar should be readable");
        Xmp::from_buffer(content).expect("xmp sidecar should parse")
    }

    fn read_xmp_text(path: &Path) -> String {
        fs::read_to_string(path).expect("xmp sidecar should be utf-8 text")
    }

    fn assert_xmp_property_eq(xmp: &Xmp, namespace: &str, property: &str, expected: &str) {
        let mut flags = PropFlags::NONE;
        let value = xmp
            .get_property(namespace, property, &mut flags)
            .unwrap_or_else(|_| panic!("missing XMP property {}:{}", namespace, property));
        assert_eq!(value.to_str().expect("xmp string should decode"), expected);
    }

    fn make_export_args() -> super::super::ExportArgs {
        super::super::ExportArgs {
            out_dir: None,
            dryrun: false,
            nas_safe: false,
            max_write_mib_per_sec: None,
            max_read_mib_per_sec: None,
            io_delay_ms: None,
            io_chunk_kib: None,
            path: String::from("/tmp/library.aplibrary"),
        }
    }

    #[test]
    fn test_transform_versions_to_masters_path() {
        // Test with Database/Versions prefix (relative path)
        let input =
            Path::new("Database/Versions/2006/11/03/20061103-133939/7KRjR0TQSZedV48EWiDYyA");
        let expected = PathBuf::from("Masters/2006/11/03/20061103-133939");
        let result = transform_versions_to_masters_path(input);
        assert_eq!(result, Some(expected));

        // Test with just Versions prefix (older library format, relative path)
        let input = Path::new("Versions/2014/10/05/20141005-184407/Pbd+7C%eSAuTJLOuxQjhvQ");
        let expected = PathBuf::from("Masters/2014/10/05/20141005-184407");
        let result = transform_versions_to_masters_path(input);
        assert_eq!(result, Some(expected));

        // Test with absolute path (most common in real usage)
        let input = Path::new("/mnt/photos/Library.aplibrary/Database/Versions/2009/05/05/20090505-224454/NPMbcoBPTtiuQhjqjG%FpQ");
        let expected = PathBuf::from("Masters/2009/05/05/20090505-224454");
        let result = transform_versions_to_masters_path(input);
        assert_eq!(result, Some(expected));

        // Test with different UUID format (absolute path)
        let input = Path::new("/home/user/Aperture Library.aplibrary/Versions/2014/12/20/20141220-173831/5DLLeHPLQhCFDLrRsFmSOQ");
        let expected = PathBuf::from("Masters/2014/12/20/20141220-173831");
        let result = transform_versions_to_masters_path(input);
        assert_eq!(result, Some(expected));

        // Test with invalid path (no Versions prefix)
        let input = Path::new("/path/to/Masters/2006/11/03/20061103-133939");
        let result = transform_versions_to_masters_path(input);
        assert_eq!(result, None);

        // Test with incomplete path (no UUID to strip)
        let input = Path::new("Database/Versions/2006");
        let result = transform_versions_to_masters_path(input);
        // Should return Some but with just the year
        assert!(result.is_some());
    }

    #[test]
    fn test_io_throttle_nas_safe_defaults() {
        let mut args = make_export_args();
        args.nas_safe = true;

        let throttle = IoThrottle::from_args(&args);
        assert_eq!(throttle.max_read_bytes_per_sec, Some(4 * 1024 * 1024));
        assert_eq!(throttle.max_write_bytes_per_sec, Some(4 * 1024 * 1024));
        assert_eq!(throttle.operation_delay, Duration::from_millis(20));
        assert_eq!(throttle.chunk_size, 64 * 1024);
    }

    #[test]
    fn test_io_throttle_explicit_overrides() {
        let mut args = make_export_args();
        args.nas_safe = true;
        args.max_read_mib_per_sec = Some(1.5);
        args.max_write_mib_per_sec = Some(2.5);
        args.io_delay_ms = Some(75);
        args.io_chunk_kib = Some(32);

        let throttle = IoThrottle::from_args(&args);
        assert_eq!(
            throttle.max_read_bytes_per_sec,
            Some((1.5 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(
            throttle.max_write_bytes_per_sec,
            Some((2.5 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(throttle.operation_delay, Duration::from_millis(75));
        assert_eq!(throttle.chunk_size, 32 * 1024);
    }

    #[test]
    fn test_iostats_mibps() {
        let stats = IoStats {
            read_bytes: 1024,
            written_bytes: 2 * 1024 * 1024,
            elapsed: Duration::from_secs(2),
        };
        assert!((stats.mibps() - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_sanitize_filename_component_and_fallback_token() {
        assert_eq!(sanitize_filename_component("  BW High/Contrast?  ", 48), "BW-High-Contrast");
        assert_eq!(short_uuid_token("ABCDEF12-3456"), "abcdef12");
        assert_eq!(short_uuid_token("***"), "unknown");
    }

    #[test]
    fn test_version_filename_and_dedupe() {
        let candidate = build_version_filename(
            "PICT0019",
            Some("BW High Contrast"),
            "ABCDEF12",
            "rendered.jpg",
        );
        assert_eq!(candidate, "PICT0019__BW-High-Contrast.jpg");

        let fallback = build_version_filename("PICT0019", None, "ABCDEF12", "rendered.jpg");
        assert_eq!(fallback, "PICT0019__v-abcdef12.jpg");

        let mut used = HashSet::new();
        used.insert(candidate.clone());
        let deduped = dedupe_filename(
            "PICT0019",
            Some("BW High Contrast"),
            "ABCDEF12",
            "rendered.jpg",
            &mut used,
        );
        assert_eq!(deduped, "PICT0019__BW-High-Contrast__u-abcdef12.jpg");
    }

    #[test]
    fn test_version_filename_length_is_bounded() {
        let long_master_stem = "A".repeat(300);
        let long_label = Some(&"B".repeat(300));
        let filename = build_version_filename(
            &long_master_stem,
            long_label.map(|label| label.as_str()),
            "ABCDEF12",
            "rendered.jpg",
        );

        assert!(filename.len() <= MAX_EXPORT_FILENAME_LEN);
        assert!(filename.ends_with(".jpg"));
    }

    #[test]
    fn test_initialize_checkpoint_log_preserves_existing_contents() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aplib-checkpoint-init-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be creatable");
        let log_path = temp_dir.join("export-checkpoint.log");
        fs::write(&log_path, "previous-run\n").expect("seed log should be writable");

        initialize_checkpoint_log(&log_path, &temp_dir).expect("initialization should not truncate");
        append_checkpoint_log(&log_path, "master\tabc\t/path/to/master.jpg");

        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        assert!(contents.starts_with("previous-run\n"));
        assert!(contents.contains("run\t"));
        assert!(contents.contains("master\tabc\t/path/to/master.jpg"));
    }

    #[test]
    fn test_build_export_jobs_uses_sorted_version_names() {
        let master = fixture_master();
        let master_uuid = master
            .uuid()
            .clone()
            .expect("fixture master should have uuid");

        let mut first_version = fixture_version();
        first_version.master_uuid = Some(master_uuid.clone());
        first_version.is_original = Some(false);
        first_version.version_number = Some(2);
        first_version.name = Some("Second Edit".to_string());

        let mut second_version = fixture_version();
        second_version.master_uuid = Some(master_uuid.clone());
        second_version.is_original = Some(false);
        second_version.version_number = Some(1);
        second_version.name = Some("First Edit".to_string());

        let mut master_map = HashMap::new();
        master_map.insert(master_uuid.clone(), master);

        let mut version_map = HashMap::new();
        version_map.insert("version-b".to_string(), first_version);
        version_map.insert("version-a".to_string(), second_version);

        let cache = super::super::LibraryCache {
            version_map,
            master_map,
        };

        let jobs = build_export_jobs(&cache, &fixture_library_path());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].version_uuids, vec!["version-a".to_string(), "version-b".to_string()]);
        assert_eq!(jobs[0].version_filenames[0], "PICT0019__First-Edit.JPG");
        assert_eq!(jobs[0].version_filenames[1], "PICT0019__Second-Edit.JPG");
    }

    #[test]
    fn test_build_export_jobs_skips_master_with_no_filename() {
        let mut master = fixture_master();
        let master_uuid = master
            .uuid()
            .clone()
            .expect("fixture master should have uuid");
        master.image_path = Some("/".to_string());

        let mut master_map = HashMap::new();
        master_map.insert(master_uuid, master);

        let cache = super::super::LibraryCache {
            version_map: HashMap::new(),
            master_map,
        };

        let jobs = build_export_jobs(&cache, Path::new("/tmp/Library.aplibrary"));
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_build_export_jobs_fallback_for_missing_source_directory() {
        let master = fixture_master();
        let master_uuid = master
            .uuid()
            .clone()
            .expect("fixture master should have uuid");

        let mut version = fixture_version();
        let version_uuid = version
            .uuid()
            .clone()
            .expect("fixture version should have uuid");
        version.master_uuid = Some(master_uuid.clone());
        version.is_original = Some(false);
        version.source_directory = None;

        let mut master_map = HashMap::new();
        master_map.insert(master_uuid, master);

        let mut version_map = HashMap::new();
        version_map.insert(version_uuid.clone(), version);

        let cache = super::super::LibraryCache {
            version_map,
            master_map,
        };

        let jobs = build_export_jobs(&cache, Path::new("/tmp/Library.aplibrary"));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].version_uuids, vec![version_uuid]);
        assert_eq!(jobs[0].version_paths.len(), 1);
        assert!(jobs[0].version_paths[0]
            .to_string_lossy()
            .contains("CACHE_OUT_OF_DATE"));
    }

    #[test]
    fn test_export_job_files_materializes_metadata_only_version_image() {
        let master = fixture_master();
        let master_uuid = master
            .uuid()
            .clone()
            .expect("fixture master should have uuid");

        let mut version = fixture_version();
        let version_uuid = version
            .uuid()
            .clone()
            .expect("fixture version should have uuid");
        version.master_uuid = Some(master_uuid.clone());
        version.is_original = Some(false);

        let mut master_map = HashMap::new();
        master_map.insert(master_uuid.clone(), master);

        let mut version_map = HashMap::new();
        version_map.insert(version_uuid, version);

        let cache = super::super::LibraryCache {
            version_map,
            master_map,
        };

        let jobs = build_export_jobs(&cache, &fixture_library_path());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].version_paths.len(), 1);
        assert_eq!(jobs[0].version_paths[0], jobs[0].master_path);

        let out_dir = std::env::temp_dir().join(format!(
            "aplib-metadata-only-export-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&out_dir).expect("temp output dir should be creatable");

        let io_throttle = IoThrottle::from_args(&make_export_args());
        let keyword_map: HashMap<String, String> = HashMap::new();
        let export_context = ExportContext {
            cache: &cache,
            library_root_path: &fixture_library_path(),
            flat_keyword_map: &keyword_map,
            hierarchical_keyword_map: &keyword_map,
            io_throttle: &io_throttle,
            checkpoint_log_path: None,
        };

        export_job_files(&jobs[0], &out_dir, &export_context)
            .expect("export should succeed for metadata-only version");

        let exported_master = out_dir
            .join(&jobs[0].master_rel_dir)
            .join(&jobs[0].master_filename);
        let exported_version = out_dir
            .join(&jobs[0].master_rel_dir)
            .join(&jobs[0].version_filenames[0]);

        assert!(exported_master.exists());
        assert!(exported_master.with_extension("xmp").exists());
        assert!(exported_version.exists());
        assert!(exported_version.with_extension("xmp").exists());
    }

    #[test]
    fn test_export_fixture_library_writes_expected_files_and_digikam_xmp_fields() {
        let master = fixture_master();
        let master_uuid = master
            .uuid()
            .clone()
            .expect("fixture master should have uuid");

        let original_version = fixture_version();
        let edited_version = fixture_edited_version();
        let edited_version_uuid = edited_version
            .uuid()
            .clone()
            .expect("fixture edited version should have uuid");

        let mut master_map = HashMap::new();
        master_map.insert(master_uuid.clone(), master.clone());

        let mut version_map = HashMap::new();
        version_map.insert(
            original_version
                .uuid()
                .clone()
                .expect("fixture original version should have uuid"),
            original_version,
        );
        version_map.insert(edited_version_uuid.clone(), edited_version.clone());

        let cache = super::super::LibraryCache {
            version_map,
            master_map,
        };

        let jobs = build_export_jobs(&cache, &fixture_library_path());
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].version_uuids, vec![edited_version_uuid.clone()]);
        assert_eq!(jobs[0].version_filenames.len(), 1);

        let out_dir = std::env::temp_dir().join(format!(
            "aplib-fixture-export-golden-test-{}",
            std::process::id()
        ));
        if out_dir.exists() {
            fs::remove_dir_all(&out_dir).expect("temp output dir should be removable");
        }
        fs::create_dir_all(&out_dir).expect("temp output dir should be creatable");

        let io_throttle = IoThrottle::from_args(&make_export_args());
        let keyword_map: HashMap<String, String> = HashMap::new();
        let export_context = ExportContext {
            cache: &cache,
            library_root_path: &fixture_library_path(),
            flat_keyword_map: &keyword_map,
            hierarchical_keyword_map: &keyword_map,
            io_throttle: &io_throttle,
            checkpoint_log_path: None,
        };

        export_job_files(&jobs[0], &out_dir, &export_context)
            .expect("fixture export should succeed");

        let export_dir = out_dir.join(&jobs[0].master_rel_dir);
        let exported_master = export_dir.join(&jobs[0].master_filename);
        let exported_master_sidecar = exported_master.with_extension("xmp");
        let exported_version = export_dir.join(&jobs[0].version_filenames[0]);
        let exported_version_sidecar = exported_version.with_extension("xmp");

        assert!(exported_master.exists());
        assert!(exported_master_sidecar.exists());
        assert!(exported_version.exists());
        assert!(exported_version_sidecar.exists());

        let mut exported_names = fs::read_dir(&export_dir)
            .expect("export dir should be readable")
            .map(|entry| {
                entry
                    .expect("dir entry should read")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();
        exported_names.sort();
        assert_eq!(
            exported_names,
            vec![
                jobs[0].master_filename.clone(),
                exported_master_sidecar
                    .file_name()
                    .expect("master sidecar should have filename")
                    .to_string_lossy()
                    .to_string(),
                jobs[0].version_filenames[0].clone(),
                exported_version_sidecar
                    .file_name()
                    .expect("version sidecar should have filename")
                    .to_string_lossy()
                    .to_string(),
            ]
        );

        let master_xmp = read_xmp_from_file(&exported_master_sidecar);
        assert_xmp_property_eq(&master_xmp, ns::APLIB, "MasterUUID", &master_uuid);
        let master_xmp_text = read_xmp_text(&exported_master_sidecar);
        assert!(master_xmp_text.contains("<dc:title>"));
        assert!(master_xmp_text.contains("xml:lang=\"x-default\">PICT0019</rdf:li>"));
        assert!(master_xmp_text.contains("<photoshop:Headline>PICT0019</photoshop:Headline>"));
        assert_xmp_property_eq(
            &master_xmp,
            ns::NS_PHOTOSHOP,
            "DateCreated",
            &master.create_date.expect("master create date").to_rfc3339(),
        );
        assert_xmp_property_eq(
            &master_xmp,
            ns::NS_EXIF,
            "DateTimeOriginal",
            &master.image_date.expect("master image date").to_rfc3339(),
        );

        let version_xmp = read_xmp_from_file(&exported_version_sidecar);
        let version_xmp_text = read_xmp_text(&exported_version_sidecar);
        assert_xmp_property_eq(&version_xmp, ns::NS_XMP, "VersionUUID", &edited_version_uuid);
        assert_xmp_property_eq(&version_xmp, ns::APLIB, "MasterUUID", &master_uuid);
        assert_xmp_property_eq(
            &version_xmp,
            ns::APLIB,
            "MasterFilename",
            &jobs[0].master_filename,
        );
        assert_xmp_property_eq(
            &version_xmp,
            ns::NS_XMP,
            "Rating",
            &edited_version.rating.expect("version rating").to_string(),
        );
        assert!(version_xmp_text.contains("<dc:title>"));
        assert!(version_xmp_text.contains("xml:lang=\"x-default\">PICT0019</rdf:li>"));
        assert!(version_xmp_text.contains("<photoshop:Headline>PICT0019</photoshop:Headline>"));
        assert!(version_xmp_text.contains("<digiKam:PickLabel>0</digiKam:PickLabel>"));
        assert!(version_xmp_text.contains("<digiKam:ColorLabel>0</digiKam:ColorLabel>"));
        assert_xmp_property_eq(
            &version_xmp,
            ns::NS_XMP,
            "CreateDate",
            &edited_version.create_date.expect("version create date").to_rfc3339(),
        );
        assert_xmp_property_eq(
            &version_xmp,
            ns::NS_EXIF,
            "DateTimeOriginal",
            &edited_version.image_date.expect("version image date").to_rfc3339(),
        );
    }

    #[test]
    fn test_process_export_nonexistent_library_path_does_not_panic() {
        let mut args = make_export_args();
        args.path = "/definitely/not/a/real/library.aplibrary".to_string();
        process_export(&args);
    }

    #[test]
    fn test_append_checkpoint_log_writes_entries() {
        let temp_dir = std::env::temp_dir().join(format!(
            "aplib-checkpoint-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp_dir).expect("temp dir should be creatable");
        let log_path = temp_dir.join("export-checkpoint.log");

        append_checkpoint_log(&log_path, "master\tabc\t/path/to/master.jpg");
        append_checkpoint_log(&log_path, "version\tabc\tdef\t/path/to/version.jpg");

        let contents = fs::read_to_string(&log_path).expect("log should be readable");
        assert!(contents.contains("master\tabc\t/path/to/master.jpg"));
        assert!(contents.contains("version\tabc\tdef\t/path/to/version.jpg"));
    }
}
