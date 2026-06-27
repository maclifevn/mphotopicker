use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

/// Các đuôi RAW phổ biến của máy ảnh.
const RAW_EXTS: &[&str] = &[
    "nef", "cr2", "cr3", "arw", "raf", "orf", "rw2", "dng", "pef", "srw", "x3f", "raw", "rwl",
    "nrw", "kdc", "dcr", "mrw", "3fr", "mef", "iiq", "gpr",
];
const JPG_EXTS: &[&str] = &["jpg", "jpeg", "jpe", "jfif"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRequest {
    source: String,
    dest: String,
    /// Các dòng thô từ textarea (có thể có/không có đuôi).
    names: Vec<String>,
    /// "all" | "jpg" | "raw" | "raw_jpg" | "custom"
    ext_mode: String,
    /// Dùng khi ext_mode == "custom" (vd ["jpg","png"]).
    #[serde(default)]
    custom_exts: Vec<String>,
    /// Quét đệ quy thư mục con.
    recursive: bool,
    /// "skip" | "rename" | "overwrite"
    on_conflict: String,
    /// Chỉ đếm khớp, không chép (nút "Quét khớp").
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFile {
    file_name: String,
    /// đường dẫn nguồn tương đối (để hiển thị)
    rel_path: String,
    size: u64,
    /// "copied" | "renamed" | "skipped" | "overwritten"
    action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameResult {
    name: String,
    count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterResult {
    requested: usize,
    matched_names: usize,
    copied_count: usize,
    skipped_count: usize,
    total_bytes: u64,
    names: Vec<NameResult>,
    copied: Vec<CopiedFile>,
    not_found: Vec<String>,
    errors: Vec<String>,
    dry_run: bool,
}

#[derive(Clone, Serialize)]
struct Progress {
    phase: String,
    done: usize,
    total: usize,
}

/// Tách `(basename_lowercase, ext_lowercase)` từ một tên/đường dẫn.
fn stem_and_ext(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    (stem, ext)
}

/// Tập đuôi cho phép. Trả về None nghĩa là "tất cả".
fn allowed_exts(req: &FilterRequest) -> Option<Vec<String>> {
    match req.ext_mode.as_str() {
        "all" => None,
        "jpg" => Some(JPG_EXTS.iter().map(|s| s.to_string()).collect()),
        "raw" => Some(RAW_EXTS.iter().map(|s| s.to_string()).collect()),
        "raw_jpg" => Some(
            JPG_EXTS
                .iter()
                .chain(RAW_EXTS.iter())
                .map(|s| s.to_string())
                .collect(),
        ),
        "custom" => Some(
            req.custom_exts
                .iter()
                .map(|e| e.trim().trim_start_matches('.').to_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
        ),
        _ => None,
    }
}

/// Tìm đường dẫn đích không trùng bằng cách thêm hậu tố _1, _2, ...
fn unique_dest(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(file_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name);
    let ext = p.extension().and_then(|s| s.to_str());
    let mut i = 1;
    loop {
        let name = match ext {
            Some(e) => format!("{stem}_{i}.{e}"),
            None => format!("{stem}_{i}"),
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

#[tauri::command]
pub fn write_report(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| format!("Không lưu được báo cáo: {e}"))
}

/// Mở thư mục bằng trình quản lý file của hệ điều hành.
#[tauri::command]
pub fn open_dir(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err("Thư mục không tồn tại.".into());
    }
    #[cfg(target_os = "macos")]
    let res = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(target_os = "windows")]
    let res = std::process::Command::new("explorer").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let res = std::process::Command::new("xdg-open").arg(&path).spawn();
    res.map(|_| ())
        .map_err(|e| format!("Không mở được thư mục: {e}"))
}

#[tauri::command]
pub fn filter_photos(app: AppHandle, req: FilterRequest) -> Result<FilterResult, String> {
    run_filter(&req, |phase, done, total| {
        let _ = app.emit(
            "filter-progress",
            Progress {
                phase: phase.to_string(),
                done,
                total,
            },
        );
    })
}

/// Lõi xử lý, tách khỏi Tauri để test được. `progress(phase, done, total)`.
fn run_filter<F: Fn(&str, usize, usize)>(
    req: &FilterRequest,
    progress: F,
) -> Result<FilterResult, String> {
    let source = PathBuf::from(&req.source);
    let dest = PathBuf::from(&req.dest);

    if req.source.trim().is_empty() || !source.is_dir() {
        return Err("Thư mục nguồn không hợp lệ.".into());
    }
    if !req.dry_run {
        if req.dest.trim().is_empty() {
            return Err("Chưa chọn thư mục đích.".into());
        }
        if !dest.exists() {
            fs::create_dir_all(&dest).map_err(|e| format!("Không tạo được thư mục đích: {e}"))?;
        }
    }

    // 1. Chuẩn hoá danh sách tên -> map basename -> số file khớp.
    let mut wanted: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new(); // giữ thứ tự nhập
    for line in &req.names {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (stem, _) = stem_and_ext(Path::new(line));
        if stem.is_empty() {
            continue;
        }
        if !wanted.contains_key(&stem) {
            order.push(stem.clone());
            wanted.insert(stem, 0);
        }
    }
    if wanted.is_empty() {
        return Err("Danh sách tên ảnh đang trống.".into());
    }

    let allowed = allowed_exts(&req);
    let ext_ok = |ext: &str| -> bool {
        match &allowed {
            None => true,
            Some(list) => list.iter().any(|e| e == ext),
        }
    };

    let max_depth = if req.recursive { usize::MAX } else { 1 };

    // 2. Quét nguồn, thu thập các file khớp.
    let mut matches: Vec<PathBuf> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for entry in WalkDir::new(&source).max_depth(max_depth).into_iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Lỗi đọc: {e}"));
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let (stem, ext) = stem_and_ext(path);
        if let Some(count) = wanted.get_mut(&stem) {
            if ext_ok(&ext) {
                *count += 1;
                matches.push(path.to_path_buf());
            }
        }
    }

    let total = matches.len();
    progress("scan", total, total);

    // 3. Chép (trừ khi dry_run).
    let mut copied: Vec<CopiedFile> = Vec::new();
    let mut skipped_count = 0usize;
    let mut total_bytes = 0u64;

    for (i, path) in matches.iter().enumerate() {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let rel_path = path
            .strip_prefix(&source)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        if req.dry_run {
            copied.push(CopiedFile {
                file_name,
                rel_path,
                size,
                action: "match".into(),
            });
            continue;
        }

        let target = dest.join(&file_name);
        let (target, action) = if target.exists() {
            match req.on_conflict.as_str() {
                "skip" => {
                    skipped_count += 1;
                    copied.push(CopiedFile {
                        file_name,
                        rel_path,
                        size,
                        action: "skipped".into(),
                    });
                    progress("copy", i + 1, total);
                    continue;
                }
                "overwrite" => (target, "overwritten"),
                _ => (unique_dest(&dest, &file_name), "renamed"),
            }
        } else {
            (target, "copied")
        };

        match fs::copy(path, &target) {
            Ok(_) => {
                total_bytes += size;
                let final_name = target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&file_name)
                    .to_string();
                copied.push(CopiedFile {
                    file_name: final_name,
                    rel_path,
                    size,
                    action: action.into(),
                });
            }
            Err(e) => errors.push(format!("Lỗi chép {file_name}: {e}")),
        }

        progress("copy", i + 1, total);
    }

    // 4. Tổng hợp kết quả theo tên.
    let mut names: Vec<NameResult> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();
    let mut matched_names = 0usize;
    for name in &order {
        let count = *wanted.get(name).unwrap_or(&0);
        if count > 0 {
            matched_names += 1;
        } else {
            not_found.push(name.clone());
        }
        names.push(NameResult {
            name: name.clone(),
            count,
        });
    }

    let copied_count = copied.iter().filter(|c| c.action != "skipped").count();

    Ok(FilterResult {
        requested: order.len(),
        matched_names,
        copied_count: if req.dry_run { total } else { copied_count },
        skipped_count,
        total_bytes,
        names,
        copied,
        not_found,
        errors,
        dry_run: req.dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mphotopicker_test_{}_{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &PathBuf, bytes: &[u8]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn req(source: &PathBuf, dest: &PathBuf, names: &[&str], ext_mode: &str, recursive: bool, dry: bool) -> FilterRequest {
        FilterRequest {
            source: source.to_string_lossy().to_string(),
            dest: dest.to_string_lossy().to_string(),
            names: names.iter().map(|s| s.to_string()).collect(),
            ext_mode: ext_mode.to_string(),
            custom_exts: vec![],
            recursive,
            on_conflict: "rename".to_string(),
            dry_run: dry,
        }
    }

    fn setup_source(tag: &str) -> PathBuf {
        let src = tmp_root(tag);
        touch(&src.join("DSC_0001.JPG"), b"jpg1");
        touch(&src.join("DSC_0001.NEF"), b"raw-bytes-bigger");
        touch(&src.join("DSC_0002.jpg"), b"jpg2");
        touch(&src.join("IMG_2451.png"), b"png");
        touch(&src.join("sub/DSC_0003.jpg"), b"jpg3-in-sub");
        src
    }

    #[test]
    fn matches_basename_ignoring_extension_and_case() {
        let src = setup_source("base");
        let dest = tmp_root("base_dest");
        // 'all' mode, names with/without extension, lowercase input vs uppercase file
        let r = run_filter(
            &req(&src, &dest, &["dsc_0001", "DSC_0002.jpg", "IMG_2451", "NOPE_9999"], "all", true, false),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(r.requested, 4);
        assert_eq!(r.matched_names, 3, "3 of 4 names should match");
        assert_eq!(r.not_found, vec!["nope_9999".to_string()]);
        // DSC_0001 -> JPG + NEF (2 files), DSC_0002 -> 1, IMG_2451 -> 1 = 4 copied
        assert_eq!(r.copied_count, 4);
        assert!(dest.join("DSC_0001.JPG").exists());
        assert!(dest.join("DSC_0001.NEF").exists());
        // originals untouched
        assert!(src.join("DSC_0001.JPG").exists());
    }

    #[test]
    fn jpg_mode_excludes_raw() {
        let src = setup_source("jpg");
        let dest = tmp_root("jpg_dest");
        let r = run_filter(&req(&src, &dest, &["DSC_0001"], "jpg", false, false), |_, _, _| {}).unwrap();
        assert_eq!(r.copied_count, 1, "only the JPG, not the NEF");
        assert!(dest.join("DSC_0001.JPG").exists());
        assert!(!dest.join("DSC_0001.NEF").exists());
    }

    #[test]
    fn raw_jpg_mode_takes_both() {
        let src = setup_source("rawjpg");
        let dest = tmp_root("rawjpg_dest");
        let r = run_filter(&req(&src, &dest, &["DSC_0001"], "raw_jpg", false, false), |_, _, _| {}).unwrap();
        assert_eq!(r.copied_count, 2);
    }

    #[test]
    fn recursive_toggle_controls_subfolders() {
        let src = setup_source("rec");
        let dest_off = tmp_root("rec_off");
        let off = run_filter(&req(&src, &dest_off, &["DSC_0003"], "all", false, false), |_, _, _| {}).unwrap();
        assert_eq!(off.matched_names, 0, "subfolder file not found when non-recursive");

        let dest_on = tmp_root("rec_on");
        let on = run_filter(&req(&src, &dest_on, &["DSC_0003"], "all", true, false), |_, _, _| {}).unwrap();
        assert_eq!(on.matched_names, 1);
        assert!(dest_on.join("DSC_0003.jpg").exists());
    }

    #[test]
    fn dry_run_copies_nothing() {
        let src = setup_source("dry");
        let dest = tmp_root("dry_dest");
        let r = run_filter(&req(&src, &dest, &["DSC_0001"], "all", false, true), |_, _, _| {}).unwrap();
        assert!(r.dry_run);
        assert_eq!(r.matched_names, 1);
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0, "dest stays empty on dry run");
    }

    #[test]
    fn conflict_rename_keeps_both() {
        let src = setup_source("conf");
        let dest = tmp_root("conf_dest");
        // pre-existing file with same name in dest
        touch(&dest.join("DSC_0002.jpg"), b"existing");
        let r = run_filter(&req(&src, &dest, &["DSC_0002"], "all", false, false), |_, _, _| {}).unwrap();
        assert_eq!(r.copied_count, 1);
        assert!(dest.join("DSC_0002.jpg").exists());
        assert!(dest.join("DSC_0002_1.jpg").exists(), "renamed copy should exist");
    }

    #[test]
    fn duplicate_names_deduped() {
        let src = setup_source("dup");
        let dest = tmp_root("dup_dest");
        let r = run_filter(&req(&src, &dest, &["DSC_0001", "dsc_0001", "DSC_0001.nef"], "jpg", false, true), |_, _, _| {}).unwrap();
        assert_eq!(r.requested, 1, "same basename collapses to one request");
    }
}
