use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Align, Layout, Vec2};

use crate::theme;
use crate::ui::layout::btn_ghost;

/// Shared selectable, color-tagged, vertically resizable Hub log console.
/// Returns true when Clear is clicked.
pub fn show(
    ui: &mut egui::Ui,
    id_salt: &str,
    lines: &[String],
    default_kind: &'static str,
    empty: &str,
) -> bool {
    let mut clear = false;
    let auto_scroll_id = egui::Id::new(format!("{id_salt}_auto_scroll"));
    let mut auto_scroll = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(auto_scroll_id).unwrap_or(true));

    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add(btn_ghost("Clear")).clicked() {
                clear = true;
            }
            if ui.checkbox(&mut auto_scroll, "Auto-scroll").changed() {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(auto_scroll_id, auto_scroll));
            }
        });
    });
    ui.add_space(5.0);

    let full_width = ui.available_width();
    egui::Resize::default()
        .id_salt(format!("{id_salt}_resize"))
        .default_size(Vec2::new(full_width, 260.0))
        .min_width(full_width)
        .max_width(full_width)
        .min_height(150.0)
        .max_height(480.0)
        .show(ui, |ui| {
            ui.set_min_width(full_width);
            ui.set_max_width(full_width);
            egui::Frame::new()
                .fill(theme::bg())
                .stroke(theme::card_stroke())
                .corner_radius(5.0)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if lines.is_empty() {
                        ui.colored_label(theme::muted(), empty);
                        return;
                    }

                    let display_text = decorate(lines, default_kind);
                    let mut readonly = display_text.as_str();
                    let mut layouter =
                        |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                            let mut job = layout_job(text.as_str());
                            job.wrap.max_width = wrap_width;
                            ui.fonts_mut(|fonts| fonts.layout_job(job))
                        };

                    egui::ScrollArea::vertical()
                        .id_salt(format!("{id_salt}_scroll"))
                        .auto_shrink([false, false])
                        .stick_to_bottom(auto_scroll)
                        .show(ui, |ui| {
                            ui.add_sized(
                                [ui.available_width(), ui.available_height().max(140.0)],
                                egui::TextEdit::multiline(&mut readonly)
                                    .font(theme::mono_small())
                                    .desired_width(f32::INFINITY)
                                    .layouter(&mut layouter),
                            );
                        });
                });
        });

    clear
}

fn decorate(lines: &[String], default_kind: &'static str) -> String {
    lines
        .iter()
        .map(|line| {
            let clean = strip_ansi(line);
            let (stamp, message) = split_timestamp(&clean);
            let kind = classify(message, default_kind);
            if stamp.is_empty() {
                format!("[{kind}] {message}")
            } else {
                format!("{stamp}  [{kind}] {message}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_timestamp(line: &str) -> (&str, &str) {
    if line.len() >= 8 {
        let bytes = line.as_bytes();
        if bytes.get(2) == Some(&b':') && bytes.get(5) == Some(&b':') {
            return (&line[..8], line[8..].trim_start());
        }
    }
    ("", line)
}

fn classify(message: &str, default_kind: &'static str) -> &'static str {
    let lower = message.trim_start().to_ascii_lowercase();

    // Gate output often contains source-code words such as `error` or `warn` inside
    // rustfmt diffs. Do not label those source lines as failures. Only classify explicit
    // diagnostics; everything else remains GATE.
    if default_kind == "GATE" {
        if lower.starts_with("error:")
            || lower.starts_with("error[")
            || lower.contains("test result: failed")
            || lower.contains("could not compile")
            || lower.contains("quality gate failed")
        {
            return "ERROR";
        }
        if lower.starts_with("warning:") {
            return "WARN";
        }
        return "GATE";
    }

    if lower.contains("fail") || lower.contains("error") || lower.contains("panic") {
        "ERROR"
    } else if lower.contains("warn") || lower.contains("degraded") || lower.contains("not running")
    {
        "WARN"
    } else if lower.contains("build")
        || lower.contains("rebuild")
        || lower.contains("cargo")
        || lower.contains("compil")
    {
        "BUILD"
    } else if lower.contains("client") {
        "CLIENT"
    } else if lower.contains("server") || lower.contains("probe") || lower.contains("listener") {
        "SERVER"
    } else if lower.contains("load") {
        "LOAD"
    } else {
        default_kind
    }
}

fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
            continue;
        }
        let ch = input[i..].chars().next().expect("valid utf-8 boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn layout_job(text: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = theme::mono_small();

    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            job.append(
                "\n",
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::body(),
                    ..Default::default()
                },
            );
        }

        let (stamp, rest) = split_timestamp(line);
        let rest = rest.trim_start();
        if !stamp.is_empty() {
            job.append(
                stamp,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::muted(),
                    ..Default::default()
                },
            );
            job.append(
                "  ",
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::muted(),
                    ..Default::default()
                },
            );
        }

        if let Some(end) = rest.find(']')
            && rest.starts_with('[')
        {
            let tag = &rest[..=end];
            let message = rest[end + 1..].trim_start();
            job.append(
                tag,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: tag_color(tag),
                    ..Default::default()
                },
            );
            job.append(
                "  ",
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::body(),
                    ..Default::default()
                },
            );
            job.append(
                message,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::body(),
                    ..Default::default()
                },
            );
        } else {
            job.append(
                rest,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::body(),
                    ..Default::default()
                },
            );
        }
    }

    job
}

fn tag_color(tag: &str) -> egui::Color32 {
    match tag {
        "[ERROR]" => theme::destructive(),
        "[WARN]" => egui::Color32::from_rgb(220, 160, 50),
        "[BUILD]" => egui::Color32::from_rgb(190, 110, 220),
        "[GATE]" => egui::Color32::from_rgb(210, 175, 70),
        "[CLIENT]" => egui::Color32::from_rgb(80, 180, 210),
        "[SERVER]" => theme::success(),
        "[LOAD]" => egui::Color32::from_rgb(170, 140, 210),
        _ => theme::accent(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_errors_before_stream_kind() {
        assert_eq!(classify("client connected", "CLIENT"), "CLIENT");
        assert_eq!(classify("server error: bind failed", "SERVER"), "ERROR");
        assert_eq!(classify("Compiling purgatory-client", "CLIENT"), "BUILD");
        assert_eq!(classify("quality-gate | checking fmt", "INFO"), "INFO");
    }

    #[test]
    fn gate_source_diff_words_are_not_false_errors() {
        assert_eq!(
            classify("if lower.contains(\"error\") {", "GATE"),
            "GATE"
        );
        assert_eq!(classify("error: could not compile `x`", "GATE"), "ERROR");
        assert_eq!(classify("warning: unused import", "GATE"), "WARN");
    }

    #[test]
    fn strips_terminal_ansi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[31m-red\u{1b}[0m"), "-red");
    }
}
