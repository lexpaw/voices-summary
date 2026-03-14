use chrono::Utc;
use tokio::process::Command;

use crate::{
    errors::{AppError, AppResult},
    models::{AppSettings, DetectedDevice},
};

#[cfg(not(target_os = "windows"))]
pub async fn discover_target_devices(_settings: &AppSettings) -> AppResult<Vec<DetectedDevice>> {
    Err(AppError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
pub async fn discover_target_devices(settings: &AppSettings) -> AppResult<Vec<DetectedDevice>> {
    let script = r#"
$ErrorActionPreference='Stop'
$results = @()
Get-CimInstance Win32_DiskDrive | Where-Object { $_.InterfaceType -eq 'USB' } | ForEach-Object {
  $drive = $_
  $diskId = $_.PNPDeviceID
  Get-CimAssociatedInstance -InputObject $drive -ResultClassName Win32_DiskPartition | ForEach-Object {
    $partition = $_
    Get-CimAssociatedInstance -InputObject $partition -ResultClassName Win32_LogicalDisk | ForEach-Object {
      $results += [PSCustomObject]@{
        driveLetter = $_.DeviceID
        volumeLabel = $_.VolumeName
        pnpDeviceId = $diskId
      }
    }
  }
}
$results | ConvertTo-Json -Depth 4
"#;

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(script)
        .output()
        .await?;

    if !output.status.success() {
        return Err(AppError::System(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() || stdout == "null" {
        return Ok(Vec::new());
    }

    let raw_devices = parse_devices_json(&stdout)?;
    let matched = raw_devices
        .into_iter()
        .filter_map(|item| {
            let volume_label = item.volume_label.clone();
            let pnp_device_id = item.pnp_device_id.clone();
            let drive_letter = item.drive_letter.clone();
            let normalized_pnp = pnp_device_id
                .clone()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let has_vid = settings
                .device_match_rule
                .vid
                .as_ref()
                .map(|value| normalized_pnp.contains(&format!("VID_{}", value.to_ascii_uppercase())))
                .unwrap_or(true);
            let has_pid = settings
                .device_match_rule
                .pid
                .as_ref()
                .map(|value| normalized_pnp.contains(&format!("PID_{}", value.to_ascii_uppercase())))
                .unwrap_or(true);
            let label_match = settings
                .device_match_rule
                .volume_label
                .as_ref()
                .map(|label| volume_label.as_ref().map(|name| name.eq_ignore_ascii_case(label)).unwrap_or(false))
                .unwrap_or(true);
            let path_hint_match = if settings.device_match_rule.path_hints.is_empty() {
                true
            } else {
                settings.device_match_rule.path_hints.iter().any(|hint| {
                    std::path::Path::new(&format!("{drive_letter}\\{hint}")).exists()
                })
            };

            if has_vid && has_pid && label_match {
                Some(DetectedDevice {
                    identifier: format!(
                        "USB:{}:{}:{}",
                        settings.device_match_rule.vid.clone().unwrap_or_else(|| "unknown".to_string()),
                        settings.device_match_rule.pid.clone().unwrap_or_else(|| "unknown".to_string()),
                        volume_label.clone().unwrap_or_else(|| "unknown".to_string())
                    ),
                    drive_letter,
                    volume_label,
                    pnp_device_id,
                    path_hints_matched: path_hint_match,
                    last_seen_at: Utc::now(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(matched)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDeviceRecord {
    drive_letter: String,
    volume_label: Option<String>,
    pnp_device_id: Option<String>,
}

#[cfg(target_os = "windows")]
fn parse_devices_json(raw: &str) -> AppResult<Vec<RawDeviceRecord>> {
    if raw.trim_start().starts_with('[') {
        Ok(serde_json::from_str(raw)?)
    } else {
        Ok(vec![serde_json::from_str(raw)?])
    }
}
