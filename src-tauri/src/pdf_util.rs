use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

/// 若两个路径指向同一文件则返回错误，避免读入后被同路径覆盖损坏源文件。
fn ensure_distinct_source_output(src: &Path, out: &Path) -> Result<(), String> {
    let src_c = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    let out_c = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());
    if src_c == out_c {
        return Err("输出路径与源文件相同，为避免损坏原文件已阻止操作".into());
    }
    Ok(())
}

/// 原子保存：先写临时文件再 rename，避免中途失败留下半截文件。
pub fn save_document(doc: &mut Document, out: &Path) -> Result<(), String> {
    save_document_from(doc, out, None)
}

/// 带源文件保护的保存；传入 src 时会先校验源与目标不同。
pub fn save_document_from(doc: &mut Document, out: &Path, src: Option<&Path>) -> Result<(), String> {
    if let Some(src) = src {
        ensure_distinct_source_output(src, out)?;
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let tmp = out.with_extension(format!(
        "{}.tmp",
        out.extension().and_then(|e| e.to_str()).unwrap_or("pdf")
    ));
    {
        let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
        if let Err(e) = doc.save_to(&mut file) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
    }
    if let Err(e) = std::fs::rename(&tmp, out) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("写入输出文件失败: {}", e));
    }
    Ok(())
}

/// 解析页码范围，如 "1-5, 8, 10-12"。
/// 关键：在展开区间之前完成所有边界校验，避免 "1-4294967295" 先生成数十亿元素导致 OOM。
pub fn parse_page_ranges(input: &str, max_page: u32) -> Result<Vec<u32>, String> {
    let mut pages = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            let start: u32 = start
                .trim()
                .parse()
                .map_err(|_| format!("无效页码: {}", start))?;
            let end: u32 = end
                .trim()
                .parse()
                .map_err(|_| format!("无效页码: {}", end))?;
            if start == 0 || end == 0 || start > end {
                return Err(format!("无效范围: {}", part));
            }
            // 先校验上界再展开，杜绝超大区间分配
            if end > max_page {
                return Err(format!("页码 {} 超出文档总页数 {}", end, max_page));
            }
            for p in start..=end {
                pages.push(p);
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| format!("无效页码: {}", part))?;
            if p == 0 {
                return Err(format!("页码从 1 开始: {}", part));
            }
            if p > max_page {
                return Err(format!("页码 {} 超出文档总页数 {}", p, max_page));
            }
            pages.push(p);
        }
    }
    if pages.is_empty() {
        return Err("请指定至少一个页码".into());
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

/// 解析拆分范围组，如 "1-3;4-6" → [[1,2,3],[4,5,6]]
pub fn parse_split_groups(input: &str, max_page: u32) -> Result<Vec<Vec<u32>>, String> {
    let mut groups = Vec::new();
    for part in input.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        groups.push(parse_page_ranges(part, max_page)?);
    }
    if groups.is_empty() {
        return Err("请指定至少一个页码范围".into());
    }
    Ok(groups)
}

pub fn load_document(path: &Path) -> Result<Document, String> {
    if !path.exists() {
        return Err(format!("文件不存在: {}", path.display()));
    }
    Document::load(path).map_err(|e| e.to_string())
}

pub fn load_document_with_password(path: &Path, password: &str) -> Result<Document, String> {
    if !path.exists() {
        return Err(format!("文件不存在: {}", path.display()));
    }
    Document::load_with_password(path, password).map_err(|e| e.to_string())
}

pub fn page_count(doc: &Document) -> u32 {
    doc.get_pages().len() as u32
}

/// 保留指定页，删除其余页
pub fn keep_only_pages(doc: &mut Document, keep: &[u32]) -> Result<(), String> {
    let max_page = page_count(doc);
    let keep_set: std::collections::BTreeSet<u32> = keep.iter().copied().collect();
    let to_delete: Vec<u32> = doc
        .get_pages()
        .keys()
        .copied()
        .filter(|p| !keep_set.contains(p))
        .collect();
    delete_pages_desc(doc, &to_delete, max_page)?;
    doc.prune_objects();
    Ok(())
}

pub fn delete_pages_desc(doc: &mut Document, pages: &[u32], max_page: u32) -> Result<(), String> {
    let parsed = if pages.is_empty() {
        parse_page_ranges("", max_page).unwrap_or_default()
    } else {
        let mut sorted: Vec<u32> = pages.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        for &p in &sorted {
            if p == 0 || p > max_page {
                return Err(format!("页码 {} 超出范围 1-{}", p, max_page));
            }
        }
        sorted
    };
    let mut sorted = parsed;
    sorted.sort_by(|a, b| b.cmp(a));
    for p in sorted {
        doc.delete_pages(&[p]);
    }
    doc.prune_objects();
    Ok(())
}

