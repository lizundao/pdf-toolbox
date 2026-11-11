use crate::compress::{BatchCompressResult, BatchItemResult, CompressResult};
use crate::pdf_util::{load_document, save_document_from, unique_output_path};
use crate::progress::emit_progress;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, ColorType};
use lopdf::{Document, Object, ObjectId, Stream};
use std::path::Path;
use tauri::AppHandle;

struct ImageJob {
    id: ObjectId,
    width: u32,
    height: u32,
    color_space: Option<String>,
    filters: Option<Vec<String>>,
    raw: Vec<u8>,
}

fn image_raw_bytes(stream: &Stream) -> Result<Vec<u8>, String> {
    let filters = stream.filters().map_err(|e| e.to_string())?;
    if filters.iter().any(|f| *f == b"DCTDecode") || filters.is_empty() {
        return Ok(stream.content.clone());
    }
    stream.get_plain_content().map_err(|e| e.to_string())
}

fn decode_to_dynamic(
    raw: &[u8],
    width: u32,
    height: u32,
    color_space: &Option<String>,
    filters: &Option<Vec<String>>,
) -> Result<DynamicImage, String> {
    let is_jpeg = filters
        .as_ref()
        .map(|f| f.iter().any(|x| x == "DCTDecode"))
        .unwrap_or(false)
        || image::guess_format(raw).ok() == Some(ImageFormat::Jpeg);

    if is_jpeg {
        return image::load_from_memory(raw).map_err(|e| e.to_string());
    }

    let cs = color_space.as_deref().unwrap_or("DeviceRGB");
    match cs {
        "DeviceRGB" | "RGB" => {
            let need = (width as usize) * (height as usize) * 3;
            if raw.len() < need {
                return Err("RGB 数据不足".into());
            }
            ImageBuffer::from_raw(width, height, raw[..need].to_vec())
                .map(DynamicImage::ImageRgb8)
                .ok_or_else(|| "无法构建 RGB 图像".into())
        }
        "DeviceGray" | "Gray" => {
            let need = (width as usize) * (height as usize);
            if raw.len() < need {
                return Err("灰度数据不足".into());
            }
            ImageBuffer::from_raw(width, height, raw[..need].to_vec())
                .map(DynamicImage::ImageLuma8)
                .ok_or_else(|| "无法构建灰度图像".into())
        }
        other => Err(format!("暂不支持色彩空间: {}", other)),
    }
}

fn encode_jpeg(
    img: DynamicImage,
    quality: u8,
    grayscale: bool,
    max_dimension: u32,
) -> Result<(Vec<u8>, u32, u32, bool), String> {
    let mut img = img;
    if max_dimension > 0 {
        let (w, h) = img.dimensions();
        let longest = w.max(h);
        if longest > max_dimension {
            let scale = max_dimension as f32 / longest as f32;
            let nw = ((w as f32 * scale).round() as u32).max(1);
            let nh = ((h as f32 * scale).round() as u32).max(1);
            img = img.resize(nw, nh, image::imageops::FilterType::Lanczos3);
        }
    }

    let use_gray = grayscale;
    let mut buf = Vec::new();
    let q = quality.clamp(10, 100);
    let mut enc = JpegEncoder::new_with_quality(&mut buf, q);

    if use_gray {
        let luma = img.grayscale().to_luma8();
        let (w, h) = luma.dimensions();
        enc.encode(luma.as_raw(), w, h, ColorType::L8.into())
            .map_err(|e| e.to_string())?;
        Ok((buf, w, h, true))
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        enc.encode(rgb.as_raw(), w, h, ColorType::Rgb8.into())
            .map_err(|e| e.to_string())?;
        Ok((buf, w, h, false))
    }
}

