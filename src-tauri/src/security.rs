use crate::pdf_util::{load_document, load_document_with_password, save_document};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Document, Object, ObjectId};
use rand::RngCore;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(serde::Serialize)]
pub struct SecurityResult {
    pub message: String,
    pub output_path: String,
}

#[derive(serde::Serialize)]
pub struct PdfMetadataInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub encrypted: bool,
}

fn build_permissions(allow_print: bool, allow_copy: bool) -> Permissions {
    let mut perms = Permissions::all();
    if !allow_print {
        perms.remove(Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY);
    }
    if !allow_copy {
        perms.remove(Permissions::COPYABLE);
    }
    perms
}

/// 使用 AES-256（PDF 2.0 / Revision 6，当前 PDF 标准最强加密）。
/// 刻意不兼容 Acrobat 9 及以下等老旧阅读器，以强度优先。
fn encrypt_document(
    doc: &mut Document,
    user_password: &str,
    owner_password: &str,
    allow_print: bool,
    allow_copy: bool,
) -> Result<(), String> {
    if doc.is_encrypted() {
        return Err("PDF 已加密，请先解密".into());
    }
    if user_password.is_empty() {
        return Err("请设置打开密码".into());
    }
    if owner_password.is_empty() {
        return Err("请设置权限密码（所有者密码）".into());
    }
    if owner_password == user_password {
        return Err("权限密码必须与打开密码不同，否则打印/复制限制会失效".into());
    }

    // lopdf 生成加密密钥时必须读取 trailer 的 /ID 数组首元素；部分手工生成或
    // 精简的 PDF 缺失该项会导致 "missing the file /ID elements" 加密失败。
    // 这里在缺失或格式非法时补一个 16 字节文件标识符，保证加密可进行。
    let id_valid = doc
        .trailer
        .get(b"ID")
        .ok()
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.first().cloned())
        .map(|v| matches!(v.as_str(), Ok(s) if !s.is_empty()))
        .unwrap_or(false);
    if !id_valid {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seed = format!("{}-{}", nanos, std::process::id());
        let mut id = seed.into_bytes();
        id.resize(16, 0xA5);
        id.truncate(16);
        doc.trailer.set(
            b"ID",
            Object::Array(vec![
                Object::String(id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(id, lopdf::StringFormat::Hexadecimal),
            ]),
        );
    }

    // 32 字节密码学安全随机文件密钥（AES-256 要求）。
    let mut file_encryption_key = [0u8; 32];
    rand::rng().fill_bytes(&mut file_encryption_key[..]);

    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    crypt_filters.insert(b"StdCF".to_vec(), Arc::new(Aes256CryptFilter));

    // V5 = PDF 2.0 / Revision 6；encrypt_metadata=true 让文档元数据也纳入加密。
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters,
        file_encryption_key: &file_encryption_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password,
        permissions: build_permissions(allow_print, allow_copy),
    };

    let state = EncryptionState::try_from(version)
        .map_err(|e| format!("初始化 AES-256 加密失败：{}", e))?;
    doc.encrypt(&state).map_err(|e| e.to_string())?;
    // AES-256 属于 PDF 2.0，同步更新文件头版本以符合规范。
    doc.version = "2.0".to_string();
    Ok(())
}

pub(crate) fn decrypt_document(doc: &mut Document, password: &str) -> Result<(), String> {
    if !doc.is_encrypted() {
        return Err("PDF 未加密".into());
    }
    doc.decrypt(password).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("password") || msg.contains("Password") || msg.contains("PasswordValidation") {
            "密码错误或无法解密".into()
        } else {
            msg
        }
    })
}

/// 清除文档信息字典、目录 XMP（/Metadata）以及页级 /PieceInfo。
fn clear_metadata(doc: &mut Document) -> Result<bool, String> {
    let mut cleared = false;

    // 1) Trailer /Info 文档信息字典
    let info_id = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| o.as_reference().ok());
    if let Some(iid) = info_id {
        if let Ok(dict) = doc.get_object_mut(iid).and_then(Object::as_dict_mut) {
            for key in [
                "Title", "Author", "Subject", "Keywords", "Creator", "Producer",
                "CreationDate", "ModDate", "Trapped",
            ] {
                dict.remove(key.as_bytes());
            }
        }
        doc.trailer.remove(b"Info");
        doc.objects.remove(&iid);
        cleared = true;
    }

    // 2) Catalog 的 /Metadata（XMP 元数据流）
    let root_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok());
    if let Some(rid) = root_id {
        // 先在借用作用域内摘除目录项并记录要删除的流对象号，
        // 再在借用结束后删除对象，避免对 doc.objects 的双重可变借用。
        let metadata_ref = if let Ok(cat) = doc.get_object_mut(rid).and_then(Object::as_dict_mut) {
            let meta_ref = match cat.remove(b"Metadata") {
                Some(Object::Reference(mid)) => Some(mid),
                Some(_) => None,
                None => None,
            };
            if cat.remove(b"PieceInfo").is_some() {
                cleared = true;
            }
            if meta_ref.is_some() {
                cleared = true;
            }
            meta_ref
        } else {
            None
        };
        if let Some(mid) = metadata_ref {
            doc.objects.remove(&mid);
        }
    }

    // 3) 兜底：移除所有残留的 XMP 元数据流（Type = Metadata）
    let xmp_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let stream = obj.as_stream().ok()?;
            let type_name = stream.dict.get(b"Type").ok()?.as_name().ok()?;
            if type_name == b"Metadata" {
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    for id in xmp_ids {
        doc.objects.remove(&id);
        cleared = true;
    }

    Ok(cleared)
}

