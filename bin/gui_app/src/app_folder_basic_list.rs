use std::sync::Arc;
use app::file_intent::Action;
use app::app_folder::AppFolder;
use egui;
use tokio;
use crate::fuzzy_search::{FuzzySearcher, render_search_bar};
use crate::clipped_selectable::ClippedSelectableLabel;
use crate::app_file_actions::{check_file_shortcuts, render_file_context_menu};
use crate::app_bookmarks::render_file_bookmarks;

pub fn render_files_basic_list(
    ui: &mut egui::Ui, 
    searcher: &mut FuzzySearcher, selected_action: Action, folder: &Arc<AppFolder>,
) {
    let file_tracker = folder.get_file_tracker().blocking_read();
    let mut files = folder.get_mut_files_blocking();
    let mut bookmarks = folder.get_bookmarks().blocking_write();
    let mut is_bookmarks_changed = false;

    render_search_bar(ui, searcher);

    if file_tracker.get_action_count()[selected_action] == 0 {
        ui.heading(format!("No {}s", selected_action.to_str().to_lowercase()));
        return;
    }

    let is_not_busy = folder.get_busy_lock().try_lock().is_ok();
    let selected_descriptor = *folder.get_selected_descriptor().blocking_read();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
        ui.with_layout(layout, |ui| {
            let mut files_iter = files.to_iter();
            while let Some(mut file) = files_iter.next_mut() {
                let action = file.get_action();
                if action != selected_action {
                    continue;
                }

                if !searcher.search(file.get_src()) {
                    continue;
                }

                ui.horizontal(|ui| {
                    {
                        let src = file.get_src();
                        let bookmark = bookmarks.get_mut_with_insert(src);
                        is_bookmarks_changed = render_file_bookmarks(ui, bookmark) || is_bookmarks_changed;
                    }
                    let layout = egui::Layout::top_down(egui::Align::Min).with_cross_justify(true);
                    ui.with_layout(layout, |ui| {
                        let src = file.get_src();
                        let descriptor = file.get_src_descriptor();
                        let is_selected = descriptor.is_some() && *descriptor == selected_descriptor;
                        let elem = ClippedSelectableLabel::new(is_selected, src);
                        let res = ui.add(elem);
                        if res.clicked() {
                            if is_selected {
                                *folder.get_selected_descriptor().blocking_write() = None;
                            } else {
                                *folder.get_selected_descriptor().blocking_write() = *descriptor;
                            }
                        }
                        if is_not_busy && res.hovered() {
                            check_file_shortcuts(ui, &mut file);
                        }
                        res.context_menu(|ui| {
                            render_file_context_menu(ui, folder.get_folder_path(), &mut file, is_not_busy);
                        });
                    });
                });
            }
        });
    });

    if is_bookmarks_changed {
        tokio::spawn({
            let folder = folder.clone();
            async move {
                folder.save_bookmarks_to_file().await
            }
        });
    }
}