/// 返回 (已压缩张数, 跳过张数)。跳过的图片会明确反馈给用户。
fn recompress_images(
    doc: &mut Document,
    quality: u8,
    max_dimension: u32,
    grayscale: bool,
) -> Result<(u32, u32), String> {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut jobs = Vec::new();
    let mut skipped = 0u32;

    for page_id in page_ids {
        let images = doc.get_page_images(page_id).map_err(|e| e.to_string())?;
        for img in images {
            if img
                .filters
                .as_ref()
                .map(|f| f.iter().any(|x| x == "JPXDecode"))
                .unwrap_or(false)
            {
                skipped += 1;
                continue;
            }
            let raw = match doc.get_object(img.id).and_then(Object::as_stream) {
                Ok(stream) => match image_raw_bytes(stream) {
                    Ok(b) => b,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                },
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            jobs.push(ImageJob {
                id: img.id,
                width: img.width.max(1) as u32,
                height: img.height.max(1) as u32,
                color_space: img.color_space.clone(),
                filters: img.filters.clone(),
                raw,
            });
        }
    }

    let mut done = 0u32;
    for job in jobs {
        let decoded = match decode_to_dynamic(
            &job.raw,
            job.width,
            job.height,
            &job.color_space,
            &job.filters,
        ) {
            Ok(d) => d,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let (jpeg, w, h, is_gray) = match encode_jpeg(decoded, quality, grayscale, max_dimension) {
            Ok(v) => v,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(job.id) {
            stream.dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            stream.dict.remove(b"DecodeParms");
            stream.dict.set("Width", Object::Integer(w as i64));
            stream.dict.set("Height", Object::Integer(h as i64));
            stream.dict.set("BitsPerComponent", Object::Integer(8));
            stream.dict.set(
                "ColorSpace",
                Object::Name(
                    (if is_gray { "DeviceGray" } else { "DeviceRGB" }).as_bytes().to_vec(),
                ),
            );
            stream.set_content(jpeg);
            done += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((done, skipped))
}

fn scan_compress_file(
    src: &Path,
    out: &Path,
    quality: u8,
    max_dimension: u32,
    grayscale: bool,
) -> Result<CompressResult, String> {
    let original_size = std::fs::metadata(src)
        .map_err(|e| format!("无法读取源文件信息: {}", e))?
        .len();
    let mut doc = load_document(src)?;
    let (images_done, skipped) = recompress_images(&mut doc, quality, max_dimension, grayscale)?;
    if images_done == 0 {
        return Err("未找到可重压缩的图片（可能为纯文字 PDF 或 JPEG2000 图片）".into());
    }
    doc.compress();
    save_document_from(&mut doc, out, Some(src))?;

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
        skipped,
    })
}

#[tauri::command(async)]
pub fn compress_pdf_images(
    source_path: String,
    output_path: String,
    quality: u8,
    max_dimension: u32,
    grayscale: bool,
) -> Result<CompressResult, String> {
    scan_compress_file(
        Path::new(&source_path),
        Path::new(&output_path),
        quality,
        max_dimension,
        grayscale,
    )
}

#[tauri::command(async)]
pub fn compress_pdf_images_batch(
    app: AppHandle,
    source_paths: Vec<String>,
    output_dir: String,
    quality: u8,
    max_dimension: u32,
    grayscale: bool,
) -> Result<BatchCompressResult, String> {
    let dir = Path::new(&output_dir);
    if !dir.is_dir() {
        return Err(format!("输出目录不存在: {}", output_dir));
    }
    let total = source_paths.len();
    emit_progress(&app, 0, format!("准备处理 {} 个文件", total));

    let mut items = Vec::with_capacity(total);
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for (i, source_path) in source_paths.into_iter().enumerate() {
        let src = Path::new(&source_path);
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let output_path = unique_output_path(dir, &stem, "scan");
        let output_str = output_path.to_string_lossy().into_owned();
        let pct = ((i + 1) as f32 / total as f32 * 100.0) as u8;

        match scan_compress_file(src, &output_path, quality, max_dimension, grayscale) {
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
