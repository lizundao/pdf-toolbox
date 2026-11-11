use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use subsetter::{subset, GlyphRemapper};
use ttf_parser::{Face, GlyphId};

/// 系统字体列表缓存：全进程只扫描一次，避免反复递归解析系统字体目录造成卡顿。
static SYSTEM_FONTS_CACHE: OnceLock<Vec<SystemFontEntry>> = OnceLock::new();

#[derive(Clone, serde::Serialize)]
pub struct SystemFontEntry {
    pub label: String,
    pub path: String,
    pub face_index: u32,
}

pub struct EmbeddedFont {
    pub type0_id: ObjectId,
    original_data: Vec<u8>,
    face_index: u32,
    remapper: GlyphRemapper,
    units_per_em: f32,
}

pub enum FontMode {
    Embedded(EmbeddedFont),
    Type1Fallback,
}

fn pdf_font_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect();
    if s.is_empty() {
        s = "WMFont".into();
    }
    format!("WM{s}")
}

fn collect_glyph_ids(face: &Face, text: &str) -> HashSet<u16> {
    let mut gids = HashSet::new();
    gids.insert(0);
    for ch in text.chars() {
        if let Some(gid) = face.glyph_index(ch) {
            gids.insert(gid.0);
        }
    }
    for ch in "0123456789/- ".chars() {
        if let Some(gid) = face.glyph_index(ch) {
            gids.insert(gid.0);
        }
    }
    gids
}

fn build_to_unicode_cmap(face: &Face, text: &str, remapper: &GlyphRemapper) -> Vec<u8> {
    let mut seen = HashSet::new();
    let mut pairs = Vec::new();
    for ch in text.chars().chain("0123456789/- ".chars()) {
        if let Some(gid) = face.glyph_index(ch) {
            if let Some(cid) = remapper.get(gid.0) {
                if seen.insert(cid) {
                    pairs.push((cid, ch as u32));
                }
            }
        }
    }

    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n",
    );
    out.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (cid, uni) in &pairs {
        out.push_str(&format!("<{:04X}> <{:04X}>\n", cid, uni));
    }
    out.push_str(
        "endbfchar\n\
         endcmap\n\
         CMapName currentdict /CMap defineresource pop\n\
         end\n\
         end\n",
    );
    out.into_bytes()
}

fn build_width_array(face: &Face, gids: &HashSet<u16>, remapper: &GlyphRemapper) -> Object {
    let mut max_cid = 0u16;
    for gid in gids {
        if let Some(cid) = remapper.get(*gid) {
            max_cid = max_cid.max(cid);
        }
    }
    let mut widths = vec![500_i64; (max_cid as usize) + 1];
    for gid in gids {
        if let Some(cid) = remapper.get(*gid) {
            let advance = face.glyph_hor_advance(GlyphId(*gid)).unwrap_or(500);
            let w = (advance as i64 * 1000 / face.units_per_em() as i64).max(100);
            widths[cid as usize] = w;
        }
    }
    Object::Array(vec![
        0.into(),
        Object::Array(widths.into_iter().map(Object::from).collect()),
    ])
}

pub fn embed_truetype_font(
    doc: &mut Document,
    font_path: &Path,
    face_index: u32,
    text: &str,
) -> Result<EmbeddedFont, String> {
    let data = std::fs::read(font_path).map_err(|e| format!("无法读取字体文件: {}", e))?;
    let face = Face::parse(&data, face_index).map_err(|e| format!("字体格式无效: {:?}", e))?;

    let gids = collect_glyph_ids(&face, text);
    let mut remapper = GlyphRemapper::new();
    for gid in &gids {
        remapper.remap(*gid);
    }

    let subset_data =
        subset(&data, face_index, &remapper).map_err(|e| format!("字体子集化失败: {:?}", e))?;

    let name = pdf_font_name(
        &face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Font".to_string()),
    );

    let bbox = face.global_bounding_box();
    let upem = face.units_per_em() as i64;

    let font_file_id = doc.add_object(Stream::new(
        dictionary! { "Length1" => subset_data.len() as i64 },
        subset_data,
    ));

    let font_descriptor_id = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => name.clone(),
        "Flags" => 4i64,
        "FontBBox" => vec![bbox.x_min.into(), bbox.y_min.into(), bbox.x_max.into(), bbox.y_max.into()],
        "ItalicAngle" => 0i64,
        "Ascent" => (face.ascender() as i64 * 1000 / upem),
        "Descent" => (face.descender() as i64 * 1000 / upem),
        "CapHeight" => (face.capital_height().unwrap_or(face.ascender()) as i64 * 1000 / upem),
        "StemV" => 80i64,
        "FontFile2" => font_file_id,
    });

    let cid_font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => name.clone(),
        "CIDSystemInfo" => dictionary! {
            "Registry" => "Adobe",
            "Ordering" => "Identity",
            "Supplement" => 0,
        },
        "FontDescriptor" => font_descriptor_id,
        "W" => build_width_array(&face, &gids, &remapper),
        "CIDToGIDMap" => "Identity",
    });

    let to_unicode = build_to_unicode_cmap(&face, text, &remapper);
    let to_unicode_id = doc.add_object(Stream::new(dictionary! {}, to_unicode));

    let type0_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => name,
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cid_font_id)],
        "ToUnicode" => to_unicode_id,
    });

    let units_per_em = face.units_per_em() as f32;

    Ok(EmbeddedFont {
        type0_id,
        original_data: data,
        face_index,
        remapper,
        units_per_em,
    })
}