pub fn rotate_pages(doc: &mut Document, pages: &[u32], degrees: i32) -> Result<(), String> {
    let max_page = page_count(doc);
    let targets = if pages.is_empty() {
        doc.get_pages().keys().copied().collect()
    } else {
        parse_page_ranges(
            &pages
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(","),
            max_page,
        )?
    };

    let delta = match degrees {
        90 | -270 => 90,
        180 | -180 => 180,
        270 | -90 => 270,
        _ => return Err("旋转角度须为 90、180 或 270".into()),
    };

    let page_map = doc.get_pages();
    for page_num in targets {
        let page_id = *page_map
            .get(&page_num)
            .ok_or_else(|| format!("页码 {} 不存在", page_num))?;
        let current = doc
            .get_object(page_id)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|d| d.get(b"Rotate").ok())
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0) as i32;
        let new_rotate = (current + delta).rem_euclid(360);
        let rotate_value = if new_rotate == 0 {
            None
        } else {
            Some(new_rotate)
        };
        let dict = doc.get_object_mut(page_id).and_then(Object::as_dict_mut).map_err(|e| e.to_string())?;
        if let Some(v) = rotate_value {
            dict.set("Rotate", Object::Integer(v as i64));
        } else {
            dict.remove(b"Rotate");
        }
    }
    Ok(())
}

/// 解析页面排序，如 "3,1,2,4"（须包含全部页码各一次，顺序保留）
pub fn parse_page_order(input: &str, max_page: u32) -> Result<Vec<u32>, String> {
    let mut pages = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let p: u32 = part
            .parse()
            .map_err(|_| format!("无效页码: {}", part))?;
        if p == 0 || p > max_page {
            return Err(format!("页码 {} 超出范围 1-{}", p, max_page));
        }
        pages.push(p);
    }
    if pages.len() as u32 != max_page {
        return Err(format!("须列出全部 {} 页的排序，当前 {} 个", max_page, pages.len()));
    }
    let mut seen = std::collections::BTreeSet::new();
    for &p in &pages {
        if !seen.insert(p) {
            return Err(format!("页码 {} 重复", p));
        }
    }
    Ok(pages)
}

pub fn page_mediabox(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    mediabox_of(doc, page_id)
}

fn mediabox_of(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|d| d.get(b"MediaBox").ok())
        .and_then(|o| o.as_array().ok())
        .and_then(|arr| {
            if arr.len() >= 4 {
                Some([
                    object_to_f64(&arr[0]),
                    object_to_f64(&arr[1]),
                    object_to_f64(&arr[2]),
                    object_to_f64(&arr[3]),
                ])
            } else {
                None
            }
        })
        .unwrap_or([0.0, 0.0, 595.0, 842.0])
}

fn object_to_f64(obj: &Object) -> f64 {
    match obj {
        Object::Integer(i) => *i as f64,
        Object::Real(f) => *f as f64,
        _ => 0.0,
    }
}

/// 按指定顺序重排页面
pub fn reorder_pages(doc: &mut Document, new_order: &[u32]) -> Result<(), String> {
    let max = page_count(doc);
    if new_order.len() as u32 != max {
        return Err(format!("须指定全部 {} 页的排序", max));
    }
    let mut seen = std::collections::BTreeSet::new();
    for &p in new_order {
        if p == 0 || p > max || !seen.insert(p) {
            return Err(format!("无效排序：页码须为 1-{} 且无重复", max));
        }
    }
    let page_map = doc.get_pages();
    let mut kids = Vec::with_capacity(new_order.len());
    for &p in new_order {
        let id = *page_map
            .get(&p)
            .ok_or_else(|| format!("页码 {} 不存在", p))?;
        kids.push(Object::Reference(id));
    }

    let pages_id = doc
        .catalog()
        .map_err(|e| e.to_string())?
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|e| e.to_string())?;

    let pages_dict = doc
        .get_object_mut(pages_id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    pages_dict.set("Count", Object::Integer(max as i64));
    pages_dict.set("Kids", Object::Array(kids));
    Ok(())
}

pub fn reorder_pages_by_input(doc: &mut Document, order_input: &str) -> Result<(), String> {
    let max = page_count(doc);
    let order = parse_page_order(order_input, max)?;
    reorder_pages(doc, &order)
}

