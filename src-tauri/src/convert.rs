use crate::fonts::{attach_type1_fallback, prepare_font, FontMode};
use crate::pdf_util::{
    load_document, load_document_with_password, page_count, parse_page_ranges, save_document_from,
};
use image::codecs::jpeg::JpegEncoder;
use image::{ColorType, DynamicImage, GenericImageView};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::fs;
use std::path::Path;

#[derive(serde::Serialize)]
pub struct ConvertResult {
    pub output_path: String,
    pub detail: String,
}

fn image_to_jpeg_bytes(img: DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut buf = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut buf, quality.clamp(10, 100));
    enc.encode(rgb.as_raw(), w, h, ColorType::Rgb8.into())
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn add_image_page(
    doc: &mut Document,
    pages_id: ObjectId,
    jpeg: Vec<u8>,
    w: u32,
    h: u32,
) -> Result<ObjectId, String> {
    let img_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => w as i64,
            "Height" => h as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg,
    ));

    let wf = w as f32;
    let hf = h as f32;
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    wf.into(),
                    0.into(),
                    0.into(),
                    hf.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec!["Im1".into()]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().map_err(|e| e.to_string())?,
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), (w as i64).into(), (h as i64).into()],
        "Resources" => dictionary! {
            "XObject" => dictionary! {
                "Im1" => img_id,
            },
        },
        "Contents" => content_id,
    });
    Ok(page_id)
}

fn build_images_pdf(image_paths: &[String], output: &Path) -> Result<u32, String> {
    if image_paths.is_empty() {
        return Err("至少选择一张图片".into());
    }

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for path_str in image_paths {
        let path = Path::new(path_str);
        if !path.exists() {
            return Err(format!("文件不存在: {}", path_str));
        }
        let img = image::open(path).map_err(|e| format!("无法读取 {}: {}", path_str, e))?;
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return Err(format!("无效图片尺寸: {}", path_str));
        }
        let jpeg = image_to_jpeg_bytes(img, 85)?;
        let page_id = add_image_page(&mut doc, pages_id, jpeg, w, h)?;
        kids.push(Object::Reference(page_id));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.clone(),
            "Count" => kids.len() as i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.compress();
    save_document_from(&mut doc, output, None)?;
    Ok(kids.len() as u32)
}

/// 生成文本 PDF。含中文时嵌入 TrueType 子集字体（复用水印模块能力），
/// 纯 ASCII 时使用标准 Helvetica，避免中文乱码/空白。
fn build_text_pdf(text: &str, output: &Path, font_path: Option<&str>) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("文本内容为空".into());
    }

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let needs_cjk = text.chars().any(|c| c as u32 > 127);

    let font_mode = prepare_font(&mut doc, font_path, 0, text)?;

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });

    // 将字体资源挂到页面
    match &font_mode {
        FontMode::Embedded(f) => f.attach_to_page(&mut doc, page_id, "F1")?,
        FontMode::Type1Fallback => attach_type1_fallback(&mut doc, page_id, "F1")?,
    }

    let text_object = |line: &str| -> Object {
        match &font_mode {
            FontMode::Embedded(f) => f.cid_text_object(line),
            FontMode::Type1Fallback => Object::string_literal(line),
        }
    };

    // 逐行排版，超出页高自动换页
    let lines: Vec<&str> = text.lines().collect();
    const TOP: f32 = 800.0;
    const LINE_H: f32 = 16.0;
    const BOTTOM: f32 = 50.0;
    let font_size = 12.0f32;
    let max_lines = ((TOP - BOTTOM) / LINE_H) as usize;

    let mut page_contents: Vec<Vec<Operation>> = Vec::new();
    let mut current: Vec<Operation> = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), font_size.into()]),
        Operation::new("Td", vec![50.into(), TOP.into()]),
    ];
    let mut on_page = 0usize;
    for line in &lines {
        if on_page > 0 && on_page % max_lines == 0 {
            current.push(Operation::new("ET", vec![]));
            page_contents.push(std::mem::take(&mut current));
            current = vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), font_size.into()]),
                Operation::new("Td", vec![50.into(), TOP.into()]),
            ];
        }
        if on_page % max_lines > 0 {
            current.push(Operation::new("Td", vec![0.into(), (-LINE_H).into()]));
        }
        current.push(Operation::new("Tj", vec![text_object(line)]));
        on_page += 1;
    }
    current.push(Operation::new("ET", vec![]));
    page_contents.push(current);

    // 第一页使用已创建的 page_id；后续页复制同为 A4 并挂同一字体资源
    let mut kids = vec![Object::Reference(page_id)];
    let mut contents_iter = page_contents.into_iter();
    let first_content = contents_iter.next().unwrap_or_default();
    let first_content_id = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: first_content }
            .encode()
            .map_err(|e| e.to_string())?,
    ));
    if let Ok(dict) = doc.get_object_mut(page_id).and_then(Object::as_dict_mut) {
        dict.set("Contents", first_content_id);
    }
    for ops in contents_iter {
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().map_err(|e| e.to_string())?,
        ));
        let extra_page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
        });
        match &font_mode {
            FontMode::Embedded(f) => f.attach_to_page(&mut doc, extra_page, "F1")?,
            FontMode::Type1Fallback => attach_type1_fallback(&mut doc, extra_page, "F1")?,
        }
        kids.push(Object::Reference(extra_page));
    }

    let page_count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => Object::Integer(page_count),
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.compress();
    // prepare_font 已按文本是否含非 ASCII（needs_cjk）自动选择嵌入字体或标准字体
    let _ = needs_cjk;
    save_document_from(&mut doc, output, None)
}

