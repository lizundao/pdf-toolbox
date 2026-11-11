use crate::fonts::{attach_type1_fallback, prepare_font, EmbeddedFont, FontMode};
use crate::pdf_util::{
    load_document, page_count, page_mediabox, parse_page_ranges, save_document_from,
};
use image::codecs::jpeg::JpegEncoder;
use image::{ColorType, GenericImageView};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::path::Path;

#[derive(serde::Serialize)]
pub struct WatermarkResult {
    pub message: String,
    pub output_path: String,
    pub pages_affected: u32,
}

enum TextDraw<'a> {
    Embedded(&'a EmbeddedFont),
    Type1,
}

fn resolve_pages(doc: &Document, page_range: Option<&str>) -> Result<Vec<u32>, String> {
    let max = page_count(doc);
    match page_range {
        Some(s) if !s.trim().is_empty() => parse_page_ranges(s, max),
        _ => Ok((1..=max).collect()),
    }
}

fn measure_text(draw: &TextDraw, text: &str, font_size: f32) -> f32 {
    match draw {
        TextDraw::Embedded(f) => f.measure(text, font_size),
        TextDraw::Type1 => text.chars().count() as f32 * font_size * 0.5,
    }
}

fn text_object(draw: &TextDraw, text: &str) -> Object {
    match draw {
        TextDraw::Embedded(f) => f.cid_text_object(text),
        TextDraw::Type1 => Object::string_literal(text),
    }
}

fn append_page_content(doc: &mut Document, page_id: ObjectId, ops: Vec<Operation>) -> Result<(), String> {
    let content = Content { operations: ops };
    doc.add_to_page_content(page_id, content)
        .map_err(|e| e.to_string())
}

fn text_watermark_ops(
    text: &str,
    font_size: f32,
    opacity: f32,
    angle_deg: f32,
    tiled: bool,
    width: f64,
    height: f64,
    draw: &TextDraw,
) -> Vec<Operation> {
    let opacity = opacity.clamp(0.05, 1.0);
    let font_size = font_size.clamp(8.0, 120.0);
    let angle = angle_deg.to_radians();
    let cos_a = angle.cos() as f32;
    let sin_a = angle.sin() as f32;
    let text_width = measure_text(draw, text, font_size);

    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new("ca", vec![opacity.into()]),
    ];

    let mut draw_at = |x: f32, y: f32| {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new("Tf", vec!["WFont".into(), font_size.into()]));
        ops.push(Operation::new("rg", vec![0.55.into(), 0.55.into(), 0.55.into()]));
        ops.push(Operation::new(
            "Tm",
            vec![
                cos_a.into(),
                sin_a.into(),
                (-sin_a).into(),
                cos_a.into(),
                (x - text_width / 2.0).into(),
                y.into(),
            ],
        ));
        ops.push(Operation::new("Tj", vec![text_object(draw, text)]));
        ops.push(Operation::new("ET", vec![]));
    };

    if tiled {
        let step_x = (text_width * 1.8).max(140.0);
        let step_y = (font_size * 3.0).max(90.0);
        let mut y = step_y;
        while y < height as f32 {
            let mut x = step_x * 0.5;
            while x < width as f32 {
                draw_at(x, y);
                x += step_x;
            }
            y += step_y;
        }
    } else {
        draw_at((width / 2.0) as f32, (height / 2.0) as f32);
    }

    ops.push(Operation::new("Q", vec![]));
    ops
}

fn image_to_jpeg(path: &Path) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::open(path).map_err(|e| format!("无法读取图片: {}", e))?;
    let (w, h) = img.dimensions();
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut buf, 90);
    enc.encode(rgb.as_raw(), w, h, ColorType::Rgb8.into())
        .map_err(|e| e.to_string())?;
    Ok((buf, w, h))
}

fn image_position(
    position: &str,
    page_w: f64,
    page_h: f64,
    img_w: f32,
    img_h: f32,
    margin: f64,
) -> (f32, f32) {
    match position {
        "top-left" => (margin as f32, (page_h - margin - img_h as f64) as f32),
        "top-right" => (
            (page_w - margin - img_w as f64) as f32,
            (page_h - margin - img_h as f64) as f32,
        ),
        "bottom-left" => (margin as f32, margin as f32),
        "bottom-right" => ((page_w - margin - img_w as f64) as f32, margin as f32),
        _ => (
            ((page_w - img_w as f64) / 2.0) as f32,
            ((page_h - img_h as f64) / 2.0) as f32,
        ),
    }
}

fn image_watermark_ops(img_w: f32, img_h: f32, x: f32, y: f32, opacity: f32) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new("ca", vec![opacity.into()]),
        Operation::new(
            "cm",
            vec![
                img_w.into(),
                0.into(),
                0.into(),
                img_h.into(),
                x.into(),
                y.into(),
            ],
        ),
        Operation::new("Do", vec!["WmImg".into()]),
        Operation::new("Q", vec![]),
    ]
}

