use std::path::Path;
use std::process::Command;

#[tauri::command(async)]
pub fn reveal_path_in_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("路径不存在: {path}"));
    }

    #[cfg(target_os = "windows")]
    {
        if p.is_dir() {
            Command::new("explorer")
                .arg(p)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            Command::new("explorer")
                .args(["/select,", &path])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if p.is_dir() {
            Command::new("open")
                .arg(p)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            Command::new("open")
                .args(["-R", &path])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let dir = if p.is_dir() {
            p.to_path_buf()
        } else {
            p.parent()
                .ok_or_else(|| "无法解析父目录".to_string())?
                .to_path_buf()
        };
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("当前平台不支持打开文件位置".into())
}
