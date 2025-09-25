use egui::{AtomExt as _, IntoAtoms, NumExt as _};
use std::collections::{HashMap, HashSet};

// WARNING: This unused import has been removed.
// use wasm_bindgen::JsCast;

use re_log_types::AbsoluteTimeRange;
use re_redap_browser::EXAMPLES_ORIGIN;
use re_ui::{
    list_item::PropertyContent,
    modal::{ModalHandler, ModalWrapper},
    UiExt as _, icons,
};
use re_uri::Fragment;
use re_viewer_context::{
    DisplayMode, ItemCollection, RecordingConfig, StoreHub, ViewerContext, open_url::ViewerOpenUrl,
};

#[derive(Clone)]
pub struct DownloadableFile {
    pub name: String,
    pub url: String,
    pub file_type: String,
    pub description: String,
}

pub struct ShareModal {
    modal: ModalHandler,

    url: Option<ViewerOpenUrl>,
    create_web_viewer_url: bool,

    /// Whether to show feedback that the download is in progress.
    show_copied_feedback: bool,

    /// Additional files to download (annotation files)
    additional_files: Vec<DownloadableFile>,

    /// Which files are selected for download
    selected_files: HashSet<String>,

    /// Individual download feedback states
    download_feedback: HashMap<String, bool>,
}

impl Default for ShareModal {
    fn default() -> Self {
        let create_web_viewer_url = cfg!(target_arch = "wasm32");

        Self {
            modal: ModalHandler::default(),
            url: None,
            create_web_viewer_url,
            show_copied_feedback: false,
            additional_files: Vec::new(),
            selected_files: HashSet::new(),
            download_feedback: HashMap::new(),
        }
    }
}

impl ShareModal {
    /// Set additional files that can be downloaded
    pub fn set_additional_files(&mut self, files: Vec<DownloadableFile>) {
        self.additional_files = files;
        // Select all files by default
        self.selected_files = self.additional_files.iter().map(|f| f.name.clone()).collect();
    }

    /// Extract base URL from current RRD URL for annotation files
    fn get_annotation_base_url(url_string: &str) -> Option<String> {
        // Remove .rrd extension and use as base for annotation files
        if let Some(base) = url_string.strip_suffix(".rrd") {
            Some(base.to_string())
        } else {
            Some(url_string.to_string())
        }
    }

    /// Generate annotation file URLs based on the RRD URL
    fn generate_annotation_files(&mut self, url_string: &str) {
        if let Some(base_url) = Self::get_annotation_base_url(url_string) {
            let annotation_files = vec![
                DownloadableFile {
                    name: "annotations.mp4".to_string(),
                    url: format!("{}_annotations.mp4", base_url),
                    file_type: "mp4".to_string(),
                    description: "Annotation video file".to_string(),
                },
                DownloadableFile {
                    name: "coordinates.csv".to_string(),
                    url: format!("{}_coordinates.csv", base_url),
                    file_type: "csv".to_string(),
                    description: "Annotation coordinates data".to_string(),
                },
                DownloadableFile {
                    name: "actions.json".to_string(),
                    url: format!("{}_actions.json", base_url),
                    file_type: "json".to_string(),
                    description: "Annotation actions metadata".to_string(),
                },
            ];

            self.set_additional_files(annotation_files);
        }
    }

    /// URL for the current screen, used as a starting point for the modal.
    fn current_url(
        store_hub: &StoreHub,
        display_mode: &DisplayMode,
        rec_cfg: Option<&RecordingConfig>,
        selection: &ItemCollection,
    ) -> anyhow::Result<ViewerOpenUrl> {
        let time_ctrl = rec_cfg.map(|cfg| cfg.time_ctrl.read());
        ViewerOpenUrl::from_context_expanded(
            store_hub,
            display_mode,
            time_ctrl.as_deref(),
            selection,
        )
    }