pub(crate) fn load_for_security(path: &Path, password: Option<&str>) -> Result<Document, String> {
    match password {
        Some(p) if !p.is_empty() => load_document_with_password(path, p),
        _ => load_document(path),
    }
}

/// 按 PDF 规范解码文档信息字符串：UTF-16BE(BOM) / UTF-8 / PDFDocEncoding(latin-1 回退)。
fn decode_meta_string(obj: Option<&Object>) -> Option<String> {
    let obj = obj?;
    let bytes = match obj {
        Object::String(bytes, _) => bytes.as_slice(),
        Object::Name(name) => name.as_slice(),
        _ => return None,
    };

    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }

    Some(match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    })
}

fn read_metadata(doc: &Document) -> PdfMetadataInfo {
    let encrypted = doc.is_encrypted();
    let mut info = PdfMetadataInfo {
        title: None,
        author: None,
        subject: None,
        keywords: None,
        creator: None,
        producer: None,
        encrypted,
    };

    let info_id = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => *id,
        _ => return info,
    };

    let Ok(dict) = doc.get_dictionary(info_id) else {
        return info;
    };

    info.title = decode_meta_string(dict.get(b"Title").ok());
    info.author = decode_meta_string(dict.get(b"Author").ok());
    info.subject = decode_meta_string(dict.get(b"Subject").ok());
    info.keywords = decode_meta_string(dict.get(b"Keywords").ok());
    info.creator = decode_meta_string(dict.get(b"Creator").ok());
    info.producer = decode_meta_string(dict.get(b"Producer").ok());
    info
}

#[tauri::command(async)]
pub fn encrypt_pdf(
    source_path: String,
    output_path: String,
    user_password: String,
    owner_password: Option<String>,
    allow_print: bool,
    allow_copy: bool,
) -> Result<SecurityResult, String> {
    let mut doc = load_document(Path::new(&source_path))?;
    let owner = owner_password
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "权限密码为必填项，且需与打开密码不同".to_string())?;
    encrypt_document(&mut doc, &user_password, &owner, allow_print, allow_copy)?;
    save_document(&mut doc, Path::new(&output_path))?;
    Ok(SecurityResult {
        message: "已加密 PDF（AES-256）".into(),
        output_path,
    })
}

#[tauri::command(async)]
pub fn decrypt_pdf(
    source_path: String,
    output_path: String,
    password: String,
) -> Result<SecurityResult, String> {
    if password.is_empty() {
        return Err("请输入密码".into());
    }
    // load_with_password 在密码正确时已完成解密并移除 /Encrypt 字典。
    // 注意：不要再调用 decrypt_document —— 此时 is_encrypted() 已为 false，
    // 会因守卫「PDF 未加密」而报错（这正是原实现的 Bug）。
    let mut doc = load_document_with_password(Path::new(&source_path), &password)
        .map_err(|_| "密码错误或无法解密".to_string())?;
    if !doc.was_encrypted() {
        return Err("该 PDF 未加密，无需解密".into());
    }
    // 清除内存中的加密状态，确保输出为纯明文。
    doc.encryption_state = None;
    save_document(&mut doc, Path::new(&output_path))?;
    Ok(SecurityResult {
        message: "已移除密码保护".into(),
        output_path,
    })
}

#[tauri::command(async)]
pub fn clear_pdf_metadata(
    source_path: String,
    output_path: String,
    password: Option<String>,
) -> Result<SecurityResult, String> {
    let mut doc = load_for_security(Path::new(&source_path), password.as_deref())?;
    if doc.is_encrypted() {
        let pwd = password
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "加密 PDF 需提供密码".to_string())?;
        decrypt_document(&mut doc, &pwd)?;
        doc.encryption_state = None;
    }
    let cleared = clear_metadata(&mut doc)?;
    save_document(&mut doc, Path::new(&output_path))?;
    Ok(SecurityResult {
        message: if cleared {
            "已清除文档属性与 XMP 元数据"
                .into()
        } else {
            "文档无元数据，已另存".into()
        },
        output_path,
    })
}

