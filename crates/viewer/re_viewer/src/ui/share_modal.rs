use egui::{vec2, AtomExt as _, IntoAtoms, NumExt as _};
use std::collections::{HashMap, HashSet};

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

// --- CORRECTED HELPER FOR THE EXACT BUTTON STYLE YOU WANT ---
/// Renders a full-width button with the primary "inverted" style.
fn primary_button_style(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let widget_text = text.into();
    let button_id_source = widget_text.text().to_owned();

    // This scope correctly styles the button and handles hover effects.
    ui.scope(|ui| {
        let tokens = ui.tokens();
        let visuals = &mut ui.style_mut().visuals;
        visuals.override_text_color = Some(tokens.text_inverse);

        // Pre-sense the interaction to get the hover state before drawing.
        let response = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with(button_id_source), // Use the button's text for a stable ID
            egui::Sense::click(),
        );

        let fill_color = if response.hovered() {
            tokens.bg_fill_inverse_hover
        } else {
            tokens.bg_fill_inverse
        };

        // Create and add the actual button widget.
        let button = egui::Button::new(widget_text)
            .fill(fill_color)
            .min_size(vec2(ui.available_width(), 32.0));

        ui.add(button)
    })
    .inner
}


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
    show_copied_feedback: bool,
    additional_files: Vec<DownloadableFile>,
    selected_files: HashSet<String>,
    download_feedback: HashMap<String, bool>,
}

impl Default for ShareModal {
    fn default() -> Self {
        Self {
            modal: ModalHandler::default(),
            url: None,
            create_web_viewer_url: cfg!(target_arch = "wasm32"),
            show_copied_feedback: false,
            additional_files: Vec::new(),
            selected_files: HashSet::new(),
            download_feedback: HashMap::new(),
        }
    }
}

impl ShareModal {
    pub fn set_additional_files(&mut self, files: Vec<DownloadableFile>) {
        self.additional_files = files;
        self.selected_files = self.additional_files.iter().map(|f| f.name.clone()).collect();
    }

    fn get_annotation_base_url(url_string: &str) -> Option<String> {
        url_string.strip_suffix(".rrd").map_or_else(
            || Some(url_string.to_string()),
            |base| Some(base.to_string()),
        )
    }

    fn generate_annotation_files(&mut self, url_string: &str) {
        if let Some(base_url) = Self::get_annotation_base_url(url_string) {
            self.set_additional_files(vec![
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
            ]);
        }
    }

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

    fn open_with_url(&mut self, url: ViewerOpenUrl) {
        self.url = Some(url);
        // On native, we might still generate files for sharing. On web, this is no longer used.
        if !cfg!(target_arch = "wasm32") {
            if let Some(url_ref) = &self.url {
                let url_string = url_ref.sharable_url(None).unwrap_or_default();
                self.generate_annotation_files(&url_string);
            }
        }
        self.modal.open();
    }

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

    pub fn button_ui(
        &mut self,
        ui: &mut egui::Ui,
        store_hub: &StoreHub,
        display_mode: &DisplayMode,
        rec_cfg: Option<&RecordingConfig>,
        selection: &ItemCollection,
    ) {
        let url_for_current_screen = Self::current_url(store_hub, display_mode, rec_cfg, selection);
        let enable_share_button = url_for_current_screen.is_ok()
            && display_mode != &DisplayMode::RedapServer(EXAMPLES_ORIGIN.clone());

        let button_text = if cfg!(target_arch = "wasm32") { "Export" } else { "Share" };

        let share_button_resp = ui
            .add_enabled_ui(enable_share_button, |ui| ui.button(button_text))
            .inner;

        match url_for_current_screen {
            Err(err) => {
                let error_text = format!(
                    "Cannot create {} URL: {err}",
                    if cfg!(target_arch = "wasm32") { "export" } else { "share" }
                );
                share_button_resp.on_disabled_hover_text(error_text);
            }
            Ok(url) => {
                if share_button_resp.clicked() {
                    self.open_with_url(url);
                }
            }
        }
    }