#[tauri::command(async)]
pub fn images_to_pdf(image_paths: Vec<String>, output_path: String) -> Result<ConvertResult, String> {
    let out = Path::new(&output_path);
    let pages = build_images_pdf(&image_paths, out)?;
    Ok(ConvertResult {
        output_path,
        detail: format!("已合成 {} 页 PDF", pages),
    })
}

#[tauri::command(async)]
pub fn extract_pdf_text(
    source_path: String,
    output_path: String,
    page_range: Option<String>,
) -> Result<ConvertResult, String> {
    let doc = load_document(Path::new(&source_path))?;
    let max = page_count(&doc);
    let pages = if let Some(ref s) = page_range {
        if s.trim().is_empty() {
            (1..=max).collect()
        } else {
            parse_page_ranges(s, max)?
        }
    } else {
        (1..=max).collect()
    };

    let text = doc.extract_text(&pages).map_err(|e| e.to_string())?;
    fs::write(&output_path, &text).map_err(|e| e.to_string())?;
    Ok(ConvertResult {
        output_path: output_path.clone(),
        detail: format!("已提取 {} 页，约 {} 字符", pages.len(), text.chars().count()),
    })
}

#[tauri::command(async)]
pub fn text_to_pdf(text: String, output_path: String) -> Result<ConvertResult, String> {
    build_text_pdf(&text, Path::new(&output_path), None)?;
    Ok(ConvertResult {
        output_path: output_path.clone(),
        detail: "已生成 PDF".into(),
    })
}

/// 读取文本：优先 UTF-8（含 BOM），失败回退 GBK/GB18030，兼容 Windows 中文文本。
fn decode_text_bytes(bytes: &[u8]) -> String {
    let trimmed = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(trimmed) {
        return s.to_string();
    }
    let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
    if !had_errors {
        return cow.into_owned();
    }
    let (cow_big, _, _) = encoding_rs::BIG5.decode(bytes);
    cow_big.into_owned()
}

#[tauri::command(async)]
pub fn text_file_to_pdf(source_path: String, output_path: String) -> Result<ConvertResult, String> {
    let bytes = fs::read(&source_path).map_err(|e| e.to_string())?;
    let text = decode_text_bytes(&bytes);
    build_text_pdf(&text, Path::new(&output_path), None)?;
    Ok(ConvertResult {
        output_path: output_path.clone(),
        detail: format!("已从 {} 生成 PDF", source_path),
    })
}

#[tauri::command(async)]
pub fn list_pdf_export_pages(
    source_path: String,
    page_range: Option<String>,
    password: Option<String>,
) -> Result<Vec<u32>, String> {
    let path = Path::new(&source_path);
    let doc = match password.as_deref().filter(|s| !s.is_empty()) {
        Some(pwd) => load_document_with_password(path, pwd)?,
        None => load_document(path)?,
    };
    let max = page_count(&doc);
    match page_range.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(s) => parse_page_ranges(s, max),
        None => Ok((1..=max).collect()),
    }
}
