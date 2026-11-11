use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
pub struct TaskProgress {
    pub percent: u8,
    pub message: String,
}

pub fn emit_progress(app: &AppHandle, percent: u8, message: impl Into<String>) {
    let _ = app.emit(
        "task-progress",
        TaskProgress {
            percent: percent.min(100),
            message: message.into(),
        },
    );
}
