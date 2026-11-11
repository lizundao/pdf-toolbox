use crate::pdf_util::{load_document, load_document_with_password, page_count, save_document};
use crate::security::{decrypt_document, load_for_security};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use std::path::{Component, Path};

/// IPC 单文件字节上限（100 MB）。超过则拒绝，避免 Vec<u8> 经 JSON 序列化导致 OOM。
const MAX_IPC_BYTES: u64 = 100 * 1024 * 1024;

#[derive(serde::Serialize)]
pub struct InfoResult {
    pub message: String,
    pub output_path: String,
}

fn ensure_info_dict(doc: &mut Document) -> Result<ObjectId, String> {
    if let Ok(Object::Reference(id)) = doc.trailer.get(b"Info") {
        return Ok(*id);
    }
    let info_id = doc.add_object(dictionary! {
        "Producer" => Object::string_literal("PDF Toolbox"),
    });
    doc.trailer.set("Info", info_id);
    Ok(info_id)
}

fn set_meta_field(dict: &mut Dictionary, key: &[u8], value: Option<&str>) {
    match value {
        Some(s) if !s.trim().is_empty() => {
            dict.set(key, Object::string_literal(s.trim()));
        }
        _ => {
            dict.remove(key);
        }
    }
}

fn apply_metadata(
    doc: &mut Document,
    title: Option<&str>,
    author: Option<&str>,
    subject: Option<&str>,
    keywords: Option<&str>,
) -> Result<(), String> {
    let info_id = ensure_info_dict(doc)?;
    let dict = doc
        .get_object_mut(info_id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    set_meta_field(dict, b"Title", title);
    set_meta_field(dict, b"Author", author);
    set_meta_field(dict, b"Subject", subject);
    set_meta_field(dict, b"Keywords", keywords);
    dict.set(
        "ModDate",
        Object::string_literal(
            chrono::Local::now()
                .format("D:%Y%m%d%H%M%S%z")
                .to_string()
                .as_str(),
        ),
    );
    Ok(())
}

fn prepare_doc(path: &Path, password: Option<&str>) -> Result<Document, String> {
    let mut doc = load_for_security(path, password)?;
    if doc.is_encrypted() {
        let pwd = password
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "加密 PDF 需提供密码".to_string())?;
        decrypt_document(&mut doc, pwd)?;
        doc.encryption_state = None;
    }
    Ok(doc)
}

/// 拒绝包含未解析 `..` 穿越段的路径。
fn has_parent_traversal(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, Component::ParentDir))
}

#[tauri::command(async)]
pub fn set_pdf_metadata(
    source_path: String,
    output_path: String,
    title: Option<String>,
    author: Option<String>,
    subject: Option<String>,
    keywords: Option<String>,
    password: Option<String>,
) -> Result<InfoResult, String> {
    let mut doc = prepare_doc(Path::new(&source_path), password.as_deref())?;
    apply_metadata(
        &mut doc,
        title.as_deref(),
        author.as_deref(),
        subject.as_deref(),
        keywords.as_deref(),
    )?;
    save_document(&mut doc, Path::new(&output_path))?;
    Ok(InfoResult {
        message: "已更新元数据".into(),
        output_path,
    })
}

/// 读取用于应用内预览的 PDF 字节。仅允许 .pdf、限制大小、拒绝路径穿越。
#[tauri::command(async)]
pub fn read_pdf_file(source_path: String) -> Result<Vec<u8>, String> {
    let path = Path::new(&source_path);
    if has_parent_traversal(path) {
        return Err("文件路径不合法".into());
    }
    if !path.exists() {
        return Err(format!("文件不存在: {}", source_path));
    }
    if !path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
    {
        return Err("仅支持预览 PDF 文件".into());
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_IPC_BYTES {
        return Err(format!(
            "文件超过 {} MB，无法在应用内预览，请使用系统阅读器打开",
            MAX_IPC_BYTES / 1024 / 1024
        ));
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_pdf_preview_info(
    source_path: String,
    password: Option<String>,
) -> Result<serde_json::Value, String> {
    let doc = match password.as_deref().filter(|s| !s.is_empty()) {
        Some(pwd) => load_document_with_password(Path::new(&source_path), pwd)?,
        None => load_document(Path::new(&source_path))?,
    };
    let meta = std::fs::metadata(&source_path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "page_count": page_count(&doc),
        "file_size": meta.len(),
        "encrypted": doc.is_encrypted(),
    }))
}

/// 检查输出路径是否已存在，供前端在覆盖前提示确认。
#[tauri::command(async)]
pub fn check_path_exists(path: String) -> Result<bool, String> {
    let p = Path::new(&path);
    if has_parent_traversal(p) {
        return Err("路径不合法".into());
    }
    Ok(p.exists())
}

/// 写入二进制文件（PDF 转图片导出）。拒绝路径穿越、危险扩展名与超大载荷。
#[tauri::command(async)]
pub fn write_binary_file(path: String, data: Vec<u8>) -> Result<(), String> {
    let p = Path::new(&path);
    if has_parent_traversal(p) {
        return Err("输出路径不合法".into());
    }
    if data.len() as u64 > MAX_IPC_BYTES {
        return Err("单张图片超过 100 MB，无法导出".into());
    }
    let dangerous = [
        "exe", "dll", "sys", "bat", "cmd", "ps1", "scr", "msi", "js", "vbs", "jar", "com",
    ];
    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        if dangerous.iter().any(|d| ext.eq_ignore_ascii_case(d)) {
            return Err("不允许写入可执行文件类型".into());
        }
    }
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(p, data).map_err(|e| e.to_string())
}