    pub fn ui(
        &mut self,
        ctx: &ViewerContext<'_>,
        ui: &egui::Ui,
        web_viewer_base_url: Option<&url::Url>,
    ) {
        if !self.modal.is_open() {
            return;
        }
        if !cfg!(target_arch = "wasm32") && self.url.is_none() {
            return;
        }

        let modal_title = if cfg!(target_arch = "wasm32") { "Export" } else { "Share" };

        let create_web_viewer_url = &mut self.create_web_viewer_url;
        let download_feedback = &mut self.download_feedback;
        let show_copied_feedback = &mut self.show_copied_feedback;

        self.modal.ui(
            ui.ctx(),
            || ModalWrapper::new(modal_title),
            |ui| {
                ui.set_max_height((ui.ctx().screen_rect().height() - 100.0).at_least(0.0).at_most(640.0));

                if cfg!(target_arch = "wasm32") {
                    // --- MODIFICATION: START ---
                    // CORRECTED: Access startup_options through `ctx.startup_options`.
                    let startup_options = ctx.startup_options;
                    // --- MODIFICATION: END ---

                    // --- RRD Download Button ---
                    ui.add_space(8.0);
                    let rrd_button_response = ui.add_enabled_ui(startup_options.rrd_url.is_some(), |ui| {
                        let downloading = download_feedback.get("rrd").copied().unwrap_or(false);
                        let label = if downloading { "Downloading RRD..." } else { "Download RRD" };
                        if primary_button_style(ui, label).clicked() && !downloading {
                            if let Some(url) = &startup_options.rrd_url {
                                Self::download_file_from_url(url, "recording.rrd");
                                download_feedback.insert("rrd".to_string(), true);
                            }
                        }
                        if downloading { ui.ctx().request_repaint(); }
                    });
                    // --- MODIFICATION: START ---
                    // CORRECTED: Call `.response.on_disabled_hover_text`.
                    rrd_button_response.response.on_disabled_hover_text("RRD file URL not provided.");
                    // --- MODIFICATION: END ---

                    // --- Annotation Buttons ---
                    ui.add_space(12.0);
                    ui.label("Download Annotations:");

                    // --- Video Button ---
                    ui.add_space(4.0);
                    let video_button_response = ui.add_enabled_ui(startup_options.mp4_url.is_some(), |ui| {
                        let downloading = download_feedback.get("video").copied().unwrap_or(false);
                        let label = if downloading { "Downloading Video..." } else { "Download Annotations Video" };
                        if primary_button_style(ui, label).clicked() && !downloading {
                            if let Some(url) = &startup_options.mp4_url {
                                Self::download_file_from_url(url, "annotations.mp4");
                                download_feedback.insert("video".to_string(), true);
                            }
                        }
                        if downloading { ui.ctx().request_repaint(); }
                    });
                    video_button_response.response.on_disabled_hover_text("Video file URL not provided.");

                    // --- CSV Button ---
                    ui.add_space(4.0);
                    let csv_button_response = ui.add_enabled_ui(startup_options.csv_url.is_some(), |ui| {
                        let downloading = download_feedback.get("csv").copied().unwrap_or(false);
                        let label = if downloading { "Downloading Coordinates..." } else { "Download Annotations Coordinates" };
                        if primary_button_style(ui, label).clicked() && !downloading {
                            if let Some(url) = &startup_options.csv_url {
                                Self::download_file_from_url(url, "coordinates.csv");
                                download_feedback.insert("csv".to_string(), true);
                            }
                        }
                        if downloading { ui.ctx().request_repaint(); }
                    });
                    csv_button_response.response.on_disabled_hover_text("Coordinates file URL not provided.");

                    // --- JSON Button ---
                    ui.add_space(4.0);
                    let json_button_response = ui.add_enabled_ui(startup_options.json_url.is_some(), |ui| {
                        let downloading = download_feedback.get("json").copied().unwrap_or(false);
                        let label = if downloading { "Downloading Actions..." } else { "Download Annotations Actions" };
                        if primary_button_style(ui, label).clicked() && !downloading {
                            if let Some(url) = &startup_options.json_url {
                                Self::download_file_from_url(url, "actions.json");
                                download_feedback.insert("json".to_string(), true);
                            }
                        }
                        if downloading { ui.ctx().request_repaint(); }
                    });
                    json_button_response.response.on_disabled_hover_text("Actions file URL not provided.");

                } else {
                    // --- NATIVE SHARE BUTTON ---
                    if let Some(url) = &mut self.url {
                        let url_string = {
                            let web_viewer_base_url = if *create_web_viewer_url { web_viewer_base_url } else { None };
                            let url_string = url.sharable_url(web_viewer_base_url).unwrap_or_default();
                            let mut url_for_text_edit = url_string.clone();
                            ui.add(
                                egui::TextEdit::singleline(&mut url_for_text_edit)
                                    .desired_width(f32::INFINITY)
                            );
                            url_string
                        };
                        let label: egui::WidgetText = if *show_copied_feedback { "Copied!".into() } else { "Copy link".into() };
                        let copy_link_response = primary_button_style(ui, label);

                        if copy_link_response.clicked() {
                            ui.ctx().copy_text(url_string.clone());
                            *show_copied_feedback = true;
                        }
                        if *show_copied_feedback && !copy_link_response.hovered() {
                            *show_copied_feedback = false;
                        }
                    }
                }

                if !cfg!(target_arch = "wasm32") {
                    if let Some(url) = &mut self.url {
                        ui.add_space(12.0);
                        ui.list_item_scope("share_dialog_url_settings", |ui| {
                            url_settings_ui(ctx, ui, url, create_web_viewer_url);
                        });
                    }
                }
            },
        );
    }
}