fn header_footer_ops(
    header: Option<&str>,
    footer: &str,
    page_num: u32,
    total: u32,
    show_date: bool,
    width: f64,
    height: f64,
    draw: &TextDraw,
) -> Vec<Operation> {
    let footer_text = footer
        .replace("{page}", &page_num.to_string())
        .replace("{total}", &total.to_string());
    let date_text = if show_date {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    } else {
        String::new()
    };

    let font_size = 10.0f32;
    let mut ops = vec![Operation::new("q", vec![])];

    if let Some(h) = header {
        if !h.is_empty() {
            let x = (width / 2.0 - measure_text(draw, h, font_size) as f64 / 2.0).max(24.0);
            ops.extend([
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["WFont".into(), font_size.into()]),
                Operation::new("rg", vec![0.3.into(), 0.3.into(), 0.3.into()]),
                Operation::new("Td", vec![x.into(), (height - 36.0).into()]),
                Operation::new("Tj", vec![text_object(draw, h)]),
                Operation::new("ET", vec![]),
            ]);
        }
    }

    if !footer_text.is_empty() {
        let x = (width / 2.0 - measure_text(draw, &footer_text, font_size) as f64 / 2.0).max(24.0);
        ops.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["WFont".into(), font_size.into()]),
            Operation::new("rg", vec![0.3.into(), 0.3.into(), 0.3.into()]),
            Operation::new("Td", vec![x.into(), 24.0.into()]),
            Operation::new("Tj", vec![text_object(draw, &footer_text)]),
            Operation::new("ET", vec![]),
        ]);
    }

    if !date_text.is_empty() {
        let x = (width - measure_text(draw, &date_text, font_size) as f64 - 24.0).max(24.0);
        ops.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["WFont".into(), font_size.into()]),
            Operation::new("rg", vec![0.3.into(), 0.3.into(), 0.3.into()]),
            Operation::new("Td", vec![x.into(), 24.0.into()]),
            Operation::new("Tj", vec![text_object(draw, &date_text)]),
            Operation::new("ET", vec![]),
        ]);
    }

    ops.push(Operation::new("Q", vec![]));
    ops
}

fn sample_text_for_header(
    header: Option<&str>,
    footer_template: &str,
    show_date: bool,
) -> String {
    let mut s = String::new();
    if let Some(h) = header {
        s.push_str(h);
    }
    s.push_str(footer_template);
    s.push_str("0123456789/- ");
    if show_date {
        s.push_str("20260101");
    }
    s
}

fn apply_text_watermark(
    doc: &mut Document,
    pages: &[u32],
    text: &str,
    font_size: f32,
    opacity: f32,
    angle: f32,
    tiled: bool,
    font_path: Option<&str>,
    face_index: u32,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("水印文字不能为空".into());
    }
    let font_mode = prepare_font(doc, font_path, face_index, text)?;
    let page_map = doc.get_pages();
    for &page_num in pages {
        let page_id = *page_map
            .get(&page_num)
            .ok_or_else(|| format!("页码 {} 不存在", page_num))?;
        let box_ = page_mediabox(doc, page_id);
        let w = box_[2] - box_[0];
        let h = box_[3] - box_[1];
        let draw = match &font_mode {
            FontMode::Embedded(f) => {
                f.attach_to_page(doc, page_id, "WFont")?;
                TextDraw::Embedded(f)
            }
            FontMode::Type1Fallback => {
                attach_type1_fallback(doc, page_id, "WFont")?;
                TextDraw::Type1
            }
        };
        let ops = text_watermark_ops(text, font_size, opacity, angle, tiled, w, h, &draw);
        append_page_content(doc, page_id, ops)?;
    }
    Ok(())
}

fn apply_image_watermark(
    doc: &mut Document,
    pages: &[u32],
    image_path: &Path,
    opacity: f32,
    scale_percent: f32,
    position: &str,
) -> Result<(), String> {
    let (jpeg, orig_w, orig_h) = image_to_jpeg(image_path)?;
    let opacity = opacity.clamp(0.05, 1.0);
    let scale = (scale_percent / 100.0).clamp(0.05, 1.0);

    let page_map = doc.get_pages();
    for &page_num in pages {
        let page_id = *page_map
            .get(&page_num)
            .ok_or_else(|| format!("页码 {} 不存在", page_num))?;
        let box_ = page_mediabox(doc, page_id);
        let page_w = box_[2] - box_[0];
        let page_h = box_[3] - box_[1];

        let target_w = (page_w * scale as f64) as f32;
        let aspect = orig_h as f32 / orig_w as f32;
        let img_w = target_w;
        let img_h = target_w * aspect;

        let img_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => orig_w as i64,
                "Height" => orig_h as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg.clone(),
        ));

        doc.add_xobject(page_id, b"WmImg", img_id)
            .map_err(|e| e.to_string())?;

        let (x, y) = image_position(position, page_w, page_h, img_w, img_h, 36.0);
        let ops = image_watermark_ops(img_w, img_h, x, y, opacity);
        append_page_content(doc, page_id, ops)?;
    }
    Ok(())
}

