use crate::pdf_util::{
    crop_pages, delete_pages_desc, insert_pdf_pages, keep_only_pages, load_document,
    merge_documents, page_count, parse_page_ranges, parse_split_groups,
    remove_blank_pages, reorder_pages_by_input, rotate_pages as apply_rotation,
    save_document_from, set_uniform_page_size, unique_output_path,
};
use std::path::{Path, PathBuf};

#[derive(serde::Serialize)]
pub struct PdfInfo {
    pub file_name: String,
    pub file_size: u64,
    pub page_count: u32,
}

#[derive(serde::Serialize)]
pub struct OperationResult {
    pub message: String,
    pub output_path: Option<String>,
    pub page_count: Option<u32>,
}

#[derive(serde::Serialize)]
pub struct SplitFileResult {
    pub path: String,
    pub pages: String,
    pub page_count: u32,
}

#[derive(serde::Serialize)]
pub struct SplitResult {
    pub files: Vec<SplitFileResult>,
}

fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into())
}

#[tauri::command(async)]
pub fn get_pdf_info(source_path: String) -> Result<PdfInfo, String> {
    let path = Path::new(&source_path);
    let doc = load_document(path)?;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    Ok(PdfInfo {
        file_name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        file_size: meta.len(),
        page_count: page_count(&doc),
    })
}

#[tauri::command(async)]
pub fn merge_pdfs(source_paths: Vec<String>, output_path: String) -> Result<OperationResult, String> {
    if source_paths.len() < 2 {
        return Err("合并至少需要 2 个 PDF 文件".into());
    }
    let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let mut doc = merge_documents(&paths)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(Path::new(&source_paths[0])))?;
    Ok(OperationResult {
        message: format!("已合并 {} 个文件", paths.len()),
        output_path: Some(output_path),
        page_count: Some(page_count(&doc)),
    })
}

#[tauri::command(async)]
pub fn split_pdf(
    source_path: String,
    output_dir: String,
    mode: String,
    page_ranges: Option<String>,
    every_n: Option<u32>,
) -> Result<SplitResult, String> {
    let src = Path::new(&source_path);
    let dir = Path::new(&output_dir);
    if !dir.is_dir() {
        return Err(format!("输出目录不存在: {}", output_dir));
    }

    let doc = load_document(src)?;
    let total = page_count(&doc);
    let stem = stem_of(src);

    let groups: Vec<Vec<u32>> = match mode.as_str() {
        "ranges" => {
            let input = page_ranges.ok_or_else(|| "请指定页码范围，如 1-3;4-6".to_string())?;
            parse_split_groups(&input, total)?
        }
        "every_n" => {
            let n = every_n.filter(|&v| v > 0).ok_or_else(|| "每 N 页须大于 0".to_string())?;
            (1..=total)
                .collect::<Vec<_>>()
                .chunks(n as usize)
                .map(|c| c.to_vec())
                .collect()
        }
        _ => return Err(format!("未知拆分模式: {}", mode)),
    };

    let mut files = Vec::new();
    for group in groups {
        if group.is_empty() {
            continue;
        }
        let label = if group.len() == 1 {
            format!("p{}", group[0])
        } else {
            format!("p{}-{}", group.first().unwrap(), group.last().unwrap())
        };
        let out = unique_output_path(dir, &stem, &label);
        let mut part = load_document(src)?;
        keep_only_pages(&mut part, &group)?;
        save_document_from(&mut part, &out, Some(src))?;
        let cnt = page_count(&part);
        files.push(SplitFileResult {
            path: out.to_string_lossy().into_owned(),
            pages: label,
            page_count: cnt,
        });
    }

    if files.is_empty() {
        return Err("未生成任何拆分文件".into());
    }

    Ok(SplitResult { files })
}

#[tauri::command(async)]
pub fn extract_pages(
    source_path: String,
    output_path: String,
    pages: String,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let max_page = page_count(&doc);
    let keep = parse_page_ranges(&pages, max_page)?;
    keep_only_pages(&mut doc, &keep)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    let cnt = page_count(&doc);
    Ok(OperationResult {
        message: format!("已提取 {} 页", cnt),
        output_path: Some(output_path),
        page_count: Some(cnt),
    })
}

