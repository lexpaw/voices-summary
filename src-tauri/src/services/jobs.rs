use std::{collections::HashSet, path::Path, sync::Arc};

use tokio::sync::MutexGuard;

use crate::{
    commands::AppState,
    errors::{AppError, AppResult},
    models::{AudioStatus, PendingAudioJob, SummaryDocument, TranscriptSegment},
    providers,
};

pub async fn start_worker_loop(state: Arc<AppState>) {
    loop {
        if let Err(error) = process_pending_jobs(state.clone()).await {
            let _ = state
                .db
                .log("ERROR", "jobs", &format!("任务轮询失败: {error}"));
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

pub async fn retry_job(state: Arc<AppState>, id: &str, stage: &str) -> AppResult<()> {
    match stage {
        "transcription" => state.db.set_audio_status(id, AudioStatus::Imported, None)?,
        "summary" => state.db.set_audio_status(id, AudioStatus::Transcribed, None)?,
        _ => return Err(AppError::Config("无效的重试阶段".to_string())),
    }
    state
        .db
        .log("INFO", "jobs", &format!("任务 {id} 已重新排队，阶段: {stage}"))?;
    Ok(())
}

async fn process_pending_jobs(state: Arc<AppState>) -> AppResult<()> {
    let settings = state.config.get().await;
    let pending = state.db.get_pending_jobs(settings.processing_concurrency)?;
    for job in pending {
        if !mark_active(&state, &job.id).await {
            continue;
        }
        let cloned_state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = process_single_job(cloned_state.clone(), job.clone()).await {
                let _ = cloned_state
                    .db
                    .log("ERROR", "jobs", &format!("任务 {} 处理失败: {error}", job.id));
            }
            unmark_active(&cloned_state, &job.id).await;
        });
    }
    Ok(())
}

async fn process_single_job(state: Arc<AppState>, job: PendingAudioJob) -> AppResult<()> {
    match job.status {
        AudioStatus::Imported | AudioStatus::FailedTranscription => {
            let segments = run_transcription(&state, &job).await?;
            run_summary(&state, &job, &segments).await?;
        }
        AudioStatus::Transcribed | AudioStatus::FailedSummary => {
            let detail = state
                .db
                .get_audio_record_detail(&job.id)?
                .ok_or_else(|| AppError::Path(format!("找不到音频记录 {}", job.id)))?;
            run_summary(&state, &job, &detail.transcript_segments).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn run_transcription(state: &Arc<AppState>, job: &PendingAudioJob) -> AppResult<Vec<TranscriptSegment>> {
    state.db.set_audio_status(&job.id, AudioStatus::Transcribing, None)?;
    state
        .db
        .record_job_event(&job.id, "transcription", "started", 1, None)?;
    let settings = state.config.get().await;
    let audio_path = Path::new(&job.imported_path);
    let result = providers::transcribe_audio(&settings, audio_path).await;
    match result {
        Ok(segments) => {
            state.db.save_transcript(&job.id, &segments)?;
            let transcript_path = write_transcript_file(&settings.output_dir, &job.id, &job.file_name, &segments).await?;
            state.db.attach_transcript_path(&job.id, &transcript_path)?;
            state.db.set_audio_status(&job.id, AudioStatus::Transcribed, None)?;
            state
                .db
                .record_job_event(&job.id, "transcription", "completed", 1, None)?;
            Ok(segments)
        }
        Err(error) => {
            let error_text = error.to_string();
            state.db.set_audio_status(
                &job.id,
                AudioStatus::FailedTranscription,
                Some(&error_text),
            )?;
            state.db.record_job_event(
                &job.id,
                "transcription",
                "failed",
                1,
                Some(&error_text),
            )?;
            Err(error)
        }
    }
}

async fn run_summary(
    state: &Arc<AppState>,
    job: &PendingAudioJob,
    segments: &[TranscriptSegment],
) -> AppResult<SummaryDocument> {
    state.db.set_audio_status(&job.id, AudioStatus::Summarizing, None)?;
    state
        .db
        .record_job_event(&job.id, "summary", "started", 1, None)?;
    let settings = state.config.get().await;
    let result = providers::summarize_transcript(&settings, &job.file_name, segments).await;
    match result {
        Ok(summary) => {
            state.db.save_summary(&job.id, &summary)?;
            let summary_path = write_summary_file(&settings.output_dir, &job.id, &job.file_name, &summary).await?;
            state.db.attach_summary_path(&job.id, &summary_path)?;
            state.db.set_audio_status(&job.id, AudioStatus::Completed, None)?;
            state.db.record_job_event(&job.id, "summary", "completed", 1, None)?;
            Ok(summary)
        }
        Err(error) => {
            let error_text = error.to_string();
            state
                .db
                .set_audio_status(&job.id, AudioStatus::FailedSummary, Some(&error_text))?;
            state.db.record_job_event(
                &job.id,
                "summary",
                "failed",
                1,
                Some(&error_text),
            )?;
            Err(error)
        }
    }
}

async fn write_transcript_file(
    output_dir: &str,
    audio_id: &str,
    file_name: &str,
    segments: &[TranscriptSegment],
) -> AppResult<String> {
    let path = Path::new(output_dir)
        .join("transcripts")
        .join(format!("{audio_id}.md"));
    let mut content = format!("# 转写稿\n\n- 文件名: {file_name}\n- 音频 ID: {audio_id}\n\n");
    for segment in segments {
        content.push_str(&format!(
            "## {} [{} - {}]\n{}\n\n",
            segment.speaker_label, segment.start_ms, segment.end_ms, segment.text
        ));
    }
    tokio::fs::write(&path, content).await?;
    Ok(path.to_string_lossy().to_string())
}

async fn write_summary_file(
    output_dir: &str,
    audio_id: &str,
    file_name: &str,
    summary: &SummaryDocument,
) -> AppResult<String> {
    let path = Path::new(output_dir)
        .join("summaries")
        .join(format!("{audio_id}.md"));
    let mut content = format!(
        "# {}\n\n- 文件名: {file_name}\n- 音频 ID: {audio_id}\n\n",
        summary.title.clone().unwrap_or_else(|| "摘要".to_string())
    );
    if !summary.bullets.is_empty() {
        content.push_str("## 要点\n");
        for bullet in &summary.bullets {
            content.push_str(&format!("- {bullet}\n"));
        }
        content.push('\n');
    }
    content.push_str("## 详细摘要\n");
    content.push_str(&summary.full_text);
    content.push('\n');
    tokio::fs::write(&path, content).await?;
    Ok(path.to_string_lossy().to_string())
}

async fn mark_active(state: &Arc<AppState>, audio_id: &str) -> bool {
    let mut guard = state.active_jobs.lock().await;
    if guard.contains(audio_id) {
        return false;
    }
    guard.insert(audio_id.to_string());
    true
}

async fn unmark_active(state: &Arc<AppState>, audio_id: &str) {
    let mut guard: MutexGuard<'_, HashSet<String>> = state.active_jobs.lock().await;
    guard.remove(audio_id);
}
