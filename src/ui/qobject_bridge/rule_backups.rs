use super::*;

impl qobject::AppController {
    pub fn backup_rules(self: Pin<&mut Self>, name: QString) -> QString {
        let name_str: String = name.to_string();
        let backup_dir = config_path("rule_backups");
        if let Err(e) = std::fs::create_dir_all(&backup_dir) {
            log::error!("Failed to create backup dir: {}", e);
            return QString::from("");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let secs = now as i64;
        let days = secs / 86400;
        let time_of_day = (secs % 86400) as u32;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;
        let (year, month, day) = days_to_ymd(days);
        let timestamp = format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            year, month, day, hours, minutes, seconds
        );
        let safe_name = if name_str.trim().is_empty() {
            timestamp.clone()
        } else {
            let sanitized: String = name_str
                .trim()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("{}_{}", timestamp, sanitized)
        };
        let filename = format!("{}.json", safe_name);
        let dest = backup_dir.join(&filename);

        let src = config_path("rules.json");
        match std::fs::read_to_string(&src) {
            Ok(content) => {
                if write_atomic(&dest, content.as_bytes()).is_err() {
                    return QString::from("");
                }
                log::info!("Rules backed up to {:?}", dest);
                QString::from(&filename)
            }
            Err(e) => {
                log::error!("Failed to read rules for backup: {}", e);
                QString::from("")
            }
        }
    }

    pub fn list_rule_backups_json(self: Pin<&mut Self>) -> QString {
        let backup_dir = config_path("rule_backups");
        let mut backups: Vec<serde_json::Value> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    let rule_count = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Vec<serde_json::Value>>(&s).ok())
                        .map(|v| v.len())
                        .unwrap_or(0);

                    let stem = filename.trim_end_matches(".json");
                    let (parsed_date, display_name) = parse_backup_stem(stem);

                    let date_str = if let Some(d) = parsed_date {
                        d
                    } else {
                        entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|dur| {
                                let secs = dur.as_secs() as i64;
                                let tod = (secs % 86400) as u32;
                                let (y, mo, d) = days_to_ymd(secs / 86400);
                                format!(
                                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                                    y,
                                    mo,
                                    d,
                                    tod / 3600,
                                    (tod % 3600) / 60,
                                    tod % 60
                                )
                            })
                            .unwrap_or_default()
                    };

                    backups.push(serde_json::json!({
                        "filename": filename,
                        "name": display_name,
                        "date": date_str,
                        "ruleCount": rule_count,
                    }));
                }
            }
        }

        backups.sort_by(|a, b| {
            let fa = a["filename"].as_str().unwrap_or("");
            let fb = b["filename"].as_str().unwrap_or("");
            fb.cmp(fa)
        });

        let json = serde_json::to_string(&backups).unwrap_or_else(|_| "[]".to_string());
        QString::from(&json)
    }

    pub fn restore_rule_backup(mut self: Pin<&mut Self>, filename: QString) {
        let filename_str: String = filename.to_string();
        if !is_plain_file_name(&filename_str) {
            log::error!(
                "restore_rule_backup: invalid backup name {:?}",
                filename_str
            );
            return;
        }
        let backup_path = config_path("rule_backups").join(&filename_str);

        match std::fs::read_to_string(&backup_path) {
            Ok(content) => {
                match serde_json::from_str::<Vec<crate::patchbay::rules::AutoConnectRule>>(&content)
                {
                    Ok(rules) => {
                        let rules_path = config_path("rules.json");
                        if write_atomic(&rules_path, content.as_bytes()).is_err() {
                            return;
                        }
                        if let Some(ref mut patchbay) = self.as_mut().rust_mut().patchbay {
                            patchbay.set_rules(rules.clone());
                        }
                        log::info!(
                            "Restored {} rules from backup {:?}",
                            rules.len(),
                            filename_str
                        );
                        self.as_mut().rules_changed();
                    }
                    Err(e) => {
                        log::error!(
                            "Backup file {:?} contains invalid rules: {}",
                            filename_str,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read backup {:?}: {}", backup_path, e);
            }
        }
    }

    pub fn delete_rule_backup(self: Pin<&mut Self>, filename: QString) {
        let filename_str: String = filename.to_string();
        if !is_plain_file_name(&filename_str) {
            log::error!("delete_rule_backup: invalid backup name {:?}", filename_str);
            return;
        }
        let backup_path = config_path("rule_backups").join(&filename_str);
        if let Err(e) = std::fs::remove_file(&backup_path) {
            log::error!("Failed to delete backup {:?}: {}", backup_path, e);
        } else {
            log::info!("Deleted rule backup: {}", filename_str);
        }
    }
}
