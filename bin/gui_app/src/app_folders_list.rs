use app::app::App;
use app::app_folder::FolderStatus;
use egui;
use enum_map;
use open as cross_open;
use std::sync::Arc;
use tokio;
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;

lazy_static::lazy_static! {
    static ref FOLDER_STATUS_ICONS: enum_map::EnumMap<FolderStatus, egui::RichText> = enum_map::enum_map! {
        FolderStatus::Unknown => egui::RichText::new("？").strong().color(egui::Color32::DARK_RED),
        FolderStatus::Empty => egui::RichText::new("O").strong().color(egui::Color32::GRAY),
        FolderStatus::Pending => egui::RichText::new("🖹").strong().color(egui::Color32::DARK_BLUE),
        FolderStatus::Done => egui::RichText::new("✔").strong().color(egui::Color32::DARK_GREEN),
    };
}

pub struct GuiAppFoldersList {
    searcher: FuzzySearcher,
    filters: enum_map::EnumMap<FolderStatus, bool>,
}

impl GuiAppFoldersList {
    pub fn new() -> Self {
        Self {
            searcher: FuzzySearcher::new(),
            filters: enum_map::enum_map! { _ => true },
        }
    }
}

impl Default for GuiAppFoldersList {
    fn default() -> Self {
        Self::new()
    }
}

fn render_folder_status(ui: &mut egui::Ui, status: FolderStatus, is_busy: bool) {
    let height = ui.text_style_height(&egui::TextStyle::Monospace);
    let size = egui::vec2(height, height);
    if !is_busy {
        let icon = FOLDER_STATUS_ICONS[status].clone().size(height);
        let elem = egui::Label::new(icon);
        ui.add_sized(size, elem);
    } else {
        let icon = egui::RichText::new("↻").strong().size(height);
        let elem = egui::Label::new(icon);
        // The spinner forces a ui refresh which could be unnecessarily expensive
        // But it looks cool so I'm keeping it
        // let elem = egui::Spinner::new();
        ui.add_sized(size, elem);
    }
}

fn render_folders_controls(
    ui: &mut egui::Ui, app: &Arc<App>,
    is_show_settings: &mut bool, is_busy: bool
) {
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!is_busy, |ui| {
            let res = ui.button("Refresh all");
            if res.clicked() {
                tokio::spawn({
                    let app = app.clone();
                    async move {
                        app.update_file_intents_for_all_folders().await
                    }
                });
            }
            res.on_disabled_hover_ui(|ui| {
                ui.label("Folders are busy");
            });

            let res = ui.button("Reload structure");
            if res.clicked() {
                tokio::spawn({
                    let app = app.clone();
                    async move {
                        app.load_folders_from_existing_root_path().await
                    }
                });
            }
            res.on_disabled_hover_ui(|ui| {
                ui.label("Folders are busy");
            });
        });

        if ui.button("Login").clicked() {
            tokio::spawn({
                let app = app.clone();
                async move {
                    app.login().await
                }
            });
        }

        let is_logged_in = app.get_login_session().blocking_read().is_some();
        let login_icon = match is_logged_in {
            true => egui::RichText::new("✔").strong().color(egui::Color32::DARK_GREEN),
            false => egui::RichText::new("🗙").strong().color(egui::Color32::DARK_RED),
        };
        ui.label(login_icon).on_hover_ui(|ui| {
            if is_logged_in {
                ui.label("Login successful");
            } else {
                ui.label("Logged out");
            }
        });

        if ui.selectable_label(*is_show_settings, "⛭").clicked() {
            *is_show_settings = !*is_show_settings;
        }
    });
}

fn render_folders_progress_bar(ui: &mut egui::Ui, total_finished: usize, total_folders: usize) {
    let total_progress: f32 = total_finished as f32 / total_folders as f32;
    let elem = egui::ProgressBar::new(total_progress)
        .text(format!("{}/{}", total_finished, total_folders))
        .desired_width(ui.available_width())
        .desired_height(ui.spacing().interact_size.y);
    ui.add(elem);
}

fn render_folders_status_filter(
    ui: &mut egui::Ui,
    status_counts: &enum_map::EnumMap<FolderStatus, usize>,
    filters: &mut enum_map::EnumMap<FolderStatus, bool>,
) {
    let layout = egui::Layout::left_to_right(egui::Align::Min)
        .with_main_justify(true)
        .with_main_wrap(true);
    ui.with_layout(layout, |ui| {
        let total_columns = 2;
        egui::Grid::new("status_filter_flags")
            .num_columns(total_columns)
            .striped(true)
            .show(ui, |ui| {
                for (index, status) in FolderStatus::iterator().enumerate() {
                    let status = *status;
                    let flag = &mut filters[status];
                    let checkbox = egui::Checkbox::new(flag, format!("{} ({})", status.to_str(), status_counts[status]));
                    ui.add(checkbox);
                    if (index + 1) % total_columns == 0 {
                        ui.end_row();
                    }
                }
            });
    });
}

pub fn render_folders_list(
    ui: &mut egui::Ui,
    gui: &mut GuiAppFoldersList, app: &Arc<App>, is_show_settings: &mut bool,
) {
    let folders = app.get_folders().blocking_read();
    let is_busy = app.get_folders_busy_lock().try_lock().is_err();
    let mut status_counts: enum_map::EnumMap<FolderStatus, usize> = enum_map::enum_map! { _ => 0 };
    for folder in folders.iter() {
        let status = folder.get_folder_status_blocking();
        status_counts[status] += 1; 
    }

    render_folders_controls(ui, app, is_show_settings, is_busy);
    render_folders_progress_bar(ui, status_counts[FolderStatus::Done], folders.len());
    ui.separator();
    render_folders_status_filter(ui, &status_counts, &mut gui.filters);
    render_search_bar(ui, &mut gui.searcher);

    if folders.is_empty() {
        if is_busy {
            ui.spinner();
        } else {
            ui.label("No folders");
        }
        return;
    }
 
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let selected_index = *app.get_selected_folder_index().blocking_read();
            for (index, folder) in folders.iter().enumerate() {
                let label = folder.get_folder_name();
                if !gui.searcher.search(label) {
                    continue;
                }

                let status = folder.get_folder_status_blocking();
                if !gui.filters[status] {
                    continue;
                }

                ui.horizontal(|ui| {
                    let is_busy = folder.get_busy_lock().try_lock().is_err();
                    render_folder_status(ui, status, is_busy);
                    let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
                    ui.with_layout(layout, |ui| {
                        let is_selected = selected_index == Some(index);
                        let elem = ClippedSelectableLabel::new(is_selected, folder.get_folder_name());
                        let res = ui.add(elem);
                        if res.clicked() {
                            let mut selected_index = app.get_selected_folder_index().blocking_write();
                            if !is_selected {
                                *selected_index = Some(index);
                            } else {
                                *selected_index = None;
                            }
                        }
                        res.context_menu(|ui| {
                            if ui.button("Open folder").clicked() {
                                tokio::spawn({
                                    let folder_path_str = folder.get_folder_path().to_string();
                                    async move {
                                        cross_open::that(folder_path_str)
                                    }
                                });
                                ui.close_menu();
                            }
                        });
                    });
                });
            }
        });
    });
}