impl EmbeddedFont {
    pub fn measure(&self, text: &str, font_size: f32) -> f32 {
        let Ok(face) = Face::parse(&self.original_data, self.face_index) else {
            return text.chars().count() as f32 * font_size * 0.5;
        };
        let scale = font_size / self.units_per_em;
        text.chars()
            .filter_map(|ch| face.glyph_index(ch))
            .map(|gid| face.glyph_hor_advance(gid).unwrap_or(0) as f32 * scale)
            .sum()
    }

    pub fn cid_text_object(&self, text: &str) -> Object {
        let Ok(face) = Face::parse(&self.original_data, self.face_index) else {
            return Object::string_literal(text);
        };
        let mut bytes = Vec::new();
        for ch in text.chars() {
            if let Some(gid) = face.glyph_index(ch) {
                if let Some(cid) = self.remapper.get(gid.0) {
                    bytes.push((cid >> 8) as u8);
                    bytes.push((cid & 0xFF) as u8);
                }
            }
        }
        Object::String(bytes, lopdf::StringFormat::Hexadecimal)
    }

    pub fn attach_to_page(
        &self,
        doc: &mut Document,
        page_id: ObjectId,
        resource_name: &str,
    ) -> Result<(), String> {
        let resources = doc
            .get_or_create_resources(page_id)
            .map_err(|e| e.to_string())?;
        let dict = resources.as_dict_mut().map_err(|e| e.to_string())?;
        if !dict.has(b"Font") {
            dict.set("Font", Dictionary::new());
        }
        let fonts = dict
            .get_mut(b"Font")
            .and_then(Object::as_dict_mut)
            .map_err(|e| e.to_string())?;
        fonts.set(resource_name, Object::Reference(self.type0_id));
        Ok(())
    }
}

pub fn attach_type1_fallback(
    doc: &mut Document,
    page_id: ObjectId,
    resource_name: &str,
) -> Result<(), String> {
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources = doc
        .get_or_create_resources(page_id)
        .map_err(|e| e.to_string())?;
    let dict = resources.as_dict_mut().map_err(|e| e.to_string())?;
    if !dict.has(b"Font") {
        dict.set("Font", Dictionary::new());
    }
    let fonts = dict
        .get_mut(b"Font")
        .and_then(Object::as_dict_mut)
        .map_err(|e| e.to_string())?;
    fonts.set(resource_name, Object::Reference(font_id));
    Ok(())
}

fn font_label(face: &Face) -> String {
    let family = face
        .names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
        .and_then(|n| n.to_string());
    let full = face
        .names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
        .and_then(|n| n.to_string());
    match (family, full) {
        (Some(f), Some(n)) if f != n => format!("{f} ({n})"),
        (Some(f), _) | (_, Some(f)) => f,
        _ => "Unknown".into(),
    }
}

fn scan_font_file(path: &Path) -> Vec<SystemFontEntry> {
    let Ok(data) = std::fs::read(path) else {
        return vec![];
    };
    let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let mut entries = Vec::new();
    for idx in 0..count {
        let Ok(face) = Face::parse(&data, idx) else {
            continue;
        };
        let label = if count > 1 {
            format!("{} #{}", font_label(&face), idx + 1)
        } else {
            font_label(&face)
        };
        entries.push(SystemFontEntry {
            label,
            path: path.to_string_lossy().into_owned(),
            face_index: idx,
        });
    }
    entries
}

