use crate::pdf_util::{load_document, unique_output_path};
use crate::progress::emit_progress;
use lopdf::SaveOptions;
use std::path::Path;
use tauri::AppHandle;

#[inline]
fn ratio_to_level(ratio: u8) -> u8 {
    let r = ratio.clamp(1, 100);
    (1 + (r as u32 * 8 / 100) as u8).min(9)
}

#[derive(serde::Serialize)]
pub struct CompressResult {
    pub original_size: u64,
    pub new_size: u64,
    pub saved_bytes: u64,
    pub percent_reduced: f64,
    /// 扫描增强时跳过的图片数（压缩始终为 0）
    pub skipped: u32,
}

#[derive(serde::Serialize)]
pub struct BatchItemResult {
    pub source_path: String,
    pub output_path: String,
    pub success: bool,
    pub result: Option<CompressResult>,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct BatchCompressResult {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub items: Vec<BatchItemResult>,
}

fn compress_file(src: &Path, out: &Path, ratio: u8) -> Result<CompressResult, String> {
    let original_size = std::fs::metadata(src)
        .map_err(|e| format!("无法读取源文件信息: {}", e))?
        .len();

    let mut doc = load_document(src)?;
    doc.compress();

    let level = ratio_to_level(ratio);
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .compression_level(level.into())
        .build();

    // 原子写 + 源文件保护：使用临时文件后再 rename
    let tmp = out.with_extension(format!(
        "{}.tmp",
        out.extension().and_then(|e| e.to_str()).unwrap_or("pdf")
    ));
    {
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        if let Err(e) = doc.save_with_options(&mut file, options) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
    }
    // 源≠目标校验
    let src_c = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    let out_c = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());
    if src_c == out_c {
        let _ = std::fs::remove_file(&tmp);
        return Err("输出路径与源文件相同，为避免损坏原文件已阻止操作".into());
    }
    if let Err(e) = std::fs::rename(&tmp, out) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("写入输出文件失败: {}", e));
    }

    let new_size = std::fs::metadata(out).map_err(|e| e.to_string())?.len();
    let saved = original_size.saturating_sub(new_size);
    let percent = if original_size > 0 {
        (saved as f64 / original_size as f64 * 100.0).round()
    } else {
        0.0
    };

    Ok(CompressResult {
        original_size,
        new_size,
        saved_bytes: saved,
        percent_reduced: percent,
        skipped: 0,
    })
}

#[tauri::command(async)]
pub fn compress_pdf(
    source_path: String,
    output_path: String,
    ratio: u8,
) -> Result<CompressResult, String> {
    compress_file(Path::new(&source_path), Path::new(&output_path), ratio)
}

#[tauri::command(async)]
pub fn compress_pdf_batch(
    app: AppHandle,
    source_paths: Vec<String>,
    output_dir: String,
    ratio: u8,
) -> Result<BatchCompressResult, String> {
    let dir = Path::new(&output_dir);
    if !dir.is_dir() {
        return Err(format!("输出目录不存在: {}", output_dir));
    }
    let total = source_paths.len();
    emit_progress(&app, 0, format!("准备压缩 {} 个文件", total));

    let mut items = Vec::with_capacity(total);
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (i, source_path) in source_paths.into_iter().enumerate() {
        let src = Path::new(&source_path);
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let output_path = unique_output_path(dir, &stem, "compressed");
        let output_str = output_path.to_string_lossy().into_owned();
        let pct = ((i + 1) as f32 / total as f32 * 100.0) as u8;

        match compress_file(src, &output_path, ratio) {
            Ok(result) => {
                succeeded += 1;
                items.push(BatchItemResult {
                    source_path,
                    output_path: output_str,
                    success: true,
                    result: Some(result),
                    error: None,
                });
            }
            Err(e) => {
                failed += 1;
                items.push(BatchItemResult {
                    source_path,
                    output_path: output_str,
                    success: false,
                    result: None,
                    error: Some(e),
                });
            }
        }
        emit_progress(&app, pct, format!("第 {}/{} 个：{}", i + 1, total, stem));
    }

    Ok(BatchCompressResult {
        total: items.len(),
        succeeded,
        failed,
        items,
    })
}