fn apply_header_footer(
    doc: &mut Document,
    pages: &[u32],
    header: Option<&str>,
    footer_template: &str,
    show_date: bool,
    font_path: Option<&str>,
    face_index: u32,
) -> Result<(), String> {
    let sample = sample_text_for_header(header, footer_template, show_date);
    let font_mode = prepare_font(doc, font_path, face_index, &sample)?;
    let total = page_count(doc);
    let page_map = doc.get_pages();
    for &page_num in pages {
        let page_id = *page_map
            .get(&page_num)
            .ok_or_else(|| format!("页码 {} 不存在", page_num))?;
        let box_ = page_mediabox(doc, page_id);
        let w = box_[2] - box_[0];
        let h = box_[3] - box_[1];
        let draw = match &font_mode {
            FontMode::Embedded(f) => {
                f.attach_to_page(doc, page_id, "WFont")?;
                TextDraw::Embedded(f)
            }
            FontMode::Type1Fallback => {
                attach_type1_fallback(doc, page_id, "WFont")?;
                TextDraw::Type1
            }
        };
        let ops = header_footer_ops(
            header,
            footer_template,
            page_num,
            total,
            show_date,
            w,
            h,
            &draw,
        );
        append_page_content(doc, page_id, ops)?;
    }
    Ok(())
}

#[tauri::command(async)]
pub fn add_text_watermark(
    source_path: String,
    output_path: String,
    text: String,
    font_size: f32,
    opacity: f32,
    angle: f32,
    tiled: bool,
    page_range: Option<String>,
    font_path: Option<String>,
    font_face_index: Option<u32>,
) -> Result<WatermarkResult, String> {
    let mut doc = load_document(Path::new(&source_path))?;
    let pages = resolve_pages(&doc, page_range.as_deref())?;
    apply_text_watermark(
        &mut doc,
        &pages,
        &text,
        font_size,
        opacity,
        angle,
        tiled,
        font_path.as_deref(),
        font_face_index.unwrap_or(0),
    )?;
    save_document_from(&mut doc, Path::new(&output_path), Some(Path::new(&source_path)))?;
    Ok(WatermarkResult {
        message: format!("已为 {} 页添加文字水印", pages.len()),
        output_path,
        pages_affected: pages.len() as u32,
    })
}

#[tauri::command(async)]
pub fn add_image_watermark(
    source_path: String,
    output_path: String,
    image_path: String,
    opacity: f32,
    scale_percent: f32,
    position: String,
    page_range: Option<String>,
) -> Result<WatermarkResult, String> {
    let mut doc = load_document(Path::new(&source_path))?;
    let pages = resolve_pages(&doc, page_range.as_deref())?;
    apply_image_watermark(
        &mut doc,
        &pages,
        Path::new(&image_path),
        opacity,
        scale_percent,
        &position,
    )?;
    save_document_from(&mut doc, Path::new(&output_path), Some(Path::new(&source_path)))?;
    Ok(WatermarkResult {
        message: format!("已为 {} 页添加图片水印", pages.len()),
        output_path,
        pages_affected: pages.len() as u32,
    })
}

#[tauri::command(async)]
pub fn add_page_headers(
    source_path: String,
    output_path: String,
    header_text: Option<String>,
    footer_template: String,
    show_date: bool,
    page_range: Option<String>,
    font_path: Option<String>,
    font_face_index: Option<u32>,
) -> Result<WatermarkResult, String> {
    let footer = if footer_template.trim().is_empty() {
        "{page} / {total}".to_string()
    } else {
        footer_template
    };
    let mut doc = load_document(Path::new(&source_path))?;
    let pages = resolve_pages(&doc, page_range.as_deref())?;
    apply_header_footer(
        &mut doc,
        &pages,
        header_text.as_deref(),
        &footer,
        show_date,
        font_path.as_deref(),
        font_face_index.unwrap_or(0),
    )?;
    save_document_from(&mut doc, Path::new(&output_path), Some(Path::new(&source_path)))?;
    Ok(WatermarkResult {
        message: format!("已为 {} 页添加页眉页脚", pages.len()),
        output_path,
        pages_affected: pages.len() as u32,
    })
}
