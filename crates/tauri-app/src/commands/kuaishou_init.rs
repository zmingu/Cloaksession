use crate::AppState;
use multizen_core::*;
use std::result::Result;
use tauri::State;

#[tauri::command]
pub async fn kuaishou_subject_list(state: State<'_, AppState>, query: KuaishouSubjectQuery) -> Result<KuaishouSubjectPage, String> { state.driver.kuaishou_subject_list(query).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_subject_detail(state: State<'_, AppState>, platform_user_id: String) -> Result<Option<KuaishouSubjectDetail>, String> { state.driver.kuaishou_subject_detail(platform_user_id).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_subject_correct(state: State<'_, AppState>, input: CorrectSubjectInput) -> Result<KuaishouSubjectArchive, String> { state.driver.kuaishou_subject_correct(input).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_subject_confirm(state: State<'_, AppState>, input: ConfirmSubjectInput) -> Result<KuaishouSubjectArchive, String> { state.driver.kuaishou_subject_confirm(input).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_subject_reocr(state: State<'_, AppState>, platform_user_id: String, expected_revision: u64) -> Result<KuaishouSubjectArchive, String> { state.driver.kuaishou_subject_reocr(platform_user_id, expected_revision).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_subject_attachment(state: State<'_, AppState>, key: String) -> Result<String, String> { state.driver.kuaishou_subject_attachment(key).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_init_steps(state: State<'_, AppState>, platform_user_id: String) -> Result<Vec<KuaishouInitStepRecord>, String> { state.driver.kuaishou_init_steps(platform_user_id).await.map_err(|e| e.to_string()) }
#[tauri::command]
pub async fn kuaishou_init_retry(state: State<'_, AppState>, profile_id: String) -> Result<(), String> { state.driver.kuaishou_init_retry(profile_id).await.map_err(|e| e.to_string()) }
#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct OcrAvailability { available: bool, message: String }
#[tauri::command]
pub async fn kuaishou_ocr_availability() -> Result<OcrAvailability, String> {
    Ok(match local_ocr::check_availability(std::time::Instant::now() + std::time::Duration::from_secs(10)).await {
        Ok(()) => OcrAvailability { available: true, message: "系统中文OCR可用；仅本地识别".into() },
        Err(e) => OcrAvailability { available: false, message: e.to_string() },
    })
}
