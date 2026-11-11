use crate::convert::ConvertResult;
use tauri::AppHandle;
#[cfg(windows)]
use crate::progress::emit_progress;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::process::{Command, Output};
#[cfg(windows)]
use std::sync::Mutex;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[derive(Clone, serde::Serialize)]
pub struct OfficeStatus {
    /// 当前操作系统是否支持（仅 Windows）
    pub platform_supported: bool,
    pub word: bool,
    pub excel: bool,
    pub powerpoint: bool,
}

#[cfg(windows)]
#[derive(Copy, Clone)]
enum OfficeKind {
    Word,
    Excel,
    PowerPoint,
}

#[cfg(windows)]
impl OfficeKind {
    fn app_name(self) -> &'static str {
        match self {
            Self::Word => "Microsoft Word",
            Self::Excel => "Microsoft Excel",
            Self::PowerPoint => "Microsoft PowerPoint",
        }
    }

    fn prog_id(self) -> &'static str {
        match self {
            Self::Word => "Word.Application",
            Self::Excel => "Excel.Application",
            Self::PowerPoint => "PowerPoint.Application",
        }
    }

    // 三个 BODY 常量仅在 Windows 下定义（依赖 PowerShell COM），
    // 故本方法同样只在 Windows 编译，避免 macOS/Linux 交叉编译时报 E0425。
    #[cfg(windows)]
    fn body(self) -> &'static str {
        match self {
            Self::Word => WORD_BODY,
            Self::Excel => EXCEL_BODY,
            Self::PowerPoint => PPT_BODY,
        }
    }
}

#[cfg(windows)]
struct OfficeCache {
    status: OfficeStatus,
    at: Instant,
}

#[cfg(windows)]
static OFFICE_CACHE: Mutex<Option<OfficeCache>> = Mutex::new(None);
#[cfg(windows)]
const CACHE_TTL: Duration = Duration::from_secs(300);

#[cfg(windows)]
fn kind_from_path(path: &Path) -> Option<OfficeKind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "doc" | "docx" | "docm" | "rtf" => Some(OfficeKind::Word),
        "xls" | "xlsx" | "xlsm" | "csv" => Some(OfficeKind::Excel),
        "ppt" | "pptx" | "pptm" => Some(OfficeKind::PowerPoint),
        _ => None,
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let trimmed = path.trim();
    if let Some(stripped) = trimmed.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(windows)]
fn absolutize_output(path: &Path) -> Result<String, String> {
    if path.is_absolute() {
        return Ok(normalize_windows_path(&path.to_string_lossy()));
    }
    // GUI 进程的工作目录不可预测，拒绝相对路径以避免写到意外位置。
    Err(format!("输出路径须为绝对路径：{}", path.display()))
}

#[cfg(windows)]
fn absolutize_source(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("无法读取路径：{}", e))?;
    Ok(normalize_windows_path(&canonical.to_string_lossy()))
}

#[cfg(windows)]
fn ps_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let msg = if !stderr.is_empty() { stderr } else { stdout };
    if msg.is_empty() {
        return format!("PowerShell 退出码 {}", output.status.code().unwrap_or(-1));
    }
    msg.lines()
        .find(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with('+')
                && !t.starts_with('~')
                && !t.starts_with("At ")
                && !t.contains("CategoryInfo")
                && !t.contains("FullyQualifiedErrorId")
        })
        .unwrap_or(msg.as_str())
        .trim()
        .to_string()
}

#[cfg(windows)]
fn run_powershell_inline(script: &str) -> Result<Output, String> {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| format!("无法调用 PowerShell：{}", e))
}

