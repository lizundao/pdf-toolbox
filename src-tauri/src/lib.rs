mod compress;
mod convert;
mod fonts;
mod info;
mod office;
mod pages;
mod pdf_util;
mod progress;
mod redact;
mod scan;
mod security;
mod shell_util;
mod watermark;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            compress::compress_pdf,
            compress::compress_pdf_batch,
            pages::get_pdf_info,
            pages::merge_pdfs,
            pages::split_pdf,
            pages::extract_pages,
            pages::delete_pages,
            pages::rotate_pdf_pages,
            pages::reorder_pdf_pages,
            pages::insert_pdf,
            pages::crop_pdf_pages,
            pages::resize_pdf_pages,
            pages::remove_blank_pdf_pages,
            scan::compress_pdf_images,
            scan::compress_pdf_images_batch,
            convert::images_to_pdf,
            convert::extract_pdf_text,
            convert::text_to_pdf,
            convert::text_file_to_pdf,
            convert::list_pdf_export_pages,
            office::get_office_status,
            office::office_to_pdf,
            watermark::add_text_watermark,
            watermark::add_image_watermark,
            watermark::add_page_headers,
            security::encrypt_pdf,
            security::decrypt_pdf,
            security::clear_pdf_metadata,
            security::get_pdf_metadata,
            redact::apply_pdf_redaction,
            info::set_pdf_metadata,
            info::read_pdf_file,
            info::get_pdf_preview_info,
            info::write_binary_file,
            info::check_path_exists,
            shell_util::reveal_path_in_folder,
            fonts::list_system_fonts,
            fonts::validate_font_file,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
