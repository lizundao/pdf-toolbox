use crate::pdf_util::{page_count, page_mediabox, save_document_from};
use crate::security::{decrypt_document, load_for_security};
use lopdf::content::{Content, Operation};
use lopdf::{Document, ObjectId};
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Serialize)]
pub struct RedactResult {
    pub message: String,
    pub output_path: String,
    pub regions_applied: u32,
}

#[derive(serde::Deserialize)]
pub struct RedactRegion {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn append_page_content(doc: &mut Document, page_id: ObjectId, ops: Vec<Operation>) -> Result<(), String> {
    let content = Content { operations: ops };
    doc.add_to_page_content(page_id, content)
        .map_err(|e| e.to_string())
}

fn black_rect_ops(x: f64, y: f64, w: f64, h: f64) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new("rg", vec![0.into(), 0.into(), 0.into()]),
        Operation::new("re", vec![x.into(), y.into(), w.into(), h.into()]),
        Operation::new("f", vec![]),
        Operation::new("Q", vec![]),
    ]
}

fn clamp_region(region: &RedactRegion, page_width: f64, page_height: f64) -> Option<(f64, f64, f64, f64)> {
    if region.width <= 0.0 || region.height <= 0.0 {
        return None;
    }
    let x = region.x.max(0.0).min(page_width);
    let y = region.y.max(0.0).min(page_height);
    let right = (region.x + region.width).min(page_width);
    let top = (region.y + region.height).min(page_height);
    let w = right - x;
    let h = top - y;
    if w > 0.5 && h > 0.5 {
        Some((x, y, w, h))
    } else {
        None
    }
}

#[tauri::command(async)]
pub fn apply_pdf_redaction(
    source_path: String,
    output_path: String,
    regions: Vec<RedactRegion>,
    password: Option<String>,
) -> Result<RedactResult, String> {
    if regions.is_empty() {
        return Err("请先在预览上框选至少一处涂黑区域".into());
    }

    let mut doc = load_for_security(Path::new(&source_path), password.as_deref())?;
    if doc.is_encrypted() {
        let pwd = password
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "加密 PDF 需提供密码".to_string())?;
        decrypt_document(&mut doc, &pwd)?;
        doc.encryption_state = None;
    }

    let max_page = page_count(&doc);
    let pages: HashMap<u32, ObjectId> = doc
        .get_pages()
        .into_iter()
        .map(|(num, id)| (num as u32, id))
        .collect();

    let mut applied = 0u32;
    for region in &regions {
        if region.page < 1 || region.page > max_page as u32 {
            return Err(format!("页码 {} 超出范围（共 {} 页）", region.page, max_page));
        }
        let page_id = *pages
            .get(&region.page)
            .ok_or_else(|| format!("找不到第 {} 页", region.page))?;
        let [_, _, page_width, page_height] = page_mediabox(&doc, page_id);
        let Some((x, y, w, h)) = clamp_region(region, page_width, page_height) else {
            continue;
        };
        append_page_content(&mut doc, page_id, black_rect_ops(x, y, w, h))?;
        applied += 1;
    }

    if applied == 0 {
        return Err("涂黑区域过小或无效，请重新框选".into());
    }

    save_document_from(&mut doc, Path::new(&output_path), Some(Path::new(&source_path)))?;
    Ok(RedactResult {
        message: format!("已在 {} 处涂黑并保存", applied),
        output_path,
        regions_applied: applied,
    })
}
