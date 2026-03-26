//! macOS record-window skill via `osascript` and `ffmpeg`.

use crate::types::{RecordWindowResult, RecordWindowSkill, RecordWindowSkillError};
use async_trait::async_trait;
use chrono::Local;
use metrics::{counter, histogram};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const RECORD_WINDOW_SKILL_EXECUTE_TOTAL: &str = "record_window_skill_execute_total";
const RECORD_WINDOW_SKILL_ERRORS_TOTAL: &str = "record_window_skill_errors_total";
const RECORD_WINDOW_SKILL_EXECUTE_DURATION_SECONDS: &str =
    "record_window_skill_execute_duration_seconds";

#[derive(Clone, Copy)]
struct WindowBounds {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

#[derive(Clone)]
pub struct MacOsRecordWindowSkill {
    dry_run: bool,
    aggregate_device_name: String,
    display_index: u8,
}

impl MacOsRecordWindowSkill {
    pub fn new() -> Self {
        Self {
            dry_run: false,
            aggregate_device_name: "Aggregate Device".to_string(),
            display_index: 1,
        }
    }

    pub fn new_for_tests() -> Self {
        Self {
            dry_run: true,
            aggregate_device_name: "Aggregate Device".to_string(),
            display_index: 1,
        }
    }

    fn normalize_app_name(app_name: Option<&str>) -> Result<String, RecordWindowSkillError> {
        let Some(name) = app_name else {
            return Err(RecordWindowSkillError::Execution(
                "app name must be provided".to_string(),
            ));
        };
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(RecordWindowSkillError::Execution(
                "app name must not be empty".to_string(),
            ));
        }
        Ok(trimmed.to_string())
    }

    fn default_filename() -> String {
        format!(
            "record-window-{}.mp4",
            Local::now().format("%Y-%m-%d-%H%M%S")
        )
    }

    fn normalize_filename(filename: Option<&str>) -> String {
        let Some(filename) = filename else {
            return Self::default_filename();
        };
        let trimmed = filename.trim();
        if trimmed.is_empty() {
            return Self::default_filename();
        }
        if trimmed.ends_with(".mp4") {
            return trimmed.to_string();
        }
        format!("{trimmed}.mp4")
    }

    fn resolve_output_path(filename: Option<&str>) -> Result<PathBuf, RecordWindowSkillError> {
        let home = env::var_os("HOME").ok_or_else(|| {
            RecordWindowSkillError::Execution(
                "HOME is not set, cannot resolve recording path".to_string(),
            )
        })?;
        let output_dir = PathBuf::from(home).join("Movies").join("aice");
        Ok(output_dir.join(Self::normalize_filename(filename)))
    }

    fn ensure_output_dir(path: &Path, dry_run: bool) -> Result<(), RecordWindowSkillError> {
        let output_dir = path.parent().ok_or_else(|| {
            RecordWindowSkillError::Execution("failed to resolve output directory".to_string())
        })?;
        if !dry_run {
            fs::create_dir_all(output_dir)
                .map_err(|e| RecordWindowSkillError::Execution(e.to_string()))?;
        }
        Ok(())
    }