#[tauri::command(async)]
pub fn delete_pages(
    source_path: String,
    output_path: String,
    pages: String,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let max_page = page_count(&doc);
    let to_delete = parse_page_ranges(&pages, max_page)?;
    delete_pages_desc(&mut doc, &to_delete, max_page)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    let cnt = page_count(&doc);
    Ok(OperationResult {
        message: format!("删除后剩余 {} 页", cnt),
        output_path: Some(output_path),
        page_count: Some(cnt),
    })
}

#[tauri::command(async)]
pub fn rotate_pdf_pages(
    source_path: String,
    output_path: String,
    pages: String,
    degrees: i32,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let max_page = page_count(&doc);
    let targets = if pages.trim().is_empty() || pages.trim() == "*" {
        doc.get_pages().keys().copied().collect()
    } else {
        parse_page_ranges(&pages, max_page)?
    };
    apply_rotation(&mut doc, &targets, degrees)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    Ok(OperationResult {
        message: format!("已旋转 {} 页（{}°）", targets.len(), degrees),
        output_path: Some(output_path),
        page_count: Some(max_page),
    })
}

#[tauri::command(async)]
pub fn reorder_pdf_pages(
    source_path: String,
    output_path: String,
    order: String,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    reorder_pages_by_input(&mut doc, &order)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    let cnt = page_count(&doc);
    Ok(OperationResult {
        message: format!("已按新顺序排列 {} 页", cnt),
        output_path: Some(output_path),
        page_count: Some(cnt),
    })
}

#[tauri::command(async)]
pub fn insert_pdf(
    target_path: String,
    insert_path: String,
    output_path: String,
    position: u32,
    pages: Option<String>,
) -> Result<OperationResult, String> {
    let cnt = insert_pdf_pages(
        Path::new(&target_path),
        Path::new(&insert_path),
        position,
        pages.as_deref(),
        Path::new(&output_path),
    )?;
    Ok(OperationResult {
        message: format!("已在位置 {} 插入页面，共 {} 页", position, cnt),
        output_path: Some(output_path),
        page_count: Some(cnt),
    })
}

#[tauri::command(async)]
pub fn crop_pdf_pages(
    source_path: String,
    output_path: String,
    pages: String,
    margin: f64,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let max_page = page_count(&doc);
    let targets = if pages.trim().is_empty() || pages.trim() == "*" {
        doc.get_pages().keys().copied().collect()
    } else {
        parse_page_ranges(&pages, max_page)?
    };
    crop_pages(&mut doc, &targets, margin)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    Ok(OperationResult {
        message: format!("已裁剪 {} 页（边距 {} pt）", targets.len(), margin),
        output_path: Some(output_path),
        page_count: Some(max_page),
    })
}

#[tauri::command(async)]
pub fn resize_pdf_pages(
    source_path: String,
    output_path: String,
    size: String,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let max_page = page_count(&doc);
    set_uniform_page_size(&mut doc, &size)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    Ok(OperationResult {
        message: format!("已将 {} 页统一为 {}", max_page, size.to_uppercase()),
        output_path: Some(output_path),
        page_count: Some(max_page),
    })
}

#[tauri::command(async)]
pub fn remove_blank_pdf_pages(
    source_path: String,
    output_path: String,
) -> Result<OperationResult, String> {
    let src = Path::new(&source_path);
    let mut doc = load_document(src)?;
    let removed = remove_blank_pages(&mut doc)?;
    let out = Path::new(&output_path);
    save_document_from(&mut doc, out, Some(src))?;
    let cnt = page_count(&doc);
    Ok(OperationResult {
        message: if removed.is_empty() {
            "未检测到空白页".into()
        } else {
            format!("已删除 {} 个空白页: {:?}", removed.len(), removed)
        },
        output_path: Some(output_path),
        page_count: Some(cnt),
    })
}
