use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    errors::AppResult,
    models::{
        AppStatus, AudioRecord, AudioRecordDetail, AudioRecordQuery, AudioStatus, DetectedDevice, FailureItem,
        LogEntry, NewAudioRecord, PendingAudioJob, SummaryDocument, TranscriptSegment,
    },
};

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new(path: PathBuf) -> AppResult<Self> {
        let db = Self { path };
        db.init()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> AppResult<Connection> {
        Ok(Connection::open(&self.path)?)
    }

    fn init(&self) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS devices (
              identifier TEXT PRIMARY KEY,
              volume_label TEXT,
              drive_letter TEXT NOT NULL,
              pnp_device_id TEXT,
              last_seen_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_state (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audio_files (
              id TEXT PRIMARY KEY,
              device_identifier TEXT NOT NULL,
              file_name TEXT NOT NULL,
              relative_path TEXT NOT NULL,
              imported_path TEXT NOT NULL,
              fingerprint TEXT NOT NULL UNIQUE,
              transcript_path TEXT,
              summary_path TEXT,
              status TEXT NOT NULL,
              error_message TEXT,
              synced_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS transcript_segments (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              audio_id TEXT NOT NULL,
              position INTEGER NOT NULL,
              speaker_label TEXT NOT NULL,
              start_ms INTEGER NOT NULL,
              end_ms INTEGER NOT NULL,
              text TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS summaries (
              audio_id TEXT PRIMARY KEY,
              title TEXT,
              bullets_json TEXT NOT NULL,
              full_text TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS processing_jobs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              audio_id TEXT NOT NULL,
              stage TEXT NOT NULL,
              status TEXT NOT NULL,
              attempt INTEGER NOT NULL,
              error_message TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_logs (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              level TEXT NOT NULL,
              scope TEXT NOT NULL,
              message TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_device(&self, device: &DetectedDevice) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO devices(identifier, volume_label, drive_letter, pnp_device_id, last_seen_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(identifier) DO UPDATE SET
              volume_label = excluded.volume_label,
              drive_letter = excluded.drive_letter,
              pnp_device_id = excluded.pnp_device_id,
              last_seen_at = excluded.last_seen_at
            "#,
            params![
                device.identifier,
                device.volume_label,
                device.drive_letter,
                device.pnp_device_id,
                device.last_seen_at.to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn set_last_scan_at(&self, time: DateTime<Utc>) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO app_state(key, value)
            VALUES ('last_scan_at', ?1)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![time.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_last_scan_at(&self) -> AppResult<Option<DateTime<Utc>>> {
        let conn = self.connect()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = 'last_scan_at'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|text| chrono::DateTime::parse_from_rfc3339(&text).ok().map(|dt| dt.with_timezone(&Utc))))
    }

    pub fn has_fingerprint(&self, fingerprint: &str) -> AppResult<bool> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM audio_files WHERE fingerprint = ?1",
            params![fingerprint],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_audio_record(&self, record: &NewAudioRecord) -> AppResult<()> {
        let conn = self.connect()?;
        let now = record.synced_at.to_rfc3339();
        conn.execute(
            r#"
            INSERT INTO audio_files(
              id, device_identifier, file_name, relative_path, imported_path, fingerprint,
              transcript_path, summary_path, status, error_message, synced_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, 'imported', NULL, ?7, ?8)
            "#,
            params![
                record.id,
                record.device_identifier,
                record.file_name,
                record.relative_path,
                record.imported_path,
                record.fingerprint,
                record.synced_at.to_rfc3339(),
                now
            ],
        )?;
        Ok(())
    }

    pub fn set_audio_status(&self, audio_id: &str, status: AudioStatus, error_message: Option<&str>) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE audio_files
            SET status = ?2, error_message = ?3, updated_at = ?4
            WHERE id = ?1
            "#,
            params![audio_id, status.as_str(), error_message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn attach_transcript_path(&self, audio_id: &str, transcript_path: &str) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE audio_files SET transcript_path = ?2, updated_at = ?3 WHERE id = ?1",
            params![audio_id, transcript_path, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn attach_summary_path(&self, audio_id: &str, summary_path: &str) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE audio_files SET summary_path = ?2, updated_at = ?3 WHERE id = ?1",
            params![audio_id, summary_path, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn record_job_event(
        &self,
        audio_id: &str,
        stage: &str,
        status: &str,
        attempt: i64,
        error_message: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO processing_jobs(audio_id, stage, status, attempt, error_message, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![audio_id, stage, status, attempt, error_message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn save_transcript(&self, audio_id: &str, segments: &[TranscriptSegment]) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM transcript_segments WHERE audio_id = ?1", params![audio_id])?;
        for (position, segment) in segments.iter().enumerate() {
            conn.execute(
                r#"
                INSERT INTO transcript_segments(audio_id, position, speaker_label, start_ms, end_ms, text)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    audio_id,
                    position as i64,
                    segment.speaker_label,
                    segment.start_ms,
                    segment.end_ms,
                    segment.text
                ],
            )?;
        }
        Ok(())
    }

    pub fn save_summary(&self, audio_id: &str, summary: &SummaryDocument) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO summaries(audio_id, title, bullets_json, full_text)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(audio_id) DO UPDATE SET
              title = excluded.title,
              bullets_json = excluded.bullets_json,
              full_text = excluded.full_text
            "#,
            params![
                audio_id,
                summary.title,
                serde_json::to_string(&summary.bullets)?,
                summary.full_text
            ],
        )?;
        Ok(())
    }

    pub fn list_audio_records(&self, query: &AudioRecordQuery) -> AppResult<Vec<AudioRecord>> {
        let conn = self.connect()?;
        let mut sql = String::from(
            "SELECT id, device_identifier, file_name, relative_path, imported_path, transcript_path, summary_path, status, error_message, synced_at, updated_at FROM audio_files WHERE 1=1",
        );
        let search = query.search.clone().unwrap_or_default();
        let status = query.status.clone().unwrap_or_default();
        if !search.trim().is_empty() {
            sql.push_str(" AND (file_name LIKE ?1 OR relative_path LIKE ?1)");
        }
        if !status.trim().is_empty() && status != "all" {
            if search.trim().is_empty() {
                sql.push_str(" AND status = ?1");
            } else {
                sql.push_str(" AND status = ?2");
            }
        }
        sql.push_str(" ORDER BY synced_at DESC");

        let mut statement = conn.prepare(&sql)?;
        let rows = if !search.trim().is_empty() && !status.trim().is_empty() && status != "all" {
            statement.query_map(params![format!("%{search}%"), status], map_audio_row)?
        } else if !search.trim().is_empty() {
            statement.query_map(params![format!("%{search}%")], map_audio_row)?
        } else if !status.trim().is_empty() && status != "all" {
            statement.query_map(params![status], map_audio_row)?
        } else {
            statement.query_map([], map_audio_row)?
        };

        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get_audio_record_detail(&self, audio_id: &str) -> AppResult<Option<AudioRecordDetail>> {
        let conn = self.connect()?;
        let record = conn
            .query_row(
                "SELECT id, device_identifier, file_name, relative_path, imported_path, transcript_path, summary_path, status, error_message, synced_at, updated_at FROM audio_files WHERE id = ?1",
                params![audio_id],
                map_audio_row,
            )
            .optional()?;
        let Some(record) = record else {
            return Ok(None);
        };

        let mut segment_stmt = conn.prepare(
            "SELECT speaker_label, start_ms, end_ms, text FROM transcript_segments WHERE audio_id = ?1 ORDER BY position ASC",
        )?;
        let segments = segment_stmt
            .query_map(params![audio_id], |row| {
                Ok(TranscriptSegment {
                    speaker_label: row.get(0)?,
                    start_ms: row.get(1)?,
                    end_ms: row.get(2)?,
                    text: row.get(3)?,
                })
            })?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        let summary = conn
            .query_row(
                "SELECT title, bullets_json, full_text FROM summaries WHERE audio_id = ?1",
                params![audio_id],
                |row| {
                    Ok(SummaryDocument {
                        title: row.get(0)?,
                        bullets: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                        full_text: row.get(2)?,
                    })
                },
            )
            .optional()?;

        Ok(Some(AudioRecordDetail {
            record,
            transcript_segments: segments,
            summary,
        }))
    }

    pub fn get_pending_jobs(&self, limit: usize) -> AppResult<Vec<PendingAudioJob>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, imported_path, file_name, status
            FROM audio_files
            WHERE status IN ('imported', 'transcribed')
            ORDER BY synced_at ASC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(PendingAudioJob {
                id: row.get(0)?,
                imported_path: row.get(1)?,
                file_name: row.get(2)?,
                status: AudioStatus::from_db(row.get::<_, String>(3)?.as_str()),
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get_record_by_id(&self, audio_id: &str) -> AppResult<Option<AudioRecord>> {
        let conn = self.connect()?;
        Ok(conn
            .query_row(
                "SELECT id, device_identifier, file_name, relative_path, imported_path, transcript_path, summary_path, status, error_message, synced_at, updated_at FROM audio_files WHERE id = ?1",
                params![audio_id],
                map_audio_row,
            )
            .optional()?)
    }

    pub fn list_logs(&self, limit: usize) -> AppResult<Vec<LogEntry>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, level, scope, message, created_at FROM app_logs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(LogEntry {
                id: row.get(0)?,
                level: row.get(1)?,
                scope: row.get(2)?,
                message: row.get(3)?,
                created_at: parse_datetime(row.get::<_, String>(4)?),
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn log(&self, level: &str, scope: &str, message: &str) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO app_logs(level, scope, message, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![level, scope, message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn build_status(&self, connected_devices: Vec<DetectedDevice>) -> AppResult<AppStatus> {
        let conn = self.connect()?;
        let total_files: i64 = conn.query_row("SELECT COUNT(1) FROM audio_files", [], |row| row.get(0))?;
        let completed_files: i64 = conn.query_row(
            "SELECT COUNT(1) FROM audio_files WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )?;
        let in_flight_files: i64 = conn.query_row(
            "SELECT COUNT(1) FROM audio_files WHERE status IN ('transcribing', 'summarizing')",
            [],
            |row| row.get(0),
        )?;
        let failed_files: i64 = conn.query_row(
            "SELECT COUNT(1) FROM audio_files WHERE status IN ('failed_transcription', 'failed_summary')",
            [],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, file_name, status, error_message, updated_at FROM audio_files WHERE status IN ('failed_transcription', 'failed_summary') ORDER BY updated_at DESC LIMIT 5",
        )?;
        let recent_failures = stmt
            .query_map([], |row| {
                Ok(FailureItem {
                    audio_id: row.get(0)?,
                    file_name: row.get(1)?,
                    status: AudioStatus::from_db(row.get::<_, String>(2)?.as_str()),
                    error_message: row.get(3)?,
                    updated_at: parse_datetime(row.get::<_, String>(4)?),
                })
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(AppStatus {
            device_connected: !connected_devices.is_empty(),
            connected_devices: connected_devices
                .into_iter()
                .map(|device| crate::models::DeviceInfo {
                    identifier: device.identifier,
                    volume_label: device.volume_label,
                    drive_letter: device.drive_letter,
                    pnp_device_id: device.pnp_device_id,
                    last_seen_at: device.last_seen_at,
                })
                .collect(),
            last_scan_at: self.get_last_scan_at()?,
            total_files,
            completed_files,
            in_flight_files,
            failed_files,
            recent_failures,
        })
    }
}

fn map_audio_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AudioRecord> {
    Ok(AudioRecord {
        id: row.get(0)?,
        device_identifier: row.get(1)?,
        file_name: row.get(2)?,
        relative_path: row.get(3)?,
        imported_path: row.get(4)?,
        transcript_path: row.get(5)?,
        summary_path: row.get(6)?,
        status: AudioStatus::from_db(row.get::<_, String>(7)?.as_str()),
        error_message: row.get(8)?,
        synced_at: parse_datetime(row.get::<_, String>(9)?),
        updated_at: parse_datetime(row.get::<_, String>(10)?),
    })
}

fn parse_datetime(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|time| time.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