    fn escape_applescript_string(input: &str) -> String {
        input.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn run_osascript(script: &str) -> Result<String, RecordWindowSkillError> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| RecordWindowSkillError::Execution(e.to_string()))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RecordWindowSkillError::Execution(if stderr.is_empty() {
            "osascript command failed".to_string()
        } else {
            stderr
        }))
    }

    fn parse_bounds(raw: &str, app_name: &str) -> Result<WindowBounds, RecordWindowSkillError> {
        if raw == "__APP_NOT_FOUND__" {
            return Err(RecordWindowSkillError::AppNotFound(app_name.to_string()));
        }
        if raw == "__NO_WINDOW__" {
            return Err(RecordWindowSkillError::WindowBoundsUnavailable(
                app_name.to_string(),
            ));
        }
        let parts: Vec<&str> = raw.split(',').collect();
        if parts.len() != 4 {
            return Err(RecordWindowSkillError::WindowBoundsUnavailable(
                app_name.to_string(),
            ));
        }
        let parse = |value: &str| -> Result<i64, RecordWindowSkillError> {
            value
                .trim()
                .parse::<i64>()
                .map_err(|_| RecordWindowSkillError::WindowBoundsUnavailable(app_name.to_string()))
        };
        let bounds = WindowBounds {
            x: parse(parts[0])?,
            y: parse(parts[1])?,
            width: parse(parts[2])?,
            height: parse(parts[3])?,
        };
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err(RecordWindowSkillError::WindowBoundsUnavailable(
                app_name.to_string(),
            ));
        }
        Ok(bounds)
    }

    fn get_window_bounds(&self, app_name: &str) -> Result<WindowBounds, RecordWindowSkillError> {
        if self.dry_run {
            return Ok(WindowBounds {
                x: 20,
                y: 40,
                width: 1280,
                height: 720,
            });
        }
        if !cfg!(target_os = "macos") {
            return Err(RecordWindowSkillError::Execution(
                "record window skill is only available on macOS".to_string(),
            ));
        }

        let escaped = Self::escape_applescript_string(app_name);
        let script = format!(
            "tell application \"System Events\"\n\
             if not (exists process \"{escaped}\") then return \"__APP_NOT_FOUND__\"\n\
             tell process \"{escaped}\"\n\
                 if (count of windows) is 0 then return \"__NO_WINDOW__\"\n\
                 set {{xPos, yPos}} to position of front window\n\
                 set {{w, h}} to size of front window\n\
                 return (xPos as text) & \",\" & (yPos as text) & \",\" & (w as text) & \",\" & (h as text)\n\
             end tell\n\
             end tell"
        );
        let raw = Self::run_osascript(&script)?;
        Self::parse_bounds(&raw, app_name)
    }

    fn get_scale_factor(&self) -> Result<f64, RecordWindowSkillError> {
        if self.dry_run {
            return Ok(2.0);
        }
        let script = "use framework \"AppKit\"\n\
                      set targetScreen to item 1 of (current application's NSScreen's screens())\n\
                      return (backingScaleFactor of targetScreen) as text";
        let raw = Self::run_osascript(script)?;
        raw.parse::<f64>().map_err(|_| {
            RecordWindowSkillError::Execution("invalid display scale factor".to_string())
        })
    }

    fn parse_device_index(line: &str) -> Option<u32> {
        let start = line.find('[')?;
        let end = line[start + 1..].find(']')?;
        line[start + 1..start + 1 + end].trim().parse::<u32>().ok()
    }

    fn find_audio_device_index(&self) -> Result<u32, RecordWindowSkillError> {
        if self.dry_run {
            return Ok(0);
        }
        let output = Command::new("ffmpeg")
            .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RecordWindowSkillError::FfmpegNotFound
                } else {
                    RecordWindowSkillError::Execution(e.to_string())
                }
            })?;

        let mut combined = String::from_utf8_lossy(&output.stderr).to_string();
        combined.push('\n');
        combined.push_str(&String::from_utf8_lossy(&output.stdout));

        let mut in_audio_section = false;
        for line in combined.lines() {
            if line.contains("AVFoundation audio devices") {
                in_audio_section = true;
                continue;
            }
            if line.contains("AVFoundation video devices") {
                in_audio_section = false;
            }
            if !in_audio_section {
                continue;
            }
            if line.contains(&self.aggregate_device_name) {
                if let Some(index) = Self::parse_device_index(line) {
                    return Ok(index);
                }
            }
        }

        Err(RecordWindowSkillError::AudioDeviceNotFound(format!(
            "missing '{}'",
            self.aggregate_device_name
        )))
    }

    fn run_ffmpeg_record(
        &self,
        path: &Path,
        crop: WindowBounds,
        audio_device_index: u32,
        duration_secs: Option<u64>,
    ) -> Result<(), RecordWindowSkillError> {
        if self.dry_run {
            return Ok(());
        }
        let input = format!("{}:{}", self.display_index, audio_device_index);
        let crop_expr = format!("crop={}:{}:{}:{}", crop.width, crop.height, crop.x, crop.y);

        let mut args: Vec<String> = vec![
            "-f".to_string(),
            "avfoundation".to_string(),
            "-capture_cursor".to_string(),
            "1".to_string(),
            "-i".to_string(),
            input,
            "-vf".to_string(),
            crop_expr,
        ];

        if let Some(duration) = duration_secs {
            args.push("-t".to_string());
            args.push(duration.to_string());
        }

        args.extend([
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "ultrafast".to_string(),
            "-crf".to_string(),
            "18".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
            path.to_string_lossy().to_string(),
        ]);

        let output = Command::new("ffmpeg").args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RecordWindowSkillError::FfmpegNotFound
            } else {
                RecordWindowSkillError::Execution(e.to_string())
            }
        })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(RecordWindowSkillError::Execution(if stderr.is_empty() {
            "ffmpeg command failed".to_string()
        } else {
            stderr
        }))
    }

    async fn execute_inner(
        &self,
        app_name: Option<&str>,
        filename: Option<&str>,
        duration_secs: Option<u64>,
    ) -> Result<RecordWindowResult, RecordWindowSkillError> {
        let app_name = Self::normalize_app_name(app_name)?;
        let path = Self::resolve_output_path(filename)?;
        Self::ensure_output_dir(&path, self.dry_run)?;

        let bounds = self.get_window_bounds(&app_name)?;
        let scale = self.get_scale_factor()?;
        let crop = WindowBounds {
            x: (bounds.x as f64 * scale).round() as i64,
            y: (bounds.y as f64 * scale).round() as i64,
            width: (bounds.width as f64 * scale).round() as i64,
            height: (bounds.height as f64 * scale).round() as i64,
        };
        let audio_device_index = self.find_audio_device_index()?;
        self.run_ffmpeg_record(&path, crop, audio_device_index, duration_secs)?;

        Ok(RecordWindowResult {
            path,
            app_name,
            duration_secs,
        })
    }
}

impl Default for MacOsRecordWindowSkill {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RecordWindowSkill for MacOsRecordWindowSkill {
    async fn execute(
        &self,
        app_name: Option<&str>,
        filename: Option<&str>,
        duration_secs: Option<u64>,
    ) -> Result<RecordWindowResult, RecordWindowSkillError> {
        let t0 = Instant::now();
        let result = self.execute_inner(app_name, filename, duration_secs).await;
        match &result {
            Ok(_) => {
                counter!(RECORD_WINDOW_SKILL_EXECUTE_TOTAL, 1, "result" => "success");
            }
            Err(e) => {
                counter!(RECORD_WINDOW_SKILL_EXECUTE_TOTAL, 1, "result" => "error");
                counter!(RECORD_WINDOW_SKILL_ERRORS_TOTAL, 1, "kind" => e.to_string());
            }
        }
        histogram!(
            RECORD_WINDOW_SKILL_EXECUTE_DURATION_SECONDS,
            t0.elapsed().as_secs_f64()
        );
        result
    }
}