#[tauri::command(async)]
pub fn get_pdf_metadata(
    source_path: String,
    password: Option<String>,
) -> Result<PdfMetadataInfo, String> {
    let doc = load_for_security(Path::new(&source_path), password.as_deref())?;
    Ok(read_metadata(&doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    #[test]
    fn decode_utf16be_metadata() {
        let obj = Object::String(vec![0xFE, 0xFF, 0x4E, 0x2D, 0x65, 0x87], lopdf::StringFormat::Literal);
        assert_eq!(decode_meta_string(Some(&obj)).as_deref(), Some("中文"));
    }

    #[test]
    fn decode_utf8_and_latin1_metadata() {
        let utf8 = Object::String("标题".as_bytes().to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(decode_meta_string(Some(&utf8)).as_deref(), Some("标题"));
        let latin1 = Object::String(vec![0xE9, 0x74], lopdf::StringFormat::Literal);
        assert_eq!(decode_meta_string(Some(&latin1)).as_deref(), Some("ét"));
    }

    /// 构造一个含单页的最小文档（补齐 /ID 以满足部分 PDF 阅读器要求）。
    fn sample_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => Object::Reference(content_id),
        });
        doc.objects.insert(pages, dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page)],
            "Count" => 1,
        }.into());
        let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", cat);
        doc.trailer.set("ID", Object::Array(vec![
            Object::string_literal(b"test-file-id-0001"),
            Object::string_literal(b"test-file-id-0001"),
        ]));
        doc
    }

    #[test]
    fn encrypt_uses_aes256_pdf2() {
        // 校验当前采用最高强度：AES-256、Revision 6、PDF 2.0、元数据一并加密。
        let mut doc = sample_doc();
        encrypt_document(&mut doc, "user123", "owner456", true, true).unwrap();
        assert_eq!(doc.version, "2.0", "文件头应标记为 PDF 2.0");

        // encrypt() 仅把参数编码进 trailer 的 /Encrypt 字典，encryption_state
        // 需重新加载后才能解析；这里按真实流程保存再读取来断言。
        let dir = std::env::temp_dir().join(format!("pdf_toolbox_test_{}_aes256", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let enc_path = dir.join("enc.pdf");
        doc.save_to(&mut std::fs::File::create(&enc_path).unwrap()).unwrap();

        let loaded = Document::load_with_password(&enc_path, "user123").unwrap();
        let state = loaded
            .encryption_state
            .as_ref()
            .expect("加载加密文档应保留加密状态");
        assert_eq!(state.version(), 5, "security handler 版本应为 V5");
        assert_eq!(state.revision(), 6, "修订版本应为 6（AES-256）");
        // V5/AES-256 密钥固定 256 位，规范规定 /Encrypt 字典不写 /Length，
        // 因此重载后 key_length() 为 None 属正常；强度由 version=5/revision=6 唯一确定。
        assert!(state.encrypt_metadata(), "元数据应一并加密");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn owner_password_required_and_distinct() {
        // 权限密码缺失或与打开密码相同都应被拒绝，否则禁止打印/复制的限制形同虚设。
        let mut empty = sample_doc();
        assert!(encrypt_document(&mut empty, "user123", "", true, false).is_err());
        let mut same = sample_doc();
        assert!(encrypt_document(&mut same, "user123", "user123", true, false).is_err());
    }

    #[test]
    fn encrypt_then_decrypt_roundtrip() {
        // 与生产 decrypt_pdf 路径一致：用正确密码加载即得到解密文档，保存后为明文。
        let mut doc = sample_doc();
        encrypt_document(&mut doc, "user123", "owner456", true, true).unwrap();
        assert!(doc.is_encrypted());

        let dir = std::env::temp_dir().join(format!("pdf_toolbox_test_{}_{}", std::process::id(), 1));
        std::fs::create_dir_all(&dir).unwrap();
        let enc_path = dir.join("enc.pdf");
        let plain_path = dir.join("plain.pdf");
        doc.save_to(&mut std::fs::File::create(&enc_path).unwrap()).unwrap();

        // 关键断言：正确密码加载后对象可用（is_encrypted=false），无 "PDF 未加密" 误报
        let mut loaded = Document::load_with_password(&enc_path, "user123").unwrap();
        assert!(!loaded.is_encrypted(), "正确密码加载后应视为已解密");
        assert!(loaded.was_encrypted(), "应保留曾经加密的标记");
        assert_eq!(loaded.get_pages().len(), 1, "解密后页树应完整");

        // 清除状态后另存，重新以普通方式加载必须是明文、无需密码
        loaded.encryption_state = None;
        loaded.save_to(&mut std::fs::File::create(&plain_path).unwrap()).unwrap();
        let reopened = Document::load(&plain_path).unwrap();
        assert!(!reopened.is_encrypted(), "另存文件应为明文");
        assert_eq!(reopened.get_pages().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_password_fails() {
        let mut doc = sample_doc();
        encrypt_document(&mut doc, "right", "owner", true, true).unwrap();
        assert!(decrypt_document(&mut doc, "wrong").is_err());
    }
}