fn font_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    {
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(w).join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(h).join(".local/share/fonts"));
            dirs.push(PathBuf::from(h).join(".fonts"));
        }
    }
    dirs
}

fn push_fonts_from_dir(dir: &Path, out: &mut Vec<SystemFontEntry>, seen: &mut HashMap<String, ()>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            push_fonts_from_dir(&path, out, seen);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "ttf" | "ttc" | "otf") {
            continue;
        }
        let key = path.to_string_lossy().to_string();
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key, ());
        out.extend(scan_font_file(&path));
    }
}

fn sort_fonts(entries: &mut [SystemFontEntry]) {
    entries.sort_by(|a, b| {
        let score = |label: &str| -> i32 {
            let l = label.to_lowercase();
            if l.contains("yahei") || l.contains("雅黑") {
                return 0;
            }
            if l.contains("simsun") || l.contains("宋体") {
                return 1;
            }
            if l.contains("simhei") || l.contains("黑体") {
                return 2;
            }
            if l.contains("pingfang") || l.contains("苹方") {
                return 3;
            }
            if l.contains("noto sans cjk") || l.contains("source han") {
                return 4;
            }
            100
        };
        score(&a.label)
            .cmp(&score(&b.label))
            .then_with(|| a.label.cmp(&b.label))
    });
}

fn scan_system_fonts() -> Vec<SystemFontEntry> {
    let mut entries = Vec::new();
    let mut seen = HashMap::new();
    for dir in font_directories() {
        if dir.is_dir() {
            push_fonts_from_dir(&dir, &mut entries, &mut seen);
        }
    }
    sort_fonts(&mut entries);
    entries
}

pub fn system_fonts() -> &'static [SystemFontEntry] {
    SYSTEM_FONTS_CACHE.get_or_init(scan_system_fonts)
}

#[tauri::command(async)]
pub fn list_system_fonts() -> Result<Vec<SystemFontEntry>, String> {
    Ok(system_fonts().to_vec())
}

pub fn default_chinese_font() -> Option<(PathBuf, u32)> {
    let fonts = system_fonts();
    fonts.iter().find_map(|f| {
        let l = f.label.to_lowercase();
        if l.contains("yahei")
            || l.contains("雅黑")
            || l.contains("simsun")
            || l.contains("宋体")
            || l.contains("simhei")
            || l.contains("黑体")
        {
            Some((PathBuf::from(&f.path), f.face_index))
        } else {
            None
        }
    })
}

pub fn resolve_font_path(
    path: Option<&str>,
    face_index: u32,
    text: &str,
) -> Result<(PathBuf, u32), String> {
    if let Some(p) = path.filter(|s| !s.trim().is_empty()) {
        let pb = PathBuf::from(p);
        if !pb.exists() {
            return Err(format!("字体文件不存在: {p}"));
        }
        return Ok((pb, face_index));
    }
    if text.chars().any(|c| c as u32 > 127) {
        if let Some((pb, idx)) = default_chinese_font() {
            return Ok((pb, idx));
        }
        return Err("中文内容需要选择支持中文的系统字体".into());
    }
    Err("请选择字体".into())
}

pub fn prepare_font(
    doc: &mut Document,
    font_path: Option<&str>,
    face_index: u32,
    text: &str,
) -> Result<FontMode, String> {
    let needs_cjk = text.chars().any(|c| c as u32 > 127);
    if !needs_cjk && font_path.is_none() {
        return Ok(FontMode::Type1Fallback);
    }
    let (path, idx) = resolve_font_path(font_path, face_index, text)?;
    Ok(FontMode::Embedded(embed_truetype_font(
        doc, &path, idx, text,
    )?))
}

#[tauri::command(async)]
pub fn validate_font_file(path: String, face_index: u32) -> Result<SystemFontEntry, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("字体文件不存在".into());
    }
    let data = std::fs::read(p).map_err(|e| e.to_string())?;
    let face = Face::parse(&data, face_index).map_err(|e| format!("无效字体: {:?}", e))?;
    Ok(SystemFontEntry {
        label: font_label(&face),
        path,
        face_index,
    })
}