#[cfg(windows)]
fn run_office_script(body: &str, source: &str, output: &str) -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let temp_dir = std::env::temp_dir();
    // 不可预测的随机文件名，避免并发覆盖与本地竞争
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let script_path = temp_dir.join(format!(
        "pdf_tool_office_{}_{}_{:x}.ps1",
        std::process::id(),
        nonce,
        output.len()
    ));

    let full_script = format!(
        r#"$ErrorActionPreference = 'Stop'
$source = [System.IO.Path]::GetFullPath($env:PDF_TOOL_SOURCE)
$output = [System.IO.Path]::GetFullPath($env:PDF_TOOL_OUTPUT)
if (-not $env:PDF_TOOL_SOURCE -or -not $env:PDF_TOOL_OUTPUT) {{ throw '缺少文件路径' }}
if (-not [System.IO.File]::Exists($source)) {{ throw "源文件不存在：$source" }}
{body}"#
    );

    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(full_script.as_bytes());
    std::fs::write(&script_path, bytes).map_err(|e| format!("无法写入临时脚本：{}", e))?;

    let output_proc = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script_path)
        .env("PDF_TOOL_SOURCE", source)
        .env("PDF_TOOL_OUTPUT", output)
        .output()
        .map_err(|e| format!("无法调用 PowerShell：{}", e))?;

    let _ = std::fs::remove_file(&script_path);

    if output_proc.status.success() {
        Ok(())
    } else {
        Err(ps_error(&output_proc))
    }
}

#[cfg(windows)]
const WORD_BODY: &str = r#"
$word = $null
$doc = $null
try {
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $doc = $word.Documents.Open($source, $false, $false, $false)
    if ($null -eq $doc) { throw "Word 无法打开文档：$source" }
    $pdf = 17
    try {
        $doc.ExportAsFixedFormat($output, $pdf)
    } catch {
        $doc.SaveAs2($output, $pdf)
    }
    $doc.Close([ref]0) | Out-Null
    $word.Quit([ref]0) | Out-Null
} catch {
    if ($doc) { try { $doc.Close([ref]0) } catch {} }
    if ($word) { try { $word.Quit([ref]0) } catch {} }
    throw
} finally {
    if ($doc) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($doc) }
    if ($word) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word) }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
"#;

#[cfg(windows)]
const EXCEL_BODY: &str = r#"
$excel = $null
$wb = $null
try {
    $excel = New-Object -ComObject Excel.Application
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $wb = $excel.Workbooks.Open($source)
    if ($null -eq $wb) { throw "Excel 无法打开文档：$source" }
    $wb.ExportAsFixedFormat(0, $output)
    $wb.Close($false) | Out-Null
    $excel.Quit() | Out-Null
} catch {
    if ($wb) { try { $wb.Close($false) } catch {} }
    if ($excel) { try { $excel.Quit() } catch {} }
    throw
} finally {
    if ($wb) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($wb) }
    if ($excel) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel) }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
"#;

#[cfg(windows)]
const PPT_BODY: &str = r#"
$ppt = $null
$pres = $null
try {
    $ppt = New-Object -ComObject PowerPoint.Application
    $ppt.Visible = 0
    $pres = $ppt.Presentations.Open($source, $true, $true, $false)
    if ($null -eq $pres) { throw "PowerPoint 无法打开文档：$source" }
    $pres.SaveAs($output, 32)
    $pres.Close() | Out-Null
    $ppt.Quit() | Out-Null
} catch {
    if ($pres) { try { $pres.Close() } catch {} }
    if ($ppt) { try { $ppt.Quit() } catch {} }
    throw
} finally {
    if ($pres) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($pres) }
    if ($ppt) { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($ppt) }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}
"#;

#[cfg(windows)]
fn com_available(prog_id: &str) -> bool {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
try {{
    $app = New-Object -ComObject '{prog_id}' -ErrorAction Stop
    $app.Quit() | Out-Null
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($app)
    'true'
}} catch {{
    'false'
}}
"#
    );
    run_powershell_inline(&script)
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
}

#[cfg(windows)]
fn read_office_cache() -> Option<OfficeStatus> {
    let guard = OFFICE_CACHE.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.status.clone())
}

#[cfg(windows)]
fn write_office_cache(status: &OfficeStatus) {
    if let Ok(mut guard) = OFFICE_CACHE.lock() {
        *guard = Some(OfficeCache {
            status: status.clone(),
            at: Instant::now(),
        });
    }
}