    /// Opens the share modal with the current URL.
    pub fn open(
        &mut self,
        store_hub: &StoreHub,
        display_mode: &DisplayMode,
        rec_cfg: Option<&RecordingConfig>,
        selection: &ItemCollection,
    ) -> anyhow::Result<()> {
        let url = Self::current_url(store_hub, display_mode, rec_cfg, selection)?;
        self.open_with_url(url);
        Ok(())
    }

    /// Opens the share modal with the given URL.
    fn open_with_url(&mut self, url: ViewerOpenUrl) {
        self.url = Some(url);

        // Generate annotation files based on the URL
        if let Some(url_ref) = &self.url {
            let url_string = url_ref.sharable_url(None).unwrap_or_default();
            self.generate_annotation_files(&url_string);
        }

        self.modal.open();
    }

    /// Downloads the RRD file based on the current URL
    fn download_rrd(url_string: String) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            let download_url = format!("{}.rrd", url_string);

            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Ok(element) = document.create_element("a") {
                        if let Ok(link) = element.dyn_into::<web_sys::HtmlAnchorElement>() {
                            link.set_href(&download_url);
                            link.set_download("recording.rrd");
                            let _ = link.style().set_property("display", "none");

                            if let Some(body) = document.body() {
                                let _ = body.append_child(&link);
                                link.click();
                                let _ = body.remove_child(&link);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Downloads a file from a given URL
    fn download_file_from_url(file_url: &str, filename: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Ok(element) = document.create_element("a") {
                        if let Ok(link) = element.dyn_into::<web_sys::HtmlAnchorElement>() {
                            link.set_href(file_url);
                            link.set_download(filename);
                            link.set_target("_blank");
                            let _ = link.style().set_property("display", "none");

                            if let Some(body) = document.body() {
                                let _ = body.append_child(&link);
                                link.click();
                                let _ = body.remove_child(&link);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Creates a download button for a specific file type
    // FIX: Changed from `&mut self` to passing `download_feedback` directly
    // This avoids borrowing `self` inside the main UI closure.
    fn download_button(
        download_feedback: &mut HashMap<String, bool>,
        ui: &mut egui::Ui,
        button_text: &str,
        file_url: &str,
        filename: &str,
        button_id: &str,
    ) -> bool {
        let is_downloading = download_feedback.get(button_id).copied().unwrap_or(false);

        let button_label = if is_downloading {
            format!("Downloading {}...", filename)
        } else {
            button_text.to_string()
        };

        let button_response = ui.button(&button_label);

        if button_response.clicked() && !is_downloading {
            Self::download_file_from_url(file_url, filename);
            download_feedback.insert(button_id.to_string(), true);

            // Reset feedback after delay
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
            return true;
        } else if is_downloading && !button_response.hovered() {
            // Reset feedback when not hovering
            download_feedback.insert(button_id.to_string(), false);
        }

        false
    }


    /// Button that opens the share popup.
    pub fn button_ui(
        &mut self,
        ui: &mut egui::Ui,
        store_hub: &StoreHub,
        display_mode: &DisplayMode,
        rec_cfg: Option<&RecordingConfig>,
        selection: &ItemCollection,
    ) {
        re_tracing::profile_function!();

        let url_for_current_screen = Self::current_url(store_hub, display_mode, rec_cfg, selection);
        let enable_share_button = url_for_current_screen.is_ok()
            && display_mode != &DisplayMode::RedapServer(EXAMPLES_ORIGIN.clone());

        let button_text = if cfg!(target_arch = "wasm32") {
            "Export"
        } else {
            "Share"
        };

        let share_button_resp = ui
            .add_enabled_ui(enable_share_button, |ui| ui.button(button_text))
            .inner;

        match url_for_current_screen {
            Err(err) => {
                let error_text = if cfg!(target_arch = "wasm32") {
                    format!("Cannot create export URL: {err}")
                } else {
                    format!("Cannot create share URL: {err}")
                };
                share_button_resp.on_disabled_hover_text(error_text);
            }
            Ok(url) => {
                if share_button_resp.clicked() {
                    self.open_with_url(url);
                }
            }
        }
    }

    /// Draws the share modal dialog if its open.
    pub fn ui(
        &mut self,
        ctx: &ViewerContext<'_>,
        ui: &egui::Ui,
        web_viewer_base_url: Option<&url::Url>,
    ) {
        let Some(url) = &mut self.url else {
            debug_assert!(!self.modal.is_open());
            return;
        };

        let modal_title = if cfg!(target_arch = "wasm32") {
            "Export"
        } else {
            "Share"
        };

        // FIX: Extract mutable fields from `self` before the closure.
        // This allows the closure to borrow these fields without borrowing all of `self`,
        // resolving the conflict with the `self.modal.ui` call.
        let create_web_viewer_url = &mut self.create_web_viewer_url;
        let download_feedback = &mut self.download_feedback;
        let show_copied_feedback = &mut self.show_copied_feedback;

        self.modal.ui(
            ui.ctx(),
            || ModalWrapper::new(modal_title),
            |ui| {
                let panel_max_height = (ui.ctx().screen_rect().height() - 100.0)
                    .at_least(0.0)
                    .at_most(640.0);
                ui.set_max_height(panel_max_height);

                // URL display
                let url_string = {
                    let web_viewer_base_url = if *create_web_viewer_url {
                        web_viewer_base_url
                    } else {
                        None
                    };
                    let url_string = url.sharable_url(web_viewer_base_url).unwrap_or_default();

                    let mut url_for_text_edit = url_string.clone();
                    egui::TextEdit::singleline(&mut url_for_text_edit)
                        .hint_text(if cfg!(target_arch = "wasm32") { "<can't export file>" } else { "<can't share link>" })
                        .text_color(ui.style().visuals.strong_text_color())
                        .desired_width(f32::INFINITY)
                        .show(ui);

                    url_string
                };

                if cfg!(target_arch = "wasm32") {
                    ui.add_space(12.0);

                    // Main RRD download button
                    let rrd_downloading = download_feedback.get("rrd").copied().unwrap_or(false);
                    let rrd_button_label = if rrd_downloading {
                        "Downloading RRD..."
                    } else {
                        "Download RRD"
                    };

                    let rrd_button_response = ui
                        .scope(|ui| {
                            let tokens = ui.tokens();
                            let visuals = &mut ui.style_mut().visuals;
                            visuals.override_text_color = Some(tokens.text_inverse);

                            let fill_color = tokens.bg_fill_inverse;

                            ui.add(
                                egui::Button::new(rrd_button_label)
                                    .fill(fill_color)
                                    .min_size(egui::vec2(ui.available_width(), 32.0)),
                            )
                        })
                        .inner;

                    if rrd_button_response.clicked() && !rrd_downloading {
                        Self::download_rrd(url_string.clone());
                        download_feedback.insert("rrd".to_string(), true);
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
                    } else if rrd_downloading && !rrd_button_response.hovered() {
                        download_feedback.insert("rrd".to_string(), false);
                    }

                    ui.add_space(16.0);

                    // Annotation download buttons
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Download Annotations:");
                            ui.add_space(8.0);

                            if let Some(base_url) = Self::get_annotation_base_url(&url_string) {
                                // Video annotations button
                                Self::download_button(
                                    download_feedback,
                                    ui,
                                    "Download Annotations Video",
                                    &format!("{}_annotations.mp4", base_url),
                                    "annotations.mp4",
                                    "video"
                                );

                                ui.add_space(4.0);

                                // Coordinates CSV button
                                Self::download_button(
                                    download_feedback,
                                    ui,
                                    "Download Annotations Coordinates",
                                    &format!("{}_coordinates.csv", base_url),
                                    "coordinates.csv",
                                    "coordinates"
                                );

                                ui.add_space(4.0);

                                // Actions JSON button
                                Self::download_button(
                                    download_feedback,
                                    ui,
                                    "Download Annotations Actions.json",
                                    &format!("{}_actions.json", base_url),
                                    "actions.json",
                                    "actions"
                                );
                            }
                        });
                    });
                } else {
                    // Native share functionality
                    let copy_link_label = if *show_copied_feedback {
                        (
                            egui::Atom::grow(),
                            "Copied to clipboard!",
                            egui::Atom::grow(),
                        )
                            .into_atoms()
                    } else {
                        (
                            egui::Atom::grow(),
                            icons::URL.as_image().tint(ui.tokens().icon_inverse),
                            "Copy link",
                            egui::Atom::grow(),
                        )
                            .into_atoms()
                    };

                    let copy_link_response = ui
                        .scope(|ui| {
                            let tokens = ui.tokens();
                            let visuals = &mut ui.style_mut().visuals;
                            visuals.override_text_color = Some(tokens.text_inverse);

                            let fill_color = if ui.ctx().read_response(ui.next_auto_id())
                                .is_some_and(|r| r.hovered()) {
                                tokens.bg_fill_inverse_hover
                            } else {
                                tokens.bg_fill_inverse
                            };

                            ui.add(
                                egui::Button::new(copy_link_label)
                                    .fill(fill_color)
                                    .min_size(egui::vec2(ui.available_width(), 20.0)),
                            )
                        })
                        .inner;

                    if copy_link_response.clicked() {
                        ui.ctx().copy_text(url_string.clone());
                        *show_copied_feedback = true;
                    } else if !copy_link_response.hovered() {
                        *show_copied_feedback = false;
                    }
                }

                ui.list_item_scope("share_dialog_url_settings", |ui| {
                    url_settings_ui(ctx, ui, url, create_web_viewer_url);
                });
            },
        );
    }
}

// No changes were needed in the helper functions below.
fn selectable_value_with_min_width<'a, Value: PartialEq>(
    ui: &mut egui::Ui,
    min_width: f32,
    current_value: &mut Value,
    selected_value: Value,
    text: impl IntoAtoms<'a>,
) -> egui::Response {
    let checked = *current_value == selected_value;
    let mut response = ui.add(
        egui::Button::selectable(checked, text)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .min_size(egui::vec2(min_width, 0.0)),
    );

    if response.clicked() && *current_value != selected_value {
        *current_value = selected_value;
        response.mark_changed();
    }
    response
}

fn selectable_value_with_available_width<'a, Value: PartialEq>(
    ui: &mut egui::Ui,
    current_value: &mut Value,
    selected_value: Value,
    text: impl IntoAtoms<'a>,
) -> egui::Response {
    selectable_value_with_min_width(
        ui,
        ui.available_width(),
        current_value,
        selected_value,
        text,
    )
}

const MIN_TOGGLE_WIDTH_RH: f32 = 120.0;

fn url_settings_ui(
    ctx: &ViewerContext<'_>,
    ui: &mut egui::Ui,
    url: &mut ViewerOpenUrl,
    create_web_viewer_url: &mut bool,
) {
    let link_format_text = if cfg!(target_arch = "wasm32") {
        "Export format"
    } else {
        "Link format"
    };

    ui.list_item_flat_noninteractive(PropertyContent::new(link_format_text).value_fn(|ui, _| {
        ui.selectable_toggle(|ui| {
            let (first_option, second_option) = if cfg!(target_arch = "wasm32") {
                ("RRD only", "Full export")
            } else {
                ("Only source", "Web viewer")
            };

            let (first_tooltip, second_tooltip) = if cfg!(target_arch = "wasm32") {
                ("Download only the RRD file.", "Download RRD with additional metadata.")
            } else {
                ("Link works only in already opened viewers and not in the browser's address bar.", "Link works in the browser's address bar, opening a new viewer. You can still use this link in the native viewer as well.")
            };

            selectable_value_with_min_width(ui, MIN_TOGGLE_WIDTH_RH, create_web_viewer_url, false, first_option)
                .on_hover_text(first_tooltip);
            selectable_value_with_available_width(ui, create_web_viewer_url, true, second_option)
                .on_hover_text(second_tooltip);
        });
    }));

    if let Some(url_time_range) = url.time_range_mut() {
        ui.add_space(8.0);
        time_range_ui(ui, url_time_range, ctx.rec_cfg);
    }
    if let Some(fragments) = url.fragment_mut() {
        ui.add_space(8.0);

        let timestamp_format = ctx.app_options().timestamp_format;
        time_cursor_ui(ui, fragments, timestamp_format, ctx.rec_cfg);
    }
}

fn time_range_ui(
    ui: &mut egui::Ui,
    url_time_range: &mut Option<re_uri::TimeSelection>,
    rec_cfg: &RecordingConfig,
) {
    let current_time_range_selection = {
        let time_ctrl = rec_cfg.time_ctrl.read();
        time_ctrl
            .loop_selection()
            .map(|range| re_uri::TimeSelection {
                timeline: *time_ctrl.timeline(),
                range: AbsoluteTimeRange::new(range.min.floor(), range.max.ceil()),
            })
    };

    let mut entire_range = url_time_range.is_none();
    let trim_text = if cfg!(target_arch = "wasm32") {
        "Export range"
    } else {
        "Trim range"
    };

    ui.list_item_flat_noninteractive(PropertyContent::new(trim_text).value_fn(|ui, _| {
        ui.selectable_toggle(|ui| {
            let (entire_text, trim_text) = if cfg!(target_arch = "wasm32") {
                ("Entire recording", "Export selection")
            } else {
                ("Entire recording", "Trim to selection")
            };

            let (entire_tooltip, trim_tooltip) = if cfg!(target_arch = "wasm32") {
                ("Export will include the entire recording.", "Export will include only the selected time range.")
            } else {
                ("Link will share the entire recording.", "Link trims the recording to the selected time range.")
            };

            selectable_value_with_min_width(
                ui,
                MIN_TOGGLE_WIDTH_RH,
                &mut entire_range,
                true,
                entire_text,
            )
            .on_hover_text(entire_tooltip);
            ui.add_enabled_ui(current_time_range_selection.is_some(), |ui| {
                selectable_value_with_available_width(
                    ui,
                    &mut entire_range,
                    false,
                    trim_text,
                )
                .on_disabled_hover_text("No time range selected.")
                .on_hover_text(trim_tooltip);
            });
        });
    }));

    if entire_range {
        *url_time_range = None;
    } else {
        *url_time_range = current_time_range_selection;
    }
}

fn time_cursor_ui(
    ui: &mut egui::Ui,
    fragments: &mut Fragment,
    timestamp_format: re_log_types::TimestampFormat,
    rec_cfg: &RecordingConfig,
) {
    let Fragment {
        selection: _,
        when,
    } = fragments;

    let current_time_cursor = {
        let time_ctrl = rec_cfg.time_ctrl.read();
        time_ctrl
            .time_cell()
            .map(|cell| (*time_ctrl.timeline().name(), cell))
    };

    let mut any_time = when.is_some();
    ui.list_item_flat_noninteractive(PropertyContent::new("Time cursor").value_fn(|ui, _| {
        ui.selectable_toggle(|ui| {
            selectable_value_with_min_width(
                ui,
                MIN_TOGGLE_WIDTH_RH,
                &mut any_time,
                false,
                "At the start",
            );
            ui.add_enabled_ui(current_time_cursor.is_some(), |ui| {
                let mut label = egui::Atoms::new(egui::Atom::from("Current"));
                if let Some((_, time_cell)) = current_time_cursor {
                    label.push_right({
                        let time = time_cell.format(timestamp_format);
                        egui::RichText::new(time).weak().small().atom_shrink(true)
                    });
                }
                label.push_left(egui::Atom::grow());
                label.push_right(egui::Atom::grow());

                selectable_value_with_available_width(ui, &mut any_time, true, label)
                    .on_disabled_hover_text("No time selected.");
            });
        });
    }));
    if any_time {
        *when = current_time_cursor;
    } else {
        *when = None;
    }
}