/// 裁剪页面边距（统一边距，单位 pt）
pub fn crop_pages(doc: &mut Document, pages: &[u32], margin: f64) -> Result<(), String> {
    if margin < 0.0 {
        return Err("边距不能为负数".into());
    }
    let page_map = doc.get_pages();
    for &page_num in pages {
        let page_id = *page_map
            .get(&page_num)
            .ok_or_else(|| format!("页码 {} 不存在", page_num))?;
        let mb = mediabox_of(doc, page_id);
        if margin * 2.0 >= (mb[2] - mb[0]).min(mb[3] - mb[1]) {
            return Err(format!("页 {} 边距过大", page_num));
        }
        let crop = [
            mb[0] + margin,
            mb[1] + margin,
            mb[2] - margin,
            mb[3] - margin,
        ];
        let dict = doc
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|e| e.to_string())?;
        dict.set(
            "CropBox",
            Object::Array(crop.map(|v| Object::Real(v as f32)).to_vec()),
        );
    }
    Ok(())
}

/// 统一页面尺寸（仅调整 MediaBox/CropBox，不缩放内容）
pub fn set_uniform_page_size(doc: &mut Document, size_name: &str) -> Result<(), String> {
    let (w, h) = match size_name.to_lowercase().as_str() {
        "a4" => (595.0_f64, 842.0),
        "letter" => (612.0, 792.0),
        _ => return Err("支持尺寸: a4、letter".into()),
    };
    let box_arr = Object::Array(vec![
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(w as f32),
        Object::Real(h as f32),
    ]);
    for page_id in doc.get_pages().values() {
        let dict = doc
            .get_object_mut(*page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|e| e.to_string())?;
        dict.set("MediaBox", box_arr.clone());
        dict.set("CropBox", box_arr.clone());
    }
    Ok(())
}

/// 判断页面内容流是否不含任何可见绘制指令。
/// 仅跳过图形状态/文本状态类无害操作符；出现绘制类操作符即视为有内容。
fn page_has_visible_content(doc: &Document, page_id: ObjectId) -> bool {
    use lopdf::content::Content;
    for cid in doc.get_page_contents(page_id) {
        let Ok(obj) = doc.get_object(cid) else { continue };
        let Ok(stream) = obj.as_stream() else { continue };
        let Ok(content) = Content::decode(&stream.content) else {
            // 无法解析时保守视为有内容，避免误删
            return true;
        };
        for op in &content.operations {
            let name = op.operator.as_str();
            // 无绘制副作用的状态/标记操作符
            if matches!(
                name,
                "q" | "Q" | "cm" | "gs" | "w" | "J" | "j" | "M" | "ri" | "i"
                    | "BT" | "ET" | "Tc" | "Tw" | "Tz" | "TL" | "Tf" | "Tr" | "Ts"
                    | "d" | "BX" | "EX" | "MP" | "DP" | "BMC" | "BDC" | "EMC"
                    | "Tm" | "Td" | "TD" | "T*" | "Tj" | "TJ" | "'" | "\""
            ) {
                // 文本定位/显示操作符：Tj/TJ/'/" 是真正绘制文字的，需要单独区分
                if matches!(name, "Tj" | "TJ" | "'" | "\"") {
                    // 若字符串为空则不算可见内容
                    if !op.operands.is_empty() {
                        return true;
                    }
                }
                continue;
            }
            // 其余均为绘制类（re/f/S/s/B/b/Do/sh/sc/SC/... 等）
            return true;
        }
    }
    false
}