#[cfg(windows)]
fn detect_office_status(app: &AppHandle) -> OfficeStatus {
    emit_progress(app, 8, "正在检测 Word…");
    let word = com_available(OfficeKind::Word.prog_id());
    emit_progress(app, 38, "正在检测 Excel…");
    let excel = com_available(OfficeKind::Excel.prog_id());
    emit_progress(app, 68, "正在检测 PowerPoint…");
    let powerpoint = com_available(OfficeKind::PowerPoint.prog_id());
    emit_progress(app, 100, "Office 检测完成");

    let status = OfficeStatus {
        platform_supported: true,
        word,
        excel,
        powerpoint,
    };
    write_office_cache(&status);
    status
}

#[cfg(windows)]
fn app_ready(kind: OfficeKind, status: &OfficeStatus) -> bool {
    match kind {
        OfficeKind::Word => status.word,
        OfficeKind::Excel => status.excel,
        OfficeKind::PowerPoint => status.powerpoint,
    }
}

#[cfg(windows)]
fn convert_with_office(kind: OfficeKind, source: &str, output: &str) -> Result<(), String> {
    run_office_script(kind.body(), source, output)
}

#[cfg(windows)]
fn office_to_pdf_inner(
    app: &AppHandle,
    source_path: &str,
    output_path: &str,
) -> Result<ConvertResult, String> {
    emit_progress(app, 5, "准备转换…");

    let source = Path::new(source_path);
    if !source.exists() {
        return Err(format!("文件不存在：{}", source_path));
    }

    let kind = kind_from_path(source).ok_or_else(|| {
        "不支持的文件类型，请选择 Word（.doc/.docx）、Excel（.xls/.xlsx）或 PowerPoint（.ppt/.pptx）".to_string()
    })?;

    emit_progress(app, 12, "检查 Office 组件…");
    let status = read_office_cache().unwrap_or_else(|| {
        emit_progress(app, 15, format!("正在检测 {}…", kind.app_name()));
        let available = com_available(kind.prog_id());
        OfficeStatus {
            platform_supported: true,
            word: matches!(kind, OfficeKind::Word) && available,
            excel: matches!(kind, OfficeKind::Excel) && available,
            powerpoint: matches!(kind, OfficeKind::PowerPoint) && available,
        }
    });

    if !app_ready(kind, &status) && !com_available(kind.prog_id()) {
        return Err(format!(
            "未检测到 {}。此功能需要本机已安装 Microsoft Office，未安装 Office 无法使用。",
            kind.app_name()
        ));
    }

    let source_str = absolutize_source(source)?;
    let output_str = absolutize_output(Path::new(output_path))?;

    if let Some(parent) = Path::new(&output_str).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    emit_progress(app, 25, format!("启动 {}…", kind.app_name()));
    emit_progress(app, 45, "打开文档…");

    convert_with_office(kind, &source_str, &output_str)?;

    emit_progress(app, 90, "正在生成 PDF…");

    if !Path::new(&output_str).exists() {
        return Err("转换未完成，未生成 PDF 文件".into());
    }

    emit_progress(app, 100, "转换完成");

    Ok(ConvertResult {
        output_path: output_str,
        detail: format!("已通过 {} 转换为 PDF", kind.app_name()),
    })
}

#[tauri::command]
pub async fn get_office_status(app: AppHandle) -> Result<OfficeStatus, String> {
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(OfficeStatus {
            platform_supported: false,
            word: false,
            excel: false,
            powerpoint: false,
        })
    }

    #[cfg(windows)]
    {
        if let Some(cached) = read_office_cache() {
            return Ok(cached);
        }
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || detect_office_status(&app))
            .await
            .map_err(|e| format!("Office 检测任务失败：{}", e))
    }
}

#[tauri::command]
pub async fn office_to_pdf(
    app: AppHandle,
    source_path: String,
    output_path: String,
) -> Result<ConvertResult, String> {
    #[cfg(not(windows))]
    {
        let _ = (app, source_path, output_path);
        return Err("Office 转 PDF 仅支持 Windows 系统".into());
    }

    #[cfg(windows)]
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            office_to_pdf_inner(&app, &source_path, &output_path)
        })
        .await
        .map_err(|e| format!("Office 转换任务失败：{}", e))?
    }
}