// --- Rest of the helper functions (unchanged and correct) ---

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
    let link_format_text = if cfg!(target_arch = "wasm32") { "Export format" } else { "Link format" };

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
    let time_ctrl = rec_cfg.time_ctrl.read();
    let current_time_range_selection = time_ctrl
        .loop_selection()
        .map(|range| re_uri::TimeSelection {
            timeline: *time_ctrl.timeline(),
            range: AbsoluteTimeRange::new(range.min.floor(), range.max.ceil()),
        });

    let mut entire_range = url_time_range.is_none();
    let trim_text = if cfg!(target_arch = "wasm32") { "Export range" } else { "Trim range" };

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

            selectable_value_with_min_width(ui, MIN_TOGGLE_WIDTH_RH, &mut entire_range, true, entire_text)
                .on_hover_text(entire_tooltip);
            ui.add_enabled_ui(current_time_range_selection.is_some(), |ui| {
                selectable_value_with_available_width(ui, &mut entire_range, false, trim_text)
                    .on_disabled_hover_text("No time range selected.")
                    .on_hover_text(trim_tooltip);
            });
        });
    }));

    *url_time_range = if entire_range { None } else { current_time_range_selection };
}

fn time_cursor_ui(
    ui: &mut egui::Ui,
    fragments: &mut Fragment,
    timestamp_format: re_log_types::TimestampFormat,
    rec_cfg: &RecordingConfig,
) {
    let time_ctrl = rec_cfg.time_ctrl.read();
    let current_time_cursor = time_ctrl.time_cell().map(|cell| (*time_ctrl.timeline().name(), cell));
    let mut any_time = fragments.when.is_some();

    ui.list_item_flat_noninteractive(PropertyContent::new("Time cursor").value_fn(|ui, _| {
        ui.selectable_toggle(|ui| {
            selectable_value_with_min_width(ui, MIN_TOGGLE_WIDTH_RH, &mut any_time, false, "At the start");
            ui.add_enabled_ui(current_time_cursor.is_some(), |ui| {
                let mut label = egui::Atoms::new(egui::Atom::from("Current"));
                if let Some((_, time_cell)) = current_time_cursor {
                    label.push_right(egui::RichText::new(time_cell.format(timestamp_format)).weak().small().atom_shrink(true));
                }
                label.push_left(egui::Atom::grow());
                label.push_right(egui::Atom::grow());

                selectable_value_with_available_width(ui, &mut any_time, true, label)
                    .on_disabled_hover_text("No time selected.");
            });
        });
    }));

    fragments.when = if any_time { current_time_cursor } else { None };
}