/// 检测空白页：无图片且内容流不含任何可见绘制指令。
/// 相比字节数阈值更稳健，避免误删稀疏矢量页或漏删带背景的空白页。
pub fn find_blank_pages(doc: &Document) -> Vec<u32> {
    let mut blanks = Vec::new();
    for (&page_num, &page_id) in doc.get_pages().iter() {
        let has_images = doc
            .get_page_images(page_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !has_images && !page_has_visible_content(doc, page_id) {
            blanks.push(page_num);
        }
    }
    blanks
}

pub fn remove_blank_pages(doc: &mut Document) -> Result<Vec<u32>, String> {
    let blanks = find_blank_pages(doc);
    if blanks.is_empty() {
        return Ok(blanks);
    }
    let max = page_count(doc);
    delete_pages_desc(doc, &blanks, max)?;
    Ok(blanks)
}

pub fn extract_doc(path: &Path, keep: &[u32]) -> Result<Document, String> {
    let mut doc = load_document(path)?;
    keep_only_pages(&mut doc, keep)?;
    Ok(doc)
}

/// 合并多个 PDF（基于 lopdf 官方 merge 示例简化，不含书签）
pub fn merge_documents(paths: &[PathBuf]) -> Result<Document, String> {
    if paths.is_empty() {
        return Err("至少需要一个 PDF 文件".into());
    }
    let mut docs = Vec::with_capacity(paths.len());
    for path in paths {
        docs.push(load_document(path)?);
    }
    merge_documents_from_docs(docs)
}

pub fn merge_documents_from_docs(docs: Vec<Document>) -> Result<Document, String> {
    if docs.is_empty() {
        return Err("至少需要一个 PDF 文件".into());
    }

    let mut max_id = 1u32;
    let mut documents_pages = BTreeMap::new();
    let mut documents_objects = BTreeMap::new();
    let mut document = Document::with_version("1.5");

    for mut doc in docs {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        for (_page_num, object_id) in doc.get_pages() {
            let object = doc
                .get_object(object_id)
                .map_err(|e| e.to_string())?
                .to_owned();
            documents_pages.insert(object_id, object);
        }
        documents_objects.extend(doc.objects);
    }

    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in documents_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                catalog_object = Some((
                    catalog_object.map(|(id, _)| id).unwrap_or(object_id),
                    object,
                ));
            }
            b"Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref old)) = pages_object {
                        if let Ok(old_dict) = old.as_dict() {
                            dictionary.extend(old_dict);
                        }
                    }
                    pages_object = Some((
                        pages_object.map(|(id, _)| id).unwrap_or(object_id),
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                document.objects.insert(object_id, object);
            }
        }
    }

    let (catalog_id, catalog_object) =
        catalog_object.ok_or_else(|| "合并失败: 未找到 Catalog".to_string())?;
    let (page_id, page_object) =
        pages_object.ok_or_else(|| "合并失败: 未找到 Pages".to_string())?;

    for (object_id, object) in documents_pages.iter() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", page_id);
            document
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    if let Ok(dictionary) = page_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", documents_pages.len() as u32);
        dictionary.set(
            "Kids",
            documents_pages
                .keys()
                .map(|&id| Object::Reference(id))
                .collect::<Vec<_>>(),
        );
        document
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", page_id);
        dictionary.remove(b"Outlines");
        document
            .objects
            .insert(catalog_id, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_id);
    // 用全局最大对象号推进，避免稀疏编号文档发生 ID 冲突或覆盖
    document.max_id = document.objects.keys().map(|id| id.0).max().unwrap_or(1) + 1;
    document.renumber_objects();

    Ok(document)
}

/// 在目标 PDF 指定位置插入另一 PDF 的页面
pub fn insert_pdf_pages(
    target_path: &Path,
    insert_path: &Path,
    position: u32,
    insert_pages: Option<&str>,
    output: &Path,
) -> Result<u32, String> {
    let total = page_count(&load_document(target_path)?);
    if position < 1 || position > total + 1 {
        return Err(format!("插入位置须在 1-{} 之间", total + 1));
    }

    let insert_total = page_count(&load_document(insert_path)?);
    let insert_list = if let Some(s) = insert_pages {
        if s.trim().is_empty() {
            (1..=insert_total).collect()
        } else {
            parse_page_ranges(s, insert_total)?
        }
    } else {
        (1..=insert_total).collect()
    };

    let mut parts: Vec<Document> = Vec::new();
    if position > 1 {
        parts.push(extract_doc(
            target_path,
            &(1..position).collect::<Vec<_>>(),
        )?);
    }
    parts.push(extract_doc(insert_path, &insert_list)?);
    if position <= total {
        parts.push(extract_doc(
            target_path,
            &(position..=total).collect::<Vec<_>>(),
        )?);
    }

    let mut merged = merge_documents_from_docs(parts)?;
    save_document_from(&mut merged, output, Some(target_path))?;
    Ok(page_count(&merged))
}

pub fn unique_output_path(dir: &Path, stem: &str, suffix: &str) -> PathBuf {
    let base = dir.join(format!("{}_{}.pdf", stem, suffix));
    if !base.exists() {
        return base;
    }
    for i in 1..1000 {
        let candidate = dir.join(format!("{}_{}_{}.pdf", stem, suffix, i));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{}_{}_fallback.pdf", stem, suffix))
}
